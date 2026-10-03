//! The writing assistant's settings (S12.1b; DESIGN.md §5.5): which provider this machine uses, where
//! its key is, and which project folders the person has switched the assistant on for.
//!
//! Owns reading and writing `assistant.json` in the app's own config folder, the keychain calls
//! for the key, and the one deliberate network call here, *Test the connection*. The commands at
//! the bottom are thin; everything that decides anything is a function over [`AssistantSettings`]
//! and a [`KeyStore`], so the tests below run with a temp folder and an in-memory key store.
//!
//! **What it must never do:**
//!
//! - Never put anything in a project. The opt-in is per machine *and* per folder, and it lives
//!   here, outside the source tree, for the reason `consent.rs` gives: anything a project carries
//!   can arrive with a clone, and a coauthor who never agreed must not find the assistant on.
//! - Never give a key back. It goes in through [`save_provider`], into the keychain, and the only
//!   thing the window ever hears is whether one exists ([`AssistantStatus::has_key`]).
//! - Never send anything on its own. [`test_connection`] is the only request in this file and runs
//!   only when the person presses the button; opening a project, saving settings and asking for the
//!   status make none.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use abstract_tex_assistant::{
    build_request, Assistant, AssistantError, KeyStore, Message, Prompt, Provider, SystemPart,
};
use serde::{Deserialize, Serialize};

use crate::project::write_atomically;

/// What this machine has chosen. A list of folders, so a person reading the file sees exactly where
/// the assistant is on.
#[derive(Debug, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    provider: Option<Provider>,
    #[serde(default)]
    enabled: Vec<String>,
    /// Show the exact request and wait for a click before it is sent. On unless the person turned
    /// it off: a file without the field is a file written before the inspector existed.
    #[serde(default = "inspect_first_by_default")]
    inspect_first: bool,
}

fn inspect_first_by_default() -> bool {
    true
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            provider: None,
            enabled: Vec::new(),
            inspect_first: inspect_first_by_default(),
        }
    }
}

/// Where the settings are kept. One per app; handed to commands as Tauri state.
#[derive(Debug)]
pub struct AssistantSettings {
    file: PathBuf,
}

/// The key store, as Tauri state. `Arc<dyn KeyStore>` so a test can hand in the in-memory one and
/// the real app the OS keychain, and the commands cannot tell which.
pub struct AssistantKeys(pub Arc<dyn KeyStore>);

/// What the window may know. Never the key itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    pub provider: Option<Provider>,
    /// A key is saved for the current provider.
    pub has_key: bool,
    /// The assistant is switched on for the open project, on this machine.
    pub enabled: bool,
    /// Each request is shown, exactly as it will be sent, and waits for a click (S13.3).
    pub inspect_first: bool,
}

impl AssistantSettings {
    pub fn at(file: PathBuf) -> Self {
        Self { file }
    }

    /// Read every time, so editing the file by hand takes effect. A file that does not parse is
    /// "nothing chosen": the safe reading, and the next save replaces it.
    fn read(&self) -> SettingsFile {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, settings: &SettingsFile) -> Result<(), String> {
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder)
                .map_err(|error| format!("Could not save the assistant's settings: {error}"))?;
        }
        let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
        write_atomically(&self.file, &text)
            .map_err(|error| format!("Could not save the assistant's settings: {error}"))
    }

    pub fn provider(&self) -> Option<Provider> {
        self.read().provider
    }

    pub fn inspect_first(&self) -> bool {
        self.read().inspect_first
    }

    fn is_enabled_for(&self, project_dir: &Path) -> bool {
        self.read().enabled.contains(&key_for(project_dir))
    }
}

/// The folder as the entry is written; see `consent.rs` — a copy elsewhere does not inherit it.
fn key_for(project_dir: &Path) -> String {
    project_dir.to_string_lossy().into_owned()
}

fn sentence(error: AssistantError) -> String {
    error.to_string()
}

/// A request that checks nothing but the address and the model: built, never sent.
fn probe_prompt() -> Prompt {
    Prompt {
        system: vec![SystemPart::plain("Reply with the single word OK.")],
        messages: vec![Message::user("Are you there?")],
        max_tokens: 16,
    }
}

/// The settings as the window sees them, for the open project (or none).
pub fn status(
    settings: &AssistantSettings,
    keys: &dyn KeyStore,
    project_dir: Option<&Path>,
) -> Result<AssistantStatus, String> {
    let provider = settings.provider();
    let has_key = match &provider {
        Some(provider) => keys.read(&provider.key_slot()).map_err(sentence)?.is_some(),
        None => false,
    };
    let enabled = project_dir.is_some_and(|dir| settings.is_enabled_for(dir));
    Ok(AssistantStatus {
        provider,
        has_key,
        enabled,
        inspect_first: settings.inspect_first(),
    })
}

/// Choose a provider and, if one is given, keep its key in the keychain. Refuses an address the
/// assistant could not use — and a model with no name — before anything is saved.
pub fn save_provider(
    settings: &AssistantSettings,
    keys: &dyn KeyStore,
    provider: Provider,
    key: Option<String>,
) -> Result<(), String> {
    if provider.model().trim().is_empty() {
        return Err("Choose a model: the provider needs to be told which one to use.".to_string());
    }
    // Build, never send, with a stand-in key: this is how a bad or insecure address is caught now
    // rather than at the first question.
    build_request(&provider, Some("placeholder"), &probe_prompt()).map_err(sentence)?;

    let key = key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    if let Some(key) = &key {
        keys.save(&provider.key_slot(), key).map_err(sentence)?;
    }
    let mut file = settings.read();
    file.provider = Some(provider);
    settings.write(&file)
}

/// Forget the current provider's key. Not an error when there is none.
pub fn clear_key(settings: &AssistantSettings, keys: &dyn KeyStore) -> Result<(), String> {
    match settings.provider() {
        Some(provider) => keys.clear(&provider.key_slot()).map_err(sentence),
        None => Ok(()),
    }
}

/// Switch the assistant on or off for one project folder on this machine.
pub fn set_enabled(settings: &AssistantSettings, project_dir: &Path, enabled: bool) -> Result<(), String> {
    let mut file = settings.read();
    let key = key_for(project_dir);
    file.enabled.retain(|existing| *existing != key);
    if enabled {
        file.enabled.push(key);
    }
    settings.write(&file)
}

/// Choose whether each request is shown before it is sent.
pub fn set_inspect_first(settings: &AssistantSettings, inspect_first: bool) -> Result<(), String> {
    let mut file = settings.read();
    file.inspect_first = inspect_first;
    settings.write(&file)
}

/// The one request this file makes: a few tokens to the chosen provider, with the saved key, to say
/// whether the address, the model and the key work. Blocking; the command runs it off the UI thread.
pub fn test_connection(settings: &AssistantSettings, keys: &dyn KeyStore) -> Result<String, String> {
    let provider = settings
        .provider()
        .ok_or_else(|| "Choose a provider first, then test the connection.".to_string())?;
    let reply = Assistant::new()
        .and_then(|assistant| assistant.complete(&provider, keys, &probe_prompt()))
        .map_err(sentence)?;
    let words: String = reply.text.trim().chars().take(80).collect();
    Ok(format!("{} answered: \u{201c}{words}\u{201d}.", provider.model()))
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

use tauri::State;

/// The commands' result: a sentence on failure, as everywhere in `commands.rs`.
type CommandResult<T> = Result<T, String>;
use crate::AppState;

fn open_project_dir(state: &AppState) -> Option<PathBuf> {
    state
        .project
        .lock()
        .unwrap()
        .as_ref()
        .map(|project| project.root_dir.clone())
}

#[tauri::command]
pub fn assistant_status(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
) -> CommandResult<AssistantStatus> {
    status(&settings, keys.0.as_ref(), open_project_dir(&state).as_deref())
}

#[tauri::command]
pub fn assistant_save_provider(
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    provider: Provider,
    key: Option<String>,
) -> CommandResult<()> {
    save_provider(&settings, keys.0.as_ref(), provider, key)
}

#[tauri::command]
pub fn assistant_clear_key(
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
) -> CommandResult<()> {
    clear_key(&settings, keys.0.as_ref())
}

#[tauri::command]
pub fn assistant_set_enabled(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    enabled: bool,
) -> CommandResult<()> {
    let project_dir = open_project_dir(&state).ok_or_else(|| "No project is open.".to_string())?;
    set_enabled(&settings, &project_dir, enabled)
}

/// Show each request before it is sent, or stop doing so. Per machine, not per project.
#[tauri::command]
pub fn assistant_set_inspect_first(
    settings: State<'_, AssistantSettings>,
    inspect_first: bool,
) -> CommandResult<()> {
    set_inspect_first(&settings, inspect_first)
}

/// Test the connection: the only request the assistant's settings ever make, and only on a click.
#[tauri::command]
pub async fn assistant_test(
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
) -> CommandResult<String> {
    // `spawn_blocking` because the HTTP client is blocking, as for the GitHub calls. The state
    // handles are cheap to clone out: the file path, and the `Arc` around the key store.
    let settings = AssistantSettings::at(settings.file.clone());
    let keys = Arc::clone(&keys.0);
    tauri::async_runtime::spawn_blocking(move || test_connection(&settings, keys.as_ref()))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use abstract_tex_assistant::MemoryKeyStore;

    fn settings() -> (tempfile::TempDir, AssistantSettings) {
        let config = tempfile::tempdir().unwrap();
        let settings = AssistantSettings::at(config.path().join("app").join("assistant.json"));
        (config, settings)
    }

    fn local_model() -> Provider {
        Provider::openai_compatible("http://localhost:11434/v1", "llama3")
    }

    #[test]
    fn nothing_is_chosen_and_nothing_is_on_until_someone_says_so() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let status = status(&settings, &keys, Some(Path::new("/work/thesis"))).unwrap();
        assert_eq!(
            status,
            AssistantStatus {
                provider: None,
                has_key: false,
                enabled: false,
                inspect_first: true
            }
        );
    }

    #[test]
    fn a_provider_and_its_key_are_saved_and_the_key_is_not_in_the_file() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let provider = Provider::anthropic("claude-x");
        save_provider(
            &settings,
            &keys,
            provider.clone(),
            Some("  sk-secret-key  ".into()),
        )
        .unwrap();

        let shown = status(&settings, &keys, None).unwrap();
        assert_eq!(shown.provider, Some(provider.clone()));
        assert!(shown.has_key);
        assert_eq!(
            keys.read("anthropic").unwrap().as_deref(),
            Some("sk-secret-key"),
            "trimmed, in the keychain"
        );

        let file = fs::read_to_string(&settings.file).unwrap();
        assert!(
            !file.contains("sk-secret-key"),
            "the key must never be in the settings file: {file}"
        );
        assert!(!serde_json::to_string(&shown).unwrap().contains("sk-secret-key"));
    }

    #[test]
    fn saving_without_a_key_keeps_the_one_already_saved() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-x"),
            Some("sk-one".into()),
        )
        .unwrap();
        save_provider(&settings, &keys, Provider::anthropic("claude-y"), None).unwrap();
        save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-z"),
            Some("  ".into()),
        )
        .unwrap();
        assert_eq!(keys.read("anthropic").unwrap().as_deref(), Some("sk-one"));
        assert_eq!(settings.provider().unwrap().model(), "claude-z");
    }

    #[test]
    fn a_bad_address_a_blank_model_and_an_insecure_remote_are_refused_and_nothing_is_saved() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        for (provider, wanted) in [
            (Provider::openai_compatible("not an address", "m"), "web address"),
            (
                Provider::openai_compatible("http://models.example.com/v1", "m"),
                "plain http",
            ),
            (
                Provider::openai_compatible("http://localhost:1/v1", "  "),
                "Choose a model",
            ),
        ] {
            let refusal = save_provider(&settings, &keys, provider, Some("sk-x".into())).unwrap_err();
            assert!(refusal.contains(wanted), "{refusal}");
        }
        assert!(settings.provider().is_none());
        assert_eq!(
            keys.read("openai-compatible@localhost:1").unwrap(),
            None,
            "no key is kept for a refused provider"
        );
    }

    #[test]
    fn a_local_model_needs_no_key_and_clearing_a_key_is_repeatable() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        save_provider(&settings, &keys, local_model(), None).unwrap();
        assert!(!status(&settings, &keys, None).unwrap().has_key);

        save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-x"),
            Some("sk-one".into()),
        )
        .unwrap();
        assert!(status(&settings, &keys, None).unwrap().has_key);
        clear_key(&settings, &keys).unwrap();
        clear_key(&settings, &keys).unwrap();
        assert!(!status(&settings, &keys, None).unwrap().has_key);
    }

    #[test]
    fn requests_are_shown_first_until_the_person_says_otherwise_and_an_old_file_means_yes() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        assert!(status(&settings, &keys, None).unwrap().inspect_first);

        // A settings file written before this option existed.
        fs::create_dir_all(settings.file.parent().unwrap()).unwrap();
        fs::write(&settings.file, r#"{"enabled":["/work/thesis"]}"#).unwrap();
        assert!(
            settings.inspect_first(),
            "an absent field must not switch the inspector off"
        );

        set_inspect_first(&settings, false).unwrap();
        assert!(!status(&settings, &keys, None).unwrap().inspect_first);
        assert!(
            settings.is_enabled_for(Path::new("/work/thesis")),
            "the other choices are kept"
        );
        set_inspect_first(&settings, true).unwrap();
        assert!(settings.inspect_first());
    }

    #[test]
    fn the_switch_is_per_project_folder_and_can_be_turned_off() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let thesis = Path::new("/work/thesis");
        set_enabled(&settings, thesis, true).unwrap();
        set_enabled(&settings, thesis, true).unwrap();

        assert!(status(&settings, &keys, Some(thesis)).unwrap().enabled);
        assert!(
            !status(&settings, &keys, Some(Path::new("/work/thesis-clone")))
                .unwrap()
                .enabled,
            "a copy elsewhere starts off"
        );
        assert!(
            !status(&settings, &keys, Some(Path::new("/work")))
                .unwrap()
                .enabled
        );
        assert!(!status(&settings, &keys, None).unwrap().enabled);
        assert_eq!(
            fs::read_to_string(&settings.file)
                .unwrap()
                .matches("/work/thesis")
                .count(),
            1,
            "listed once"
        );

        set_enabled(&settings, thesis, false).unwrap();
        assert!(!status(&settings, &keys, Some(thesis)).unwrap().enabled);
    }

    #[test]
    fn choosing_a_provider_does_not_switch_any_project_on() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        save_provider(&settings, &keys, local_model(), None).unwrap();
        assert!(
            !status(&settings, &keys, Some(Path::new("/work/thesis")))
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn a_settings_file_that_does_not_parse_means_nothing_chosen_and_is_replaced_by_the_next_save() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        fs::create_dir_all(settings.file.parent().unwrap()).unwrap();
        fs::write(&settings.file, "{ this is not json").unwrap();
        assert_eq!(
            status(&settings, &keys, Some(Path::new("/w"))).unwrap().provider,
            None
        );
        save_provider(&settings, &keys, local_model(), None).unwrap();
        assert!(settings.provider().is_some());
    }

    #[test]
    fn testing_with_no_provider_says_what_to_do_and_connects_to_nothing() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let sentence = test_connection(&settings, &keys).unwrap_err();
        assert!(sentence.contains("Choose a provider"), "{sentence}");
    }

    #[test]
    fn a_missing_key_is_said_before_any_request() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        save_provider(&settings, &keys, Provider::anthropic("claude-x"), None).unwrap();
        let sentence = test_connection(&settings, &keys).unwrap_err();
        assert!(sentence.contains("needs an API key"), "{sentence}");
    }
}
