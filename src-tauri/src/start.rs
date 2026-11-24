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

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::watch;

use crate::commands::{to_message, CommandResult};
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
    let shown = home
        .and_then(|home| parent.strip_prefix(home).ok())
        .unwrap_or(parent);
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
}
