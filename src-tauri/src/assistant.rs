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
use std::sync::{Arc, Mutex};

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

/// Held while the settings file is read, edited and written back, so two changes made at the same moment
/// cannot lose each other's edit. It is one lock for the whole process, not one per `AssistantSettings`,
/// because a command that must move the settings into another thread makes its own copy
/// (`AssistantSettings::at`) and every copy points at the same file. It guards the file, not the
/// keychain: nothing that waits for a person is done while it is held.
static SETTINGS_CHANGE: Mutex<()> = Mutex::new(());

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

    /// Read the file, let `edit` change it, and write it back, all under [`SETTINGS_CHANGE`]. Since the
    /// keychain commands run on their own threads (review finding R5.9), a change from the settings
    /// view can overlap one from the project switch; before this, each did its own read, edit and write
    /// and the later write could drop the earlier edit, or fail because both used one temporary file.
    fn change(&self, edit: impl FnOnce(&mut SettingsFile)) -> Result<(), String> {
        // A poisoned lock means another thread panicked while holding it. The file is still whole
        // (`write` replaces it atomically), so carry on rather than refuse every later change.
        let _one_change_at_a_time = SETTINGS_CHANGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = self.read();
        edit(&mut file);
        self.write(&file)
    }

    /// The settings file's own path: what the app folder's other files for the assistant are placed
    /// beside (the explanations' memory, `assistant_explain.rs`).
    pub fn file(&self) -> &Path {
        &self.file
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

/// The settings as the window sees them, for the open project (or none). `include_key` says the
/// caller needs to know whether a key is saved even when the assistant is off for the project.
pub fn status(
    settings: &AssistantSettings,
    keys: &dyn KeyStore,
    project_dir: Option<&Path>,
    include_key: bool,
) -> Result<AssistantStatus, String> {
    let provider = settings.provider();
    let enabled = project_dir.is_some_and(|dir| settings.is_enabled_for(dir));
    // The keychain may ask the person for a password, and this runs on the main thread. So it is read
    // only when the answer is used: the settings view says whether a key is saved (`include_key`), and
    // a project the assistant is on for needs it to know whether the assistant is ready. Opening any
    // other project does not touch it (review finding R5.9); `has_key` is then `false`, not "known".
    let has_key = match &provider {
        Some(provider) if include_key || enabled => {
            keys.read(&provider.key_slot()).map_err(sentence)?.is_some()
        }
        _ => false,
    };
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
    settings.change(|file| file.provider = Some(provider))
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
    let key = key_for(project_dir);
    settings.change(|file| {
        file.enabled.retain(|existing| *existing != key);
        if enabled {
            file.enabled.push(key);
        }
    })
}

/// Choose whether each request is shown before it is sent.
pub fn set_inspect_first(settings: &AssistantSettings, inspect_first: bool) -> Result<(), String> {
    settings.change(|file| file.inspect_first = inspect_first)
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

/// Run `work` on one of Tauri's threads for blocking calls and wait for it, without holding up the
/// window.
///
/// The three commands below talk to the OS keychain, which can stop to ask the person for a password
/// or take a while to answer. A plain `#[tauri::command]` function runs on the thread that also
/// draws the window, so the whole window would freeze until the prompt was answered (review finding
/// R5.9). An `async` command is handed to Tauri's own pool instead, and `spawn_blocking` is the
/// pool's place for work that waits rather than computes. A `State<'_, T>` borrows from the app and
/// cannot go into the closure, so each command copies out what it needs first (`work` is `'static`).
async fn on_blocking_thread<T, F>(work: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CommandResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn assistant_status(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    include_key: bool,
) -> CommandResult<AssistantStatus> {
    let project_dir = open_project_dir(&state);
    let settings = AssistantSettings::at(settings.file.clone());
    let keys = Arc::clone(&keys.0);
    on_blocking_thread(move || status(&settings, keys.as_ref(), project_dir.as_deref(), include_key)).await
}

#[tauri::command]
pub async fn assistant_save_provider(
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    provider: Provider,
    key: Option<String>,
) -> CommandResult<()> {
    let settings = AssistantSettings::at(settings.file.clone());
    let keys = Arc::clone(&keys.0);
    on_blocking_thread(move || save_provider(&settings, keys.as_ref(), provider, key)).await
}

#[tauri::command]
pub async fn assistant_clear_key(
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
) -> CommandResult<()> {
    let settings = AssistantSettings::at(settings.file.clone());
    let keys = Arc::clone(&keys.0);
    on_blocking_thread(move || clear_key(&settings, keys.as_ref())).await
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
    // Off the window's thread because the HTTP client is blocking, as for the GitHub calls. The state
    // handles are cheap to clone out: the file path, and the `Arc` around the key store.
    let settings = AssistantSettings::at(settings.file.clone());
    let keys = Arc::clone(&keys.0);
    on_blocking_thread(move || test_connection(&settings, keys.as_ref())).await
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

    /// A key store that counts how often it is asked: the OS keychain can prompt for a password.
    #[derive(Default)]
    struct CountingKeys {
        inner: MemoryKeyStore,
        reads: std::sync::atomic::AtomicUsize,
    }

    impl KeyStore for CountingKeys {
        fn save(&self, slot: &str, key: &str) -> Result<(), AssistantError> {
            self.inner.save(slot, key)
        }
        fn read(&self, slot: &str) -> Result<Option<String>, AssistantError> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.read(slot)
        }
        fn clear(&self, slot: &str) -> Result<(), AssistantError> {
            self.inner.clear(slot)
        }
    }

    // Review finding R5.9. Opening any project asked the keychain (on the main thread) once a provider
    // had ever been chosen, for a feature that is off for that project. Where the keychain asks for a
    // password, that prompt came with every project the author opened.
    #[test]
    fn the_keychain_is_asked_only_when_the_answer_is_needed() {
        let (_config, settings) = settings();
        let keys = CountingKeys::default();
        save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-x"),
            Some("sk-one".into()),
        )
        .unwrap();
        let reads = || keys.reads.load(std::sync::atomic::Ordering::SeqCst);
        let project = Path::new("/work/thesis");

        // Opening a project the assistant is off for: no read.
        let before = reads();
        let shown = status(&settings, &keys, Some(project), false).unwrap();
        assert_eq!(
            reads(),
            before,
            "a project the assistant is off for must not cost a keychain read"
        );
        assert!(!shown.has_key, "unread, so not claimed");

        // The settings view asks for it.
        let shown = status(&settings, &keys, Some(project), true).unwrap();
        assert!(shown.has_key);
        assert_eq!(reads(), before + 1);

        // A project it is on for needs the key to say whether the assistant is ready.
        set_enabled(&settings, project, true).unwrap();
        let shown = status(&settings, &keys, Some(project), false).unwrap();
        assert!(shown.has_key);
        assert_eq!(reads(), before + 2);
    }

    #[test]
    fn nothing_is_chosen_and_nothing_is_on_until_someone_says_so() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let status = status(&settings, &keys, Some(Path::new("/work/thesis")), true).unwrap();
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

        let shown = status(&settings, &keys, None, true).unwrap();
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
        assert!(!status(&settings, &keys, None, true).unwrap().has_key);

        save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-x"),
            Some("sk-one".into()),
        )
        .unwrap();
        assert!(status(&settings, &keys, None, true).unwrap().has_key);
        clear_key(&settings, &keys).unwrap();
        clear_key(&settings, &keys).unwrap();
        assert!(!status(&settings, &keys, None, true).unwrap().has_key);
    }

    #[test]
    fn requests_are_shown_first_until_the_person_says_otherwise_and_an_old_file_means_yes() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        assert!(status(&settings, &keys, None, true).unwrap().inspect_first);

        // A settings file written before this option existed.
        fs::create_dir_all(settings.file.parent().unwrap()).unwrap();
        fs::write(&settings.file, r#"{"enabled":["/work/thesis"]}"#).unwrap();
        assert!(
            settings.inspect_first(),
            "an absent field must not switch the inspector off"
        );

        set_inspect_first(&settings, false).unwrap();
        assert!(!status(&settings, &keys, None, true).unwrap().inspect_first);
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

        assert!(status(&settings, &keys, Some(thesis), true).unwrap().enabled);
        assert!(
            !status(&settings, &keys, Some(Path::new("/work/thesis-clone")), true)
                .unwrap()
                .enabled,
            "a copy elsewhere starts off"
        );
        assert!(
            !status(&settings, &keys, Some(Path::new("/work")), true)
                .unwrap()
                .enabled
        );
        assert!(!status(&settings, &keys, None, true).unwrap().enabled);
        assert_eq!(
            fs::read_to_string(&settings.file)
                .unwrap()
                .matches("/work/thesis")
                .count(),
            1,
            "listed once"
        );

        set_enabled(&settings, thesis, false).unwrap();
        assert!(!status(&settings, &keys, Some(thesis), true).unwrap().enabled);
    }

    #[test]
    fn choosing_a_provider_does_not_switch_any_project_on() {
        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        save_provider(&settings, &keys, local_model(), None).unwrap();
        assert!(
            !status(&settings, &keys, Some(Path::new("/work/thesis")), true)
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
            status(&settings, &keys, Some(Path::new("/w")), true)
                .unwrap()
                .provider,
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

    /// §8's "no silent data paths", for this file: every function here except the Test button's
    /// opens no socket. The provider's address is a listener on this computer, so a connection
    /// would be seen.
    #[test]
    fn choosing_saving_switching_on_and_asking_for_status_connect_to_nothing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());

        let (_config, settings) = settings();
        let keys = MemoryKeyStore::default();
        let provider = Provider::Anthropic {
            model: "claude-x".into(),
            address: address.clone(),
        };
        let project = Path::new("/work/thesis");

        // With a key and the project switched on: everything short of the Test button.
        save_provider(&settings, &keys, provider.clone(), Some("sk-x".into())).unwrap();
        set_enabled(&settings, project, true).unwrap();
        set_inspect_first(&settings, false).unwrap();
        let _ = status(&settings, &keys, Some(project), true).unwrap();
        clear_key(&settings, &keys).unwrap();

        // And the Test button with no key: refused before a socket exists.
        let sentence = test_connection(&settings, &keys).unwrap_err();
        assert!(sentence.contains("needs an API key"), "{sentence}");

        std::thread::sleep(std::time::Duration::from_millis(100));
        match listener.accept() {
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
            Ok(_) => panic!("something connected without being asked to"),
        }
    }

    // Found in the self-review of the keychain change (`R5.9`). Changing a setting reads the file,
    // edits it and writes it back. While every command ran on the window's one thread, two of them could
    // never overlap; now `assistant_save_provider` runs on another, so a change made at the same moment
    // could be lost. Eight threads each switch the assistant on for twenty projects; all 160 must be in
    // the file afterwards, and none of the writes may fail.
    #[test]
    fn settings_changed_from_several_threads_at_once_do_not_lose_each_others_changes() {
        let (_config, settings) = settings();
        std::thread::scope(|scope| {
            for thread in 0..8 {
                let settings = &settings;
                scope.spawn(move || {
                    for project in 0..20 {
                        let folder = PathBuf::from(format!("/projects/{thread}/{project}"));
                        set_enabled(settings, &folder, true).unwrap();
                    }
                });
            }
        });
        assert_eq!(settings.read().enabled.len(), 8 * 20, "a change was lost");
    }

    /// A key store that notes which thread each call arrived on. The OS keychain can stop to ask the
    /// person for a password, so a call made on the thread that handles the window's events freezes
    /// the window until they answer.
    #[derive(Default)]
    struct ThreadNotingKeys {
        inner: MemoryKeyStore,
        calls: std::sync::Mutex<Vec<(&'static str, std::thread::ThreadId)>>,
    }

    impl ThreadNotingKeys {
        fn note(&self, call: &'static str) {
            self.calls
                .lock()
                .unwrap()
                .push((call, std::thread::current().id()));
        }
    }

    impl KeyStore for ThreadNotingKeys {
        fn save(&self, slot: &str, key: &str) -> Result<(), AssistantError> {
            self.note("save");
            self.inner.save(slot, key)
        }
        fn read(&self, slot: &str) -> Result<Option<String>, AssistantError> {
            self.note("read");
            self.inner.read(slot)
        }
        fn clear(&self, slot: &str) -> Result<(), AssistantError> {
            self.note("clear");
            self.inner.clear(slot)
        }
    }

    // Review finding R5.9, the part it left open. A plain `#[tauri::command]` function runs on the
    // thread that received the message from the webview, which is the one that also draws the window.
    // `async` commands are handed to Tauri's own pool. This test uses Tauri's mock runtime (the
    // `test` feature, dev-dependency only): a real command dispatcher with no window on screen. There
    // a sync command runs on the calling thread and an async one does not, which is the same
    // difference the real window sees. Each of the three commands that touch the keychain must come
    // in on another thread. What this does not pin: that the work then goes through `spawn_blocking`
    // (see `on_blocking_thread`) and not inline on one of the pool's async threads. Both leave the
    // window alone; the first is the pool's convention for calls that wait, and no test can tell them apart.
    #[test]
    fn the_commands_that_touch_the_keychain_do_not_run_on_the_thread_that_called_them() {
        let keys = Arc::new(ThreadNotingKeys::default());
        let config = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_builder()
            .manage(AppState::new())
            .manage(AssistantSettings::at(config.path().join("assistant.json")))
            .manage(AssistantKeys(Arc::clone(&keys) as Arc<dyn KeyStore>))
            .invoke_handler(tauri::generate_handler![
                assistant_status,
                assistant_save_provider,
                assistant_clear_key
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let invoke = |command: &str, arguments: serde_json::Value| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: command.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(arguments),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
            .unwrap_or_else(|error| panic!("{command} failed: {error}"))
        };

        invoke(
            "assistant_save_provider",
            serde_json::json!({
                "provider": { "kind": "openAiCompatible", "model": "llama3", "address": "http://localhost:11434/v1" },
                "key": "secret"
            }),
        );
        invoke("assistant_status", serde_json::json!({ "includeKey": true }));
        invoke("assistant_clear_key", serde_json::json!({}));

        let this_thread = std::thread::current().id();
        let calls = keys.calls.lock().unwrap();
        let names: Vec<&str> = calls.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            ["save", "read", "clear"],
            "each command should reach the key store once"
        );
        for (name, thread) in calls.iter() {
            assert_ne!(
                *thread, this_thread,
                "{name} ran on the thread that invoked the command"
            );
        }
    }
}
