//! The start screen's commands (S17.3 onwards). Thin, like `commands.rs`: the work is in `recents` and
//! its neighbours. Answers are data, never sentences: the words are the frontend's (`src/lib/words.ts`).
//!
//! **What it must never do:** wait for a disk on the window's thread, or on an async worker. Every
//! folder named here may be on a drive that stopped answering, so reading one goes through
//! `spawn_blocking`, and a check through [`PresenceChecks`], which also gives up.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::watch;

use crate::assistant::AssistantSettings;
use crate::commands::{to_message, CommandResult};
use crate::consent::ShellEscapeConsent;
use crate::preferences::{default_place, Colours, Preferences, PreferencesFile, TEXT_SIZES};
use crate::recents::{self, RecentDocument, RecentDocuments};
use crate::AppState;

/// How long a folder has to answer whether it is there.
const PRESENCE_LIMIT: Duration = Duration::from_secs(2);

/// One recent document as the window draws it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentRow {
    folder: String,
    root_file: String,
    title: String,
    /// Where it is kept, from the home folder down: the window puts a sign between the parts.
    place: Vec<String>,
    used_at: i64,
}

/// The folder a document's folder is in, as the names a person would read. Inside the home folder that
/// folder itself is left out, so a document directly in it has no parts at all.
pub fn place_parts(folder: &Path, home: Option<&Path>) -> Vec<String> {
    let parent = folder.parent().unwrap_or(folder);
    folder_parts(parent, home)
}

/// A folder itself, as the names a person would read, from the home folder down.
fn folder_parts(folder: &Path, home: Option<&Path>) -> Vec<String> {
    let shown = home
        .and_then(|home| folder.strip_prefix(home).ok())
        .unwrap_or(folder);
    names(shown)
}

fn names(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            Component::Prefix(drive) => Some(drive.as_os_str().to_string_lossy().into_owned()),
            _ => None, // the root's own separator
        })
        .collect()
}

/// The list as the window draws it. Two documents that would look the same show their folders too.
fn rows(documents: Vec<RecentDocument>, home: Option<&Path>) -> Vec<RecentRow> {
    let mut rows: Vec<RecentRow> = documents
        .into_iter()
        .map(|document| RecentRow {
            place: place_parts(Path::new(&document.folder), home),
            folder: document.folder,
            root_file: document.root_file,
            title: document.title,
            used_at: document.used_at,
        })
        .collect();
    let mut alike: HashMap<(String, Vec<String>), usize> = HashMap::new();
    for row in &rows {
        *alike.entry((row.title.clone(), row.place.clone())).or_default() += 1;
    }
    for row in &mut rows {
        if alike[&(row.title.clone(), row.place.clone())] > 1 {
            row.place.push(recents::folder_name(Path::new(&row.folder)));
        }
    }
    rows
}

/// Asks whether folders are there, on threads of their own, and stops waiting after
/// [`PRESENCE_LIMIT`]. A thread stuck on a dead drive cannot be ended, so a folder that is still being
/// asked about is never asked about again: a second caller waits for the first one's answer. Without
/// that, every return to Home would leave one more thread behind.
#[derive(Default)]
pub struct PresenceChecks {
    /// A `watch` channel holds one value that changes and that any number of callers can wait on.
    running: Mutex<HashMap<String, watch::Receiver<Option<bool>>>>,
}

impl PresenceChecks {
    async fn answer(&self, folder: &str, check: impl FnOnce() -> bool + Send + 'static) -> bool {
        let mut answer = {
            let mut running = self.running.lock().unwrap();
            // Over when it has answered, or when its thread is gone without answering.
            running.retain(|_, answer| answer.borrow().is_none() && answer.has_changed().is_ok());
            match running.get(folder) {
                Some(answer) => answer.clone(),
                None => {
                    let (say, answer) = watch::channel(None);
                    running.insert(folder.to_string(), answer.clone());
                    tauri::async_runtime::spawn_blocking(move || say.send(Some(check())));
                    answer
                }
            }
        };
        let answered = tokio::time::timeout(PRESENCE_LIMIT, answer.wait_for(Option::is_some)).await;
        // The outer error is the limit, the inner one a check that panicked: not here, as far as
        // anyone can tell.
        matches!(answered, Ok(Ok(is_here)) if *is_here == Some(true))
    }
}

/// Seconds since 1970, as `used_at` counts them.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

/// Run `work` on the list, off the async workers: a config folder can be on a slow disk too.
async fn with_recents<T: Send + 'static>(
    recents: State<'_, RecentDocuments>,
    work: impl FnOnce(RecentDocuments) -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    let recents = recents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || work(recents))
        .await
        .map_err(to_message)?
}

#[tauri::command]
pub async fn recent_documents<R: Runtime>(
    app: AppHandle<R>,
    recents: State<'_, RecentDocuments>,
) -> CommandResult<Vec<RecentRow>> {
    let home = app.path().home_dir().ok();
    with_recents(recents, move |recents| Ok(rows(recents.list(), home.as_deref()))).await
}

/// Whether a listed document is where it was. One command per document, so that one dead drive delays
/// nothing but its own row.
#[tauri::command]
pub async fn recent_is_here(
    recents: State<'_, RecentDocuments>,
    checks: State<'_, PresenceChecks>,
    folder: String,
) -> CommandResult<bool> {
    let wanted = folder.clone();
    let listed = with_recents(recents, move |recents| Ok(listed_document(&recents, &wanted))).await?;
    Ok(match listed {
        Some(document) => {
            checks
                .answer(&folder, move || recents::is_present(&document))
                .await
        }
        None => false,
    })
}

#[tauri::command]
pub async fn recent_moved(
    recents: State<'_, RecentDocuments>,
    from: String,
    to: String,
) -> CommandResult<()> {
    with_recents(recents, move |recents| {
        recents
            .moved(Path::new(&from), Path::new(&to))
            .map_err(to_message)
    })
    .await
}

#[tauri::command]
pub async fn forget_recent(recents: State<'_, RecentDocuments>, folder: String) -> CommandResult<()> {
    with_recents(recents, move |recents| {
        recents.forget(Path::new(&folder)).map_err(to_message)
    })
    .await
}

#[tauri::command]
pub async fn clear_recents(recents: State<'_, RecentDocuments>) -> CommandResult<()> {
    with_recents(recents, |recents| recents.clear().map_err(to_message)).await
}

/// Show a listed document's folder in the system's file manager. Only a listed one: the window can ask
/// for any path, and this must not become a way to open every folder on the computer.
#[tauri::command]
pub async fn show_recent_in_folder(recents: State<'_, RecentDocuments>, folder: String) -> CommandResult<()> {
    with_recents(recents, move |recents| {
        let folder = listed(&recents, &folder)?;
        tauri_plugin_opener::reveal_item_in_dir(folder).map_err(to_message)
    })
    .await
}

fn listed_document(recents: &RecentDocuments, folder: &str) -> Option<RecentDocument> {
    recents
        .list()
        .into_iter()
        .find(|document| recents::same_folder(&document.folder, folder, cfg!(windows)))
}

fn listed(recents: &RecentDocuments, folder: &str) -> CommandResult<PathBuf> {
    listed_document(recents, folder)
        .map(|document| PathBuf::from(document.folder))
        .ok_or_else(|| "That document is not in the list.".to_string())
}

/// What the open document is called now: read from its main file, recorded, answered. Asked for after
/// a good build, which is when a new `\title` has just been saved.
#[tauri::command]
pub async fn document_title<R: Runtime>(app: AppHandle<R>) -> CommandResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (root_dir, root_file) = {
            let open = state.project.lock().unwrap();
            let project = open.as_ref().ok_or("No document is open.")?;
            (project.root_dir.clone(), project.root_file())
        };
        let title = record_opened(&app, &root_dir, root_file.as_deref());
        // Another folder may have been opened while this one's file was read.
        if let Some(project) = state.project.lock().unwrap().as_mut() {
            if project.root_dir == root_dir {
                project.title = title.clone();
            }
        }
        Ok(title)
    })
    .await
    .map_err(to_message)?
}

/// Record a document that is open, and answer its title. With no main file there is nothing to come
/// back to, so nothing is recorded; in a test with no list, neither (`try_state`).
pub fn record_opened<R: Runtime>(app: &AppHandle<R>, root_dir: &Path, root_file: Option<&Path>) -> String {
    let Some(root_file) = root_file else {
        return recents::folder_name(root_dir);
    };
    let read = crate::project::read_text(&root_dir.join(root_file))
        .ok()
        .and_then(|source| texwords::title_of(&source));
    let shown = match app.try_state::<RecentDocuments>() {
        Some(recents) => recents.record(root_dir, root_file, read.as_deref(), now()),
        None => Ok(read.clone().unwrap_or_else(|| recents::folder_name(root_dir))),
    };
    shown.unwrap_or_else(|error| {
        // The list is a convenience: the document opens whether or not it can be remembered.
        tracing::warn!(%error, "could not record the document");
        read.unwrap_or_else(|| recents::folder_name(root_dir))
    })
}

/// The place new documents are kept in, as the window shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaceView {
    path: String,
    parts: Vec<String>,
}

/// What the person chose, as the window needs it: the place is always a real one.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesView {
    place: PlaceView,
    author: String,
    text_size: u16,
    colours: Colours,
}

/// Only what is to change; the rest is left out.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PreferencesChange {
    place: Option<String>,
    author: Option<String>,
    text_size: Option<u16>,
    colours: Option<Colours>,
}

impl PreferencesChange {
    /// Make the change, or say what in it cannot be kept.
    fn apply(self, preferences: &PreferencesFile) -> CommandResult<Preferences> {
        if let Some(size) = self.text_size.filter(|size| !TEXT_SIZES.contains(size)) {
            return Err(format!("{size}% is not one of the text sizes."));
        }
        if let Some(place) = self
            .place
            .as_ref()
            .filter(|place| !Path::new(place).is_absolute())
        {
            return Err(format!("{place} is not a folder's whole address."));
        }
        preferences
            .change(|kept| {
                if self.place.is_some() {
                    kept.place = self.place;
                }
                if let Some(author) = self.author {
                    kept.author = author;
                }
                if let Some(size) = self.text_size {
                    kept.text_size = size;
                }
                if let Some(colours) = self.colours {
                    kept.colours = colours;
                }
            })
            .map_err(to_message)
    }
}

fn place_of<R: Runtime>(app: &AppHandle<R>, preferences: &Preferences) -> PathBuf {
    match &preferences.place {
        Some(place) => PathBuf::from(place),
        None => {
            let folders = app.path();
            // A computer with no home folder has no config folder either, and the app does not start.
            default_place(
                folders.document_dir().ok(),
                folders.home_dir().unwrap_or_default(),
            )
        }
    }
}

/// Where a new document goes: the place that was chosen, else a folder of our own in Documents.
pub fn new_documents_folder<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let preferences = match app.try_state::<PreferencesFile>() {
        Some(file) => file.read(),
        None => Preferences::default(), // a test without any
    };
    place_of(app, &preferences)
}

fn view<R: Runtime>(app: &AppHandle<R>, preferences: Preferences) -> PreferencesView {
    let place = place_of(app, &preferences);
    PreferencesView {
        place: PlaceView {
            parts: folder_parts(&place, app.path().home_dir().ok().as_deref()),
            path: place.to_string_lossy().into_owned(),
        },
        author: preferences.author,
        text_size: preferences.text_size,
        colours: preferences.colours,
    }
}

#[tauri::command]
pub async fn preferences<R: Runtime>(app: AppHandle<R>) -> CommandResult<PreferencesView> {
    tauri::async_runtime::spawn_blocking(move || {
        let kept = app.state::<PreferencesFile>().read();
        view(&app, kept)
    })
    .await
    .map_err(to_message)
}

#[tauri::command]
pub async fn set_preferences<R: Runtime>(
    app: AppHandle<R>,
    change: PreferencesChange,
) -> CommandResult<PreferencesView> {
    tauri::async_runtime::spawn_blocking(move || {
        let kept = change.apply(&app.state::<PreferencesFile>())?;
        Ok(view(&app, kept))
    })
    .await
    .map_err(to_message)?
}

/// A folder that is about to be made starts with no permission, whatever was allowed at that address
/// before: what was allowed was for a document that is gone (S17.6). Called before the folder exists, so
/// that a failure leaves nothing behind.
pub fn forget_permissions<R: Runtime>(app: &AppHandle<R>, folder: &Path) -> CommandResult<()> {
    // As `Project::open` will spell it, which is how permissions are kept.
    let folder = std::path::absolute(folder).map_err(to_message)?;
    // `try_state`: absent in a test that is not about this.
    if let Some(consent) = app.try_state::<ShellEscapeConsent>() {
        consent.disallow(&folder).map_err(to_message)?;
    }
    if let Some(assistant) = app.try_state::<AssistantSettings>() {
        assistant.forget(&folder)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_place_leaves_the_home_folder_out() {
        let home = Some(Path::new("/home/ana"));
        assert_eq!(
            place_parts(Path::new("/home/ana/Documents/Abstract-Tex/essay"), home),
            ["Documents", "Abstract-Tex"]
        );
        assert_eq!(
            place_parts(Path::new("/media/ana/USB/thesis"), home),
            ["media", "ana", "USB"]
        );
        assert!(place_parts(Path::new("/home/ana/thesis"), home).is_empty());
    }

    fn document(folder: &str, title: &str) -> RecentDocument {
        RecentDocument {
            folder: folder.into(),
            root_file: "main.tex".into(),
            title: title.into(),
            used_at: 7,
        }
    }

    #[test]
    fn two_documents_with_one_title_in_one_place_also_show_their_folders() {
        let home = Some(Path::new("/home/ana"));
        let rows = rows(
            vec![
                document("/home/ana/Documents/essay", "Essay"),
                document("/home/ana/Documents/essay-2", "Essay"),
                document("/home/ana/Documents/other", "Other"),
                document("/home/ana/Desktop/essay", "Essay"),
            ],
            home,
        );
        let places: Vec<String> = rows.iter().map(|row| row.place.join("/")).collect();
        assert_eq!(
            places,
            ["Documents/essay", "Documents/essay-2", "Documents", "Desktop"]
        );
    }

    #[test]
    fn a_check_that_never_answers_says_not_here() {
        let checks = PresenceChecks::default();
        let asked = std::time::Instant::now();
        let here = tauri::async_runtime::block_on(checks.answer("/a", || {
            std::thread::sleep(Duration::from_secs(5));
            true
        }));
        assert!(!here);
        assert!(asked.elapsed() < Duration::from_millis(2500));
    }

    #[test]
    fn a_folder_already_being_checked_is_not_checked_twice() {
        let checks = Arc::new(PresenceChecks::default());
        tauri::async_runtime::block_on(async {
            let slow = {
                let checks = Arc::clone(&checks);
                tauri::async_runtime::spawn(async move {
                    checks
                        .answer("/a", || {
                            std::thread::sleep(Duration::from_millis(300));
                            true
                        })
                        .await
                })
            };
            tokio::time::sleep(Duration::from_millis(100)).await;
            // Would panic on its own thread, and answer false, if it ran.
            assert!(checks.answer("/a", || panic!("a second check")).await);
            assert!(slow.await.unwrap());
            // It is over, so the next one asks again.
            assert!(!checks.answer("/a", || false).await);
        });
    }

    #[test]
    fn only_a_listed_document_is_shown_in_its_folder() {
        let config = tempfile::tempdir().unwrap();
        let recents = RecentDocuments::at(config.path().join("recent-documents.toml"));
        recents
            .record(Path::new("/home/ana/essay"), Path::new("main.tex"), None, 1)
            .unwrap();
        assert!(listed(&recents, "/etc").is_err());
        assert_eq!(
            listed(&recents, "/home/ana/essay").unwrap(),
            PathBuf::from("/home/ana/essay")
        );
    }

    #[test]
    fn a_change_that_makes_no_sense_changes_nothing() {
        let config = tempfile::tempdir().unwrap();
        let file = PreferencesFile::at(config.path().join("preferences.toml"));
        let change = |text: &str| {
            serde_json::from_str::<PreferencesChange>(text)
                .unwrap()
                .apply(&file)
        };
        assert!(change(r#"{"textSize": 900, "author": "Ana"}"#).is_err());
        assert!(change(r#"{"place": "Documents"}"#).is_err());
        assert_eq!(file.read(), Preferences::default());

        change(r#"{"textSize": 150}"#).unwrap();
        let kept = change(r#"{"colours": "contrast-light"}"#).unwrap();
        assert_eq!((kept.text_size, kept.colours), (150, Colours::ContrastLight));
    }

    #[test]
    fn new_documents_go_to_the_place_that_was_chosen() {
        let config = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_builder()
            .manage(PreferencesFile::at(config.path().join("preferences.toml")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        assert!(new_documents_folder(app.handle()).ends_with("Abstract-Tex"));
        let chosen = config.path().join("writing").to_string_lossy().into_owned();
        app.state::<PreferencesFile>()
            .change(|kept| kept.place = Some(chosen.clone()))
            .unwrap();
        assert_eq!(new_documents_folder(app.handle()), Path::new(&chosen));
    }
}
