//! The start screen's commands (S17.3 onwards). Thin, like `commands.rs`: the work is in `recents` and
//! its neighbours. Answers are data, never sentences: the words are the frontend's (`src/lib/words.ts`).
//!
//! **What it must never do:** wait for a disk on the window's thread, or on an async worker. Every
//! folder named here may be on a drive that stopped answering, so reading one goes through
//! `spawn_blocking`, and a check through [`PresenceChecks`], which also gives up.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use abstract_tex_import::{
    cloud_folders, copy_document, document_folder, files_named_by, free_name, has_tex, kind_of, unpack,
    CopyError, Crowd, FileKind, Limits, Question, SlowPlaces, UnpackError,
};
use abstract_tex_templates::folder_name_for_title;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _; // puts `decode` on `BASE64`
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tokio::sync::watch;

use crate::assistant::AssistantSettings;
use crate::commands::{to_message, CommandResult, OPENING_STOPPED};
use crate::consent::ShellEscapeConsent;
use crate::opening::Opening;
use crate::pictures::Pictures;
use crate::preferences::{default_place, Colours, Preferences, PreferencesFile, TEXT_SIZES};
use crate::project::{detect_root, is_a_document, one_spelling};
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
    /// The file with a picture of its first page, if there is one (S17.15).
    picture: Option<String>,
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
            picture: None, // `with_pictures`, which asks a disk
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

fn with_pictures(mut rows: Vec<RecentRow>, pictures: &Pictures) -> Vec<RecentRow> {
    for row in &mut rows {
        let picture = pictures.path_for(Path::new(&row.folder));
        row.picture = picture.is_file().then(|| picture.to_string_lossy().into_owned());
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
    let pictures = app.state::<Pictures>().inner().clone();
    with_recents(recents, move |recents| {
        Ok(with_pictures(rows(recents.list(), home.as_deref()), &pictures))
    })
    .await
}

/// Keep a picture of the first page of a document on the list. The window draws it, because the window
/// has the PDF open already (`page-picture.ts`).
#[tauri::command]
pub async fn save_page_picture(
    recents: State<'_, RecentDocuments>,
    pictures: State<'_, Pictures>,
    folder: String,
    png_base64: String,
) -> CommandResult<()> {
    let pictures = pictures.inner().clone();
    with_recents(recents, move |recents| {
        let png = BASE64.decode(png_base64).map_err(to_message)?;
        pictures
            .save(&recents.list(), Path::new(&folder), &png)
            .map_err(to_message)
    })
    .await
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
    pictures: State<'_, Pictures>,
    from: String,
    to: String,
) -> CommandResult<()> {
    let pictures = pictures.inner().clone();
    with_recents(recents, move |recents| {
        recents
            .moved(Path::new(&from), Path::new(&to))
            .map_err(to_message)?;
        // The document has just been opened where it is now, and is drawn again there.
        if !recents::same_folder(&from, &to, recents::IGNORING_CASE) {
            pictures.forget(Path::new(&from));
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn forget_recent(
    recents: State<'_, RecentDocuments>,
    pictures: State<'_, Pictures>,
    folder: String,
) -> CommandResult<()> {
    let pictures = pictures.inner().clone();
    with_recents(recents, move |recents| {
        recents.forget(Path::new(&folder)).map_err(to_message)?;
        pictures.forget(Path::new(&folder));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn clear_recents(
    recents: State<'_, RecentDocuments>,
    pictures: State<'_, Pictures>,
) -> CommandResult<()> {
    let pictures = pictures.inner().clone();
    with_recents(recents, move |recents| {
        recents.clear().map_err(to_message)?;
        pictures.clear();
        Ok(())
    })
    .await
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
        .find(|document| recents::same_folder(&document.folder, folder, recents::IGNORING_CASE))
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

/// Record a document that is open, and answer its title.
fn record_opened<R: Runtime>(app: &AppHandle<R>, root_dir: &Path, root_file: Option<&Path>) -> String {
    let read = read_title(root_dir, root_file);
    let title = title_to_show(app, root_dir, root_file, read.as_deref());
    record(app, root_dir, root_file, read.as_deref());
    title
}

/// The `\title` in a document's main file.
pub fn read_title(root_dir: &Path, root_file: Option<&Path>) -> Option<String> {
    let source = crate::project::read_text(&root_dir.join(root_file?)).ok()?;
    texwords::title_of(&source).filter(|title| !title.trim().is_empty())
}

/// What a document is called: what its main file says, else what it was called last time (a letter has no
/// `\title`; it was typed when the letter was made), else the name of its folder.
pub fn title_to_show<R: Runtime>(
    app: &AppHandle<R>,
    root_dir: &Path,
    root_file: Option<&Path>,
    read: Option<&str>,
) -> String {
    let known = || {
        root_file?; // with no main file it was never recorded
        let recents = app.try_state::<RecentDocuments>()?;
        listed_document(&recents, &root_dir.to_string_lossy()).map(|document| document.title)
    };
    read.map(|title| title.trim().to_string())
        .or_else(known)
        .unwrap_or_else(|| recents::folder_name(root_dir))
}

/// Put a document that is open on the list. With no main file there is nothing to come back to, so
/// nothing is recorded; in a test with no list, neither (`try_state`). It asks other drives than the
/// document's own (`crowd`), so nobody who is waiting may call it.
pub fn record<R: Runtime>(app: &AppHandle<R>, root_dir: &Path, root_file: Option<&Path>, read: Option<&str>) {
    let (Some(root_file), Some(recents)) = (root_file, app.try_state::<RecentDocuments>()) else {
        return;
    };
    // A folder of many things can still be opened, from the command line. It is not a document to come
    // back to.
    if crowd(app).holds(root_dir) {
        return;
    }
    if let Err(error) = recents.record(root_dir, root_file, read, now()) {
        // The list is a convenience: the document opens whether or not it can be remembered.
        tracing::warn!(%error, "could not record the document");
    }
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

/// What [`set_preferences`] answers for a place that is itself a document. The window has the sentence.
const PLACE_IS_A_DOCUMENT: &str = "placeIsADocument";

#[tauri::command]
pub async fn set_preferences<R: Runtime>(
    app: AppHandle<R>,
    change: PreferencesChange,
) -> CommandResult<PreferencesView> {
    tauri::async_runtime::spawn_blocking(move || {
        // Kept as a place, it would be a folder of many things, and no longer open as a document. One that
        // is such a folder already may have a main file lying about in it.
        let place = change.place.as_ref().map(Path::new);
        if place.is_some_and(|place| is_a_document(place) && !crowd(&app).holds(place)) {
            return Err(PLACE_IS_A_DOCUMENT.to_string());
        }
        let kept = change.apply(&app.state::<PreferencesFile>())?;
        Ok(view(&app, kept))
    })
    .await
    .map_err(to_message)?
}

fn nearest_text_size(asked: u16) -> u16 {
    TEXT_SIZES
        .into_iter()
        .min_by_key(|size| size.abs_diff(asked))
        .unwrap_or(100)
}

/// Make everything in the window `size` percent of its usual size. From Rust, so that the page needs no
/// permission to zoom its window.
#[tauri::command]
pub fn apply_text_size<R: Runtime>(window: WebviewWindow<R>, size: u16) -> CommandResult<()> {
    let zoom = f64::from(nearest_text_size(size)) / 100.0;
    window.set_zoom(zoom).map_err(to_message)
}

/// The folders that are never one document: the ones the system names, the ones new documents go in, the
/// home folder and everything above it, and drives.
pub fn crowd<R: Runtime>(app: &AppHandle<R>) -> Crowd {
    /// How one of them is found. A plain `fn`, so that a table can hold different ones.
    type Find<R> = fn(&AppHandle<R>) -> Option<PathBuf>;
    let found: [(&str, Find<R>); 10] = [
        ("desktop", |app| app.path().desktop_dir().ok()),
        ("downloads", |app| app.path().download_dir().ok()),
        ("documents", |app| app.path().document_dir().ok()),
        ("pictures", |app| app.path().picture_dir().ok()),
        ("music", |app| app.path().audio_dir().ok()),
        ("videos", |app| app.path().video_dir().ok()),
        ("public", |app| app.path().public_dir().ok()),
        ("temporary", |app| app.path().temp_dir().ok()),
        ("place", |app| Some(new_documents_folder(app))),
        // Documents made before another place was chosen are still in the first one.
        ("default place", |app| {
            let folders = app.path();
            Some(default_place(
                folders.document_dir().ok(),
                folders.home_dir().ok()?,
            ))
        }),
    ];
    // Windows looks whether a folder is there before it names it, and Documents can be on a network.
    let ask = |(name, find): (&str, Find<R>)| -> (PathBuf, Question) {
        let app = app.clone();
        (name.into(), Box::new(move || find(&app)))
    };
    let places = slow_places(app);
    let home = app.path().home_dir().ok();
    // `flatten` drops the ones this system does not name, or not in time.
    let mut listed: Vec<PathBuf> = places.ask(found.map(ask).into()).into_iter().flatten().collect();
    let variable = |name: &str| std::env::var_os(name).map(PathBuf::from);
    listed.extend(cloud_folders(home.as_deref(), variable));
    Crowd::new(listed, home, is_mount_point, places)
}

/// A folder is where a drive begins when it is on another device than the folder it is in.
#[cfg(unix)]
fn is_mount_point(folder: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let device = |path: &Path| std::fs::metadata(path).map(|found| found.dev()).ok();
    match folder.parent() {
        Some(parent) => device(folder) != device(parent),
        None => true,
    }
}

/// A Windows drive is a root, which `Crowd` knows without asking.
#[cfg(not(unix))]
fn is_mount_point(_folder: &Path) -> bool {
    false
}

/// What was chosen on the Open page, and what it needs. The window has the words for each.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Choice {
    /// Open this folder, with this file (a path inside it) in front.
    Document {
        folder: String,
        file: Option<String>,
    },
    /// A folder of many things.
    CrowdedFolder {
        place: Vec<String>,
    },
    /// A `.tex` among many things. `place` is where a folder of its own would be made, and `others` how
    /// many files it uses would go with it.
    StrayFile {
        file: String,
        place: Vec<String>,
        others: usize,
    },
    /// A folder with no `.tex` in it.
    NoTex {
        folder: String,
    },
    Zip {
        file: String,
    },
    Word,
    Pdf,
    OtherArchive,
    OtherFile,
    Gone,
}

/// Look at what was chosen. Reads disks that may be slow, so only from a blocking thread.
fn inspect(path: &Path, crowd: &Crowd, place: impl Fn() -> PathBuf, home: Option<&Path>) -> Choice {
    let Ok(found) = std::fs::metadata(path) else {
        return Choice::Gone;
    };
    let text = |path: &Path| path.to_string_lossy().into_owned();
    if found.is_dir() {
        if crowd.holds(path) {
            return Choice::CrowdedFolder {
                place: folder_parts(path, home),
            };
        }
        if !path.join("abstract-tex.toml").is_file() && !has_tex(path) {
            return Choice::NoTex { folder: text(path) };
        }
        return Choice::Document {
            folder: text(path),
            file: None,
        };
    }
    match kind_of(path) {
        FileKind::Tex => {
            match document_folder(path, &|folder| crowd.holds(folder), &|folder| detect_root(folder)) {
                Some(folder) => Choice::Document {
                    file: path
                        .strip_prefix(&folder)
                        .ok()
                        .map(crate::project::to_forward_slashes),
                    folder: text(&folder),
                },
                None => Choice::StrayFile {
                    file: text(path),
                    place: folder_parts(&place(), home),
                    others: files_named_by(path).len(),
                },
            }
        }
        FileKind::Zip => Choice::Zip { file: text(path) },
        FileKind::Word => Choice::Word,
        FileKind::Pdf => Choice::Pdf,
        FileKind::OtherArchive => Choice::OtherArchive,
        FileKind::Other => Choice::OtherFile,
    }
}

#[tauri::command]
pub async fn inspect_choice<R: Runtime>(app: AppHandle<R>, path: String) -> CommandResult<Choice> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::absolute(&path).map_err(to_message)?;
        let home = app.path().home_dir().ok();
        Ok(inspect(
            &path,
            &crowd(&app),
            || new_documents_folder(&app), // asked only when it is needed: it can be slow to name
            home.as_deref(),
        ))
    })
    .await
    .map_err(to_message)?
}

/// What became of a zip. `Refused` has the name it is about, or of an `Io` what the disk said.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Unpacked {
    Unpacked { folder: String },
    Refused { why: WhyRefused, name: String },
    Stopped,
}

/// The window has a sentence for each (`UNPACK_REFUSALS` in `src/lib/start.svelte.ts`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WhyRefused {
    NotAZip,
    Locked,
    Unsupported,
    TooMany,
    TooBig,
    UnsafeName,
    SameName,
    NoTex,
    Io,
}

/// What the window is told of an error.
fn told(error: UnpackError) -> Unpacked {
    let refused = |why, name| Unpacked::Refused { why, name };
    match error {
        UnpackError::Stopped => Unpacked::Stopped,
        UnpackError::NotAZip => refused(WhyRefused::NotAZip, String::new()),
        UnpackError::Locked => refused(WhyRefused::Locked, String::new()),
        UnpackError::Unsupported => refused(WhyRefused::Unsupported, String::new()),
        UnpackError::TooMany => refused(WhyRefused::TooMany, String::new()),
        UnpackError::TooBig => refused(WhyRefused::TooBig, String::new()),
        UnpackError::NoTex => refused(WhyRefused::NoTex, String::new()),
        UnpackError::UnsafeName(name) => refused(WhyRefused::UnsafeName, name),
        UnpackError::SameName(_, name) => refused(WhyRefused::SameName, name),
        // Its kind only: the whole error names a staging folder nobody has heard of.
        UnpackError::Io(error) => refused(WhyRefused::Io, error.kind().to_string()),
    }
}

/// The part of [`unpack_zip`] that touches a disk.
fn unpack_to_place<R: Runtime>(app: &AppHandle<R>, zip: &Path, opening: &Opening) -> CommandResult<Unpacked> {
    let place = new_documents_folder(app);
    let name = folder_name_for_title(&zip.file_stem().unwrap_or_default().to_string_lossy());
    let folder = free_name(&place, &name);
    forget_permissions(app, &folder)?;
    let staged = match unpack(zip, &place, &Limits::default(), &|| opening.is_stopped()) {
        Ok(staged) => staged,
        Err(error) => return Ok(told(error)),
    };
    // The same claim as for opening a folder: stopped a moment ago, it must not appear after all.
    if !opening.adopt() {
        return Ok(Unpacked::Stopped);
    }
    Ok(match staged.place(&folder) {
        Ok(()) => {
            forget_listing(app, &folder);
            Unpacked::Unpacked {
                folder: folder.to_string_lossy().into_owned(),
            }
        }
        Err(error) => told(UnpackError::Io(error)),
    })
}

/// Unpack a zip into a folder of its own in the place new documents go. The zip is left alone.
#[tauri::command]
pub async fn unpack_zip<R: Runtime>(app: AppHandle<R>, path: String) -> CommandResult<Unpacked> {
    make_folder(app, move |app, opening| {
        unpack_to_place(app, Path::new(&path), opening)
    })
    .await
    .unwrap_or(Ok(Unpacked::Stopped))
}

/// Make a document's folder as the first part of opening it, so that *Stop* works on both alike. `None`
/// when it was stopped: what is being made then sees so for itself, and cleans up.
pub async fn make_folder<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    work: impl FnOnce(&AppHandle<R>, &Opening) -> CommandResult<T> + Send + 'static,
) -> Option<CommandResult<T>> {
    let opening = app.state::<AppState>().openings.begin();
    let making = {
        let opening = Arc::clone(&opening);
        tauri::async_runtime::spawn_blocking(move || work(&app, &opening))
    };
    tokio::select! {
        answer = making => Some(answer.map_err(to_message).and_then(|answer| answer)),
        () = opening.stopped() => None,
    }
}

/// The part of [`copy_stray`] that touches a disk.
fn copy_to_place<R: Runtime>(app: &AppHandle<R>, file: &Path, opening: &Opening) -> CommandResult<String> {
    let place = new_documents_folder(app);
    let name = folder_name_for_title(&file.file_stem().unwrap_or_default().to_string_lossy());
    let folder = free_name(&place, &name);
    forget_permissions(app, &folder)?;
    // Their kind only: the whole error names a staging folder nobody has heard of.
    let unwritten = |error: std::io::Error| {
        format!(
            "A copy could not be made in {} ({}). Choose another place for new documents.",
            place.display(),
            error.kind()
        )
    };
    let staged = copy_document(file, &place, &|| opening.is_stopped()).map_err(|error| match error {
        CopyError::Read { name, source } => format!("{name} could not be read ({}).", source.kind()),
        CopyError::Write(error) => unwritten(error),
        CopyError::Stopped => OPENING_STOPPED.to_string(),
    })?;
    if !opening.adopt() {
        return Err(OPENING_STOPPED.to_string());
    }
    staged.place(&folder).map_err(unwritten)?;
    forget_listing(app, &folder);
    Ok(folder.to_string_lossy().into_owned())
}

/// Copy a `.tex` that is among other things, and the files it uses, into a folder of its own in the place
/// new documents go. The originals stay as they are. Answers the new folder.
#[tauri::command]
pub async fn copy_stray<R: Runtime>(app: AppHandle<R>, file: String) -> CommandResult<String> {
    make_folder(app, move |app, opening| {
        copy_to_place(app, Path::new(&file), opening)
    })
    .await
    .unwrap_or_else(|| Err(OPENING_STOPPED.to_string()))
}

/// A folder that is about to be made starts with no permission, whatever was allowed at that address
/// before: what was allowed was for a document that is gone (S17.6). Called before the folder exists, so
/// that a failure leaves nothing behind.
pub fn forget_permissions<R: Runtime>(app: &AppHandle<R>, folder: &Path) -> CommandResult<()> {
    // `try_state`: absent in a test that is not about this.
    if let Some(consent) = app.try_state::<ShellEscapeConsent>() {
        for spelling in spellings_of(app, folder, consent.allowed_folders().map_err(to_message)?)? {
            consent.disallow(&spelling).map_err(to_message)?;
        }
    }
    if let Some(assistant) = app.try_state::<AssistantSettings>() {
        for spelling in spellings_of(app, folder, assistant.enabled_folders()?)? {
            assistant.forget(&spelling)?;
        }
    }
    Ok(())
}

/// A folder that has just been made is not the document that was listed at that address: its title and
/// its picture go. Called once it is made, so that a failure takes nothing off the list.
pub fn forget_listing<R: Runtime>(app: &AppHandle<R>, folder: &Path) {
    let (Some(recents), Some(pictures)) = (app.try_state::<RecentDocuments>(), app.try_state::<Pictures>())
    else {
        return; // a test that is not about this
    };
    let listed = recents.list().into_iter().map(|document| document.folder);
    for spelling in spellings_of(app, folder, listed.collect()).unwrap_or_default() {
        if let Err(error) = recents.forget(&spelling) {
            tracing::warn!(%error, "could not take the old document off the list");
        }
        pictures.forget(&spelling);
    }
}

/// Those of `kept` that are `folder`, however they were spelled: with a separator at the end, through a
/// link to a folder it is in, or in other letters where the system does not tell them apart. Asks disks
/// within a limit; one that does not answer is compared by its spelling.
fn spellings_of<R: Runtime>(
    app: &AppHandle<R>,
    folder: &Path,
    kept: Vec<String>,
) -> CommandResult<Vec<PathBuf>> {
    let folder = one_spelling(folder).map_err(to_message)?;
    let same = |a: &Path, b: &Path| {
        recents::same_folder(&a.to_string_lossy(), &b.to_string_lossy(), recents::IGNORING_CASE)
    };
    let name = |folder: &Path| PathBuf::from(recents::folder_name(folder));
    let alike: Vec<PathBuf> = kept
        .into_iter()
        .map(PathBuf::from)
        .filter(|kept| same(&name(kept), &name(&folder)))
        .collect();
    // Where they are, not the folder itself: it is not there yet.
    let parents: Vec<&Path> = std::iter::once(&folder)
        .chain(&alike)
        .map(|folder| folder.parent().unwrap_or(folder))
        .collect();
    let real = slow_places(app).real_places(&parents);
    let (here, others) = (&real[0], &real[1..]);
    let is_the_same = |kept: &PathBuf, place: &Option<PathBuf>| {
        same(kept, &folder) || matches!((here, place), (Some(here), Some(place)) if same(here, place))
    };
    Ok(alike
        .iter()
        .zip(others)
        .filter(|(kept, place)| is_the_same(kept, place))
        .map(|(kept, _)| kept.clone())
        .collect())
}

fn slow_places<R: Runtime>(app: &AppHandle<R>) -> SlowPlaces {
    match app.try_state::<SlowPlaces>() {
        Some(places) => places.inner().clone(),
        None => SlowPlaces::default(), // a test that is not about this
    }
}

#[cfg(test)]
mod tests {
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
    fn taking_a_document_off_the_list_removes_its_picture() {
        let config = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_builder()
            .manage(RecentDocuments::at(config.path().join("recent-documents.toml")))
            .manage(Pictures::at(config.path().join("recent-pictures")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        for folder in ["/a", "/b", "/c"] {
            app.state::<RecentDocuments>()
                .record(Path::new(folder), Path::new("main.tex"), None, 1)
                .unwrap();
        }
        let png = BASE64.encode(b"\x89PNG\r\n\x1a\n and the rest");
        let save = |folder: &str| {
            tauri::async_runtime::block_on(save_page_picture(
                app.state(),
                app.state(),
                folder.into(),
                png.clone(),
            ))
        };
        let drawn = || -> Vec<String> {
            tauri::async_runtime::block_on(recent_documents(app.handle().clone(), app.state()))
                .unwrap()
                .into_iter()
                .filter(|row| row.picture.as_ref().is_some_and(|path| Path::new(path).is_file()))
                .map(|row| row.folder)
                .collect()
        };
        assert!(drawn().is_empty());
        for folder in ["/a", "/b", "/c"] {
            save(folder).unwrap();
        }
        assert_eq!(drawn(), ["/c", "/b", "/a"]);
        assert!(save("/d").is_err(), "not on the list");
        let files = || {
            std::fs::read_dir(config.path().join("recent-pictures"))
                .unwrap()
                .count()
        };

        tauri::async_runtime::block_on(forget_recent(app.state(), app.state(), "/b".into())).unwrap();
        assert_eq!((drawn(), files()), (vec!["/c".to_string(), "/a".into()], 2));
        // Found where it was: its drive is back.
        tauri::async_runtime::block_on(recent_moved(app.state(), app.state(), "/a".into(), "/a/".into()))
            .unwrap();
        assert_eq!(drawn(), ["/c", "/a"]);
        tauri::async_runtime::block_on(recent_moved(app.state(), app.state(), "/a".into(), "/e".into()))
            .unwrap();
        assert_eq!((drawn(), files()), (vec!["/c".to_string()], 1));
        tauri::async_runtime::block_on(clear_recents(app.state(), app.state())).unwrap();
        assert_eq!((drawn(), files()), (vec![], 0));
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
    fn a_size_that_is_not_offered_becomes_the_nearest_one() {
        for (asked, nearest) in [
            (0, 100),
            (100, 100),
            (107, 100),
            (108, 115),
            (160, 150),
            (999, 200),
        ] {
            assert_eq!(nearest_text_size(asked), nearest, "{asked}");
        }
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

    #[test]
    fn what_was_chosen_says_what_it_needs() {
        let home = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = home.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "thesis/main.tex",
            "\\documentclass{book}\\include{chapters/intro}",
        );
        write("thesis/chapters/intro.tex", "text");
        write(
            "Downloads/paper.tex",
            "\\documentclass{article}\\input{a}\\input{b}",
        );
        write("Downloads/a.tex", "");
        write("photos/cat.jpg", "");
        write("settled/abstract-tex.toml", "");
        for name in ["essay.docx", "essay.pdf", "essay.zip", "essay.7z", "essay.txt"] {
            write(name, "");
        }
        let downloads = home.path().join("Downloads");
        let crowd = Crowd::new(
            vec![downloads.clone()],
            Some(home.path().into()),
            |_| false,
            SlowPlaces::default(),
        );
        let place = home.path().join("Documents/Abstract-Tex");
        let look = |path: &str| {
            inspect(
                &home.path().join(path),
                &crowd,
                || place.clone(),
                Some(home.path()),
            )
        };
        let text = |path: &str| home.path().join(path).to_string_lossy().into_owned();
        let document = |folder: &str, file: Option<&str>| Choice::Document {
            folder: text(folder),
            file: file.map(str::to_string),
        };

        assert_eq!(look("thesis"), document("thesis", None));
        assert_eq!(
            look("thesis/chapters/intro.tex"),
            document("thesis", Some("chapters/intro.tex"))
        );
        assert_eq!(look("thesis/main.tex"), document("thesis", Some("main.tex")));
        assert_eq!(look("settled"), document("settled", None));
        assert_eq!(
            look("Downloads"),
            Choice::CrowdedFolder {
                place: vec!["Downloads".into()]
            }
        );
        assert_eq!(
            look("Downloads/paper.tex"),
            Choice::StrayFile {
                file: text("Downloads/paper.tex"),
                place: vec!["Documents".into(), "Abstract-Tex".into()],
                others: 1,
            }
        );
        assert_eq!(
            look("photos"),
            Choice::NoTex {
                folder: text("photos")
            }
        );
        assert_eq!(
            look("essay.zip"),
            Choice::Zip {
                file: text("essay.zip")
            }
        );
        assert_eq!(look("essay.docx"), Choice::Word);
        assert_eq!(look("essay.pdf"), Choice::Pdf);
        assert_eq!(look("essay.7z"), Choice::OtherArchive);
        assert_eq!(look("essay.txt"), Choice::OtherFile);
        assert_eq!(look("no such file.tex"), Choice::Gone);
    }

    #[test]
    fn an_answer_is_a_tagged_object() {
        let said = serde_json::to_value(Choice::StrayFile {
            file: "/a".into(),
            place: vec![],
            others: 2,
        })
        .unwrap();
        assert_eq!(
            said,
            serde_json::json!({"kind": "strayFile", "file": "/a", "place": [], "others": 2})
        );
    }

    #[test]
    fn the_folders_the_system_names_and_the_place_for_new_documents_are_crowded() {
        let config = tempfile::tempdir().unwrap();
        let place = config.path().join("writing");
        std::fs::create_dir(&place).unwrap();
        let app = tauri::test::mock_builder()
            .manage(PreferencesFile::at(config.path().join("preferences.toml")))
            .manage(RecentDocuments::at(config.path().join("recent-documents.toml")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let default = new_documents_folder(app.handle());
        app.state::<PreferencesFile>()
            .change(|kept| kept.place = Some(place.to_string_lossy().into_owned()))
            .unwrap();
        let crowd = crowd(app.handle());
        assert!(crowd.holds(&place) && crowd.holds(&default));
        assert!(crowd.holds(&app.path().home_dir().unwrap()) && crowd.holds(&std::env::temp_dir()));
        assert!(!crowd.holds(config.path()));

        // Opened all the same, from the command line: it is not a document to come back to.
        std::fs::write(place.join("main.tex"), "").unwrap();
        record_opened(app.handle(), &place, Some(Path::new("main.tex")));
        assert!(app.state::<RecentDocuments>().list().is_empty());
    }

    #[test]
    fn a_document_s_own_folder_is_no_place_for_new_ones() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let before = new_documents_folder(app.handle());
        let write = |path: &str, text: &str| {
            let path = folder.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("thesis/Main.TEX", "% my thesis\n\\documentclass{book}");
        write("settled/abstract-tex.toml", "");
        write("papers/bees/main.tex", "\\documentclass{article}");
        write("papers/notes.tex", "no main file: it has no class");
        write("writing/loose.tex", "\\documentclass{article}");
        let choose = |name: &str| {
            let place = Some(folder.path().join(name).to_string_lossy().into_owned());
            let change = PreferencesChange {
                place,
                ..Default::default()
            };
            tauri::async_runtime::block_on(set_preferences(app.handle().clone(), change))
        };
        for name in ["thesis", "settled"] {
            assert_eq!(choose(name), Err("placeIsADocument".to_string()), "{name}");
        }
        assert_eq!(new_documents_folder(app.handle()), before);
        // "writing" is the place already: what lies about in it makes no document of it.
        for name in ["writing", "papers", "not there yet"] {
            choose(name).unwrap();
        }
    }

    /// An app whose new documents go to `writing` in `folder`, and that has permissions to inherit.
    fn app_that_writes_in(folder: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .manage(AppState::new())
            .manage(PreferencesFile::at(folder.join("preferences.toml")))
            .manage(ShellEscapeConsent::at(folder.join("shell-escape.toml")))
            .manage(AssistantSettings::at(folder.join("assistant.json")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let place = folder.join("writing").to_string_lossy().into_owned();
        app.state::<PreferencesFile>()
            .change(|kept| kept.place = Some(place))
            .unwrap();
        app
    }

    fn zip_with(folder: &Path, name: &str, files: &[&str]) -> String {
        let path = folder.join(name);
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for file in files {
            writer
                .start_file(*file, zip::write::SimpleFileOptions::default())
                .unwrap();
        }
        writer.finish().unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_zip_is_unpacked_into_a_folder_of_its_own() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let zip = zip_with(folder.path(), "My Thesis (1).zip", &["main.tex"]);
        for name in ["my-thesis-1", "my-thesis-1-2"] {
            let answer = tauri::async_runtime::block_on(unpack_zip(app.handle().clone(), zip.clone()));
            let made = folder.path().join("writing").join(name);
            assert_eq!(
                answer.unwrap(),
                Unpacked::Unpacked {
                    folder: made.to_string_lossy().into_owned()
                }
            );
            assert!(made.join("main.tex").is_file());
        }
        assert!(Path::new(&zip).is_file(), "it is left alone");
    }

    #[test]
    fn what_is_unpacked_inherits_no_permission() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let made = folder.path().join("writing").join("thesis");
        let (consent, settings) = (
            app.state::<ShellEscapeConsent>(),
            app.state::<AssistantSettings>(),
        );
        consent.allow(&made).unwrap();
        crate::assistant::set_enabled(&settings, &made, true).unwrap();
        let zip = zip_with(folder.path(), "thesis.zip", &["main.tex"]);
        tauri::async_runtime::block_on(unpack_zip(app.handle().clone(), zip)).unwrap();
        assert!(made.is_dir());
        assert!(!consent.allows(&made) && !settings.is_enabled_for(&made));
    }

    /// An old row is how a folder is reached under another spelling than the one it is made under.
    #[cfg(unix)]
    #[test]
    fn what_is_made_inherits_nothing_through_another_spelling_of_its_address() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        app.manage(RecentDocuments::at(folder.path().join("recent-documents.toml")));
        app.manage(Pictures::at(folder.path().join("recent-pictures")));
        let place = folder.path().join("writing");
        std::fs::create_dir(&place).unwrap();
        std::os::unix::fs::symlink(&place, folder.path().join("link")).unwrap();
        std::fs::create_dir(folder.path().join("other")).unwrap();
        let at = |path: &str| folder.path().join(path);
        let same = [
            at("writing/thesis/"),
            at("link/thesis"),
            at("other/../writing/thesis"),
        ];
        let another = at("other/thesis");
        let (consent, settings) = (
            app.state::<ShellEscapeConsent>(),
            app.state::<AssistantSettings>(),
        );
        let (recents, pictures) = (app.state::<RecentDocuments>(), app.state::<Pictures>());
        for kept in same.iter().chain([&another]) {
            consent.allow(kept).unwrap();
            crate::assistant::set_enabled(&settings, kept, true).unwrap();
            recents
                .record(kept, Path::new("main.tex"), Some("My resignation"), 1)
                .unwrap();
            pictures
                .save(&recents.list(), kept, b"\x89PNG\r\n\x1a\n and the rest")
                .unwrap();
        }
        let zip = zip_with(folder.path(), "thesis.zip", &["main.tex"]);
        tauri::async_runtime::block_on(unpack_zip(app.handle().clone(), zip)).unwrap();
        for kept in &same {
            assert!(
                !consent.allows(kept) && !settings.is_enabled_for(kept),
                "{}",
                kept.display()
            );
            assert!(!pictures.path_for(kept).exists(), "{}", kept.display());
        }
        assert!(consent.allows(&another) && settings.is_enabled_for(&another));
        let listed: Vec<String> = recents.list().into_iter().map(|row| row.folder).collect();
        assert_eq!(listed, [another.to_string_lossy()]);
        assert_eq!(
            title_to_show(
                app.handle(),
                &at("writing/thesis"),
                Some(Path::new("main.tex")),
                None
            ),
            "thesis"
        );
    }

    #[test]
    fn a_refused_zip_says_why_and_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let refused = |why, name: &str| Unpacked::Refused {
            why,
            name: name.into(),
        };
        let mut cases = vec![
            (
                zip_with(folder.path(), "a.zip", &["../x.tex"]),
                refused(WhyRefused::UnsafeName, "../x.tex"),
            ),
            (
                zip_with(folder.path(), "b.zip", &["Main.tex", "main.tex"]),
                refused(WhyRefused::SameName, "main.tex"),
            ),
            (
                zip_with(folder.path(), "c.zip", &["x.txt"]),
                refused(WhyRefused::NoTex, ""),
            ),
        ];
        std::fs::write(folder.path().join("d.zip"), "hello").unwrap();
        for (name, answer) in [
            ("d.zip", refused(WhyRefused::NotAZip, "")),
            ("gone.zip", refused(WhyRefused::Io, "entity not found")),
        ] {
            cases.push((folder.path().join(name).to_string_lossy().into_owned(), answer));
        }
        for (zip, expected) in cases {
            let answer = tauri::async_runtime::block_on(unpack_zip(app.handle().clone(), zip));
            assert_eq!(answer.unwrap(), expected);
        }
        assert!(!folder.path().join("writing").exists());
    }

    #[test]
    fn a_stopped_unpacking_makes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let zip = zip_with(folder.path(), "thesis.zip", &["main.tex"]);
        let openings = &app.state::<AppState>().openings;
        let opening = openings.begin();
        openings.stop();
        let answer = unpack_to_place(app.handle(), Path::new(&zip), &opening);
        assert_eq!(answer.unwrap(), Unpacked::Stopped);
        // Not even begun: it is asked before anything is written.
        assert!(!folder.path().join("writing").exists());
    }

    fn copy(app: &tauri::App<tauri::test::MockRuntime>, file: &Path) -> CommandResult<String> {
        let file = file.to_string_lossy().into_owned();
        tauri::async_runtime::block_on(copy_stray(app.handle().clone(), file))
    }

    #[test]
    fn a_copy_gets_a_folder_of_its_own() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let paper = folder.path().join("My Paper.tex");
        std::fs::write(&paper, "\\input{part}").unwrap();
        std::fs::write(folder.path().join("part.tex"), "").unwrap();
        for name in ["my-paper", "my-paper-2"] {
            let made = folder.path().join("writing").join(name);
            assert_eq!(copy(&app, &paper).unwrap(), made.to_string_lossy());
            assert!(made.join("My Paper.tex").is_file() && made.join("part.tex").is_file());
        }
        assert!(paper.is_file());
    }

    #[test]
    fn a_copy_inherits_no_permission() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let made = folder.path().join("writing").join("paper");
        let (consent, settings) = (
            app.state::<ShellEscapeConsent>(),
            app.state::<AssistantSettings>(),
        );
        consent.allow(&made).unwrap();
        crate::assistant::set_enabled(&settings, &made, true).unwrap();
        std::fs::write(folder.path().join("paper.tex"), "").unwrap();
        copy(&app, &folder.path().join("paper.tex")).unwrap();
        assert!(made.is_dir());
        assert!(!consent.allows(&made) && !settings.is_enabled_for(&made));
    }

    #[test]
    fn a_copy_that_cannot_be_made_says_which_file_or_which_place() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        let refusal = copy(&app, &folder.path().join("gone.tex")).unwrap_err();
        assert!(
            refusal.contains("gone.tex") && !refusal.contains(".abstract-tex-new"),
            "{refusal}"
        );
        std::fs::write(folder.path().join("paper.tex"), "").unwrap();
        std::fs::remove_dir_all(folder.path().join("writing")).unwrap();
        std::fs::write(folder.path().join("writing"), "a file").unwrap();
        let refusal = copy(&app, &folder.path().join("paper.tex")).unwrap_err();
        assert!(refusal.contains("writing"), "{refusal}");
    }

    #[test]
    fn a_stopped_copy_makes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let app = app_that_writes_in(folder.path());
        std::fs::write(folder.path().join("paper.tex"), "").unwrap();
        let openings = &app.state::<AppState>().openings;
        let opening = openings.begin();
        openings.stop();
        assert!(copy_to_place(app.handle(), &folder.path().join("paper.tex"), &opening).is_err());
        let left = std::fs::read_dir(folder.path().join("writing")).unwrap().count();
        assert_eq!(left, 0);
    }

    #[test]
    fn a_refusal_is_a_tagged_object() {
        let said = serde_json::to_value(Unpacked::Refused {
            why: WhyRefused::NotAZip,
            name: String::new(),
        })
        .unwrap();
        assert_eq!(
            said,
            serde_json::json!({"kind": "refused", "why": "notAZip", "name": ""})
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_on_its_parents_device_is_no_drive() {
        let place = tempfile::tempdir().unwrap();
        assert!(!is_mount_point(place.path()));
        assert!(is_mount_point(Path::new("/")));
    }
}
