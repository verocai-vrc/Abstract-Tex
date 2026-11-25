//! The commands the frontend can invoke. Thin by design: validate, delegate, emit.
//!
//! Anything worth a unit test lives in `project`, `compile`, `watcher` or a library crate; this
//! file only glues them to Tauri. It must never hold a `Mutex` guard across an `.await`, and it
//! must never hand the frontend a path outside the open project.

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use abstract_tex_engine::draft::{self, DraftJob};
use abstract_tex_engine::{BuildJob, EngineInfo};
use abstract_tex_git::{
    BranchRow, BranchState, CommitRow, DiffSides, Discarded, Initialised, LargeFile, ProseSummary,
    Status as GitStatus, SyncOutcome,
};
use abstract_tex_github::{Account, NewRepository, Repository as GitHubRepository, Visibility};
use abstract_tex_reconcile::TextOp;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::bibliography::{self, BibliographyIndex, Finding};
use crate::compile::CompileEvent;
use crate::consent::ShellEscapeConsent;
use crate::git;
use crate::latexdiff::{self, ComparisonRequest, ComparisonStarted};
use crate::lsp::LspEvent;
use crate::opening::Opening;
use crate::project::{write_atomically, Project, ProjectInfo};
use crate::recents::RecentDocuments;
use crate::synctex::{self, ForwardQuery, ForwardResult, InverseQuery, InverseResult};
use crate::watcher::{self, remember_write, Change};
use crate::AppState;

/// Tauri needs command errors to be serialisable. A `String` is the simplest thing that is, and
/// every error here is destined for a person anyway, so we flatten `anyhow`/`thiserror` errors
/// to their message at this edge (see the note in `abstract_tex_engine`).
pub type CommandResult<T> = Result<T, String>;

pub fn to_message(error: impl Display) -> String {
    error.to_string()
}

/// Run a closure with the open project, or fail with a clear message if none is open.
fn with_project<T>(state: &AppState, f: impl FnOnce(&mut Project) -> anyhow::Result<T>) -> CommandResult<T> {
    let mut guard = state.project.lock().unwrap();
    let project = guard.as_mut().ok_or_else(|| "No document is open.".to_string())?;
    f(project).map_err(to_message)
}

/// A folder to open at launch: the first command-line argument, or `ABSTRACT_TEX_OPEN`.
/// `abstract-tex C:\thesis` is how a desktop app is expected to behave; the environment variable
/// is for smoke tests and CI, where passing arguments through `tauri dev` is awkward.
#[tauri::command]
pub fn initial_project() -> Option<String> {
    folder_from_args(std::env::args().skip(1)).or_else(|| {
        std::env::var("ABSTRACT_TEX_OPEN")
            .ok()
            .filter(|p| Path::new(p).is_dir())
    })
}

/// The folder named by the command line, if it names one.
///
/// Takes the arguments rather than reading them, so it can be tested.
///
/// A path with spaces is the case worth the extra code. `abstract-tex C:\My Thesis` reaches us as
/// one argument when it was quoted, but as `["C:\My", "Thesis"]` when it was not — and this
/// repository's own path contains spaces, so the unquoted form is not a corner case. We try the
/// whole tail joined first, then the first argument alone; whichever is a directory wins. The
/// ambiguity is real but harmless: a folder that exists is what the author meant.
fn folder_from_args(args: impl Iterator<Item = String>) -> Option<String> {
    let candidates: Vec<String> = args.take_while(|a| !a.starts_with('-')).collect();
    if candidates.is_empty() {
        return None;
    }
    let joined = candidates.join(" ");
    if Path::new(&joined).is_dir() {
        return Some(joined);
    }
    candidates
        .into_iter()
        .next()
        .filter(|first| Path::new(first).is_dir())
}

/// Which engine will run builds, or `None` if nothing was found.
#[tauri::command]
pub async fn engine_info(state: State<'_, AppState>) -> CommandResult<Option<EngineInfo>> {
    match state.orchestrator.engine() {
        Some(engine) => engine.probe().await.map(Some).map_err(to_message),
        None => Ok(None),
    }
}

/// Open a folder: load config, start watching, allow the PDF pane to read the build folder.
///
/// The reading happens on a blocking thread, because a folder on a drive that stopped answering never
/// returns and the window must (S17.2). `select!` waits for whichever comes first: the folder, or the
/// person's *Stop*. A read that is stuck cannot be interrupted, only left behind: it ends when the
/// drive answers or the app closes, and `Opening::adopt` then refuses to let it change anything.
///
/// Generic over the runtime so that a test can drive it with Tauri's mock one; the app itself only
/// ever uses the default.
#[tauri::command]
pub async fn open_project<R: Runtime>(app: AppHandle<R>, path: String) -> CommandResult<ProjectInfo> {
    let opening = app.state::<AppState>().openings.begin();
    let folder = PathBuf::from(path);
    let reading = {
        let (app, opening) = (app.clone(), Arc::clone(&opening));
        tauri::async_runtime::spawn_blocking(move || open_folder(&app, &folder, &opening))
    };
    tokio::select! {
        answer = reading => answer.map_err(to_message)?,
        () = opening.stopped() => Err(OPENING_STOPPED.to_string()),
    }
}

/// Stop the opening that is being read. `false` when it is too late: the document is open.
#[tauri::command]
pub fn stop_opening(state: State<'_, AppState>) -> bool {
    state.openings.stop()
}

pub(crate) const OPENING_STOPPED: &str = "Opening was stopped.";

/// The part of [`open_project`] that touches a disk.
fn open_folder<R: Runtime>(
    app: &AppHandle<R>,
    folder: &Path,
    opening: &Opening,
) -> CommandResult<ProjectInfo> {
    let mut project = Project::open(folder).map_err(to_message)?;
    let state = app.state::<AppState>();

    let emitter = app.clone();
    let watching = watcher::watch(
        &project.root_dir,
        state.written.clone(),
        move |change| match change {
            Change::Manuscript(event) => {
                // The watcher already dropped our own writes, so this is an external change. A `.bib`
                // or `.tex` may have changed what the bibliography index says; rebuild it here, on the
                // watcher's thread, so the frontend learns within one debounce window (S7.2).
                let touches_bibliography = bibliography::affects_index(Path::new(&event.path));
                let _ = emitter.emit("fs:changed", event);
                if touches_bibliography {
                    emit_bibliography(&emitter);
                }
                // Whoever wrote the file, `git status` now says something different (S10.3a).
                git::emit_status_changed(&emitter);
            }
            // Git itself wrote something the Source Control view reads — `git add` in a terminal,
            // say. Nothing to tell the editor about: no file in the project changed.
            Change::GitMetadata => git::emit_status_changed(&emitter),
        },
    );
    // A folder that cannot be watched, or not all of it, is not a reason to refuse to open it (R8.7):
    // the editor, the build and the PDF do not depend on it. The author is told what will be missed.
    let (watcher, watch_notice) = match watching {
        Ok(watcher) => {
            let notice = watcher.is_incomplete().then(watcher::incomplete_notice);
            (Some(watcher), notice)
        }
        Err(error) => (None, Some(watcher::unavailable_notice(&error))),
    };

    let mut info = project.info();
    let root_dir = project.root_dir.clone();
    let root_file = project.root_file();
    // Each project builds with the engine it asks for (S9.4); a running build of the previous
    // project finishes on the engine it started with.
    let (engine, engine_notice) = crate::compile::engine_for(project.config.project.engine.as_deref());
    info.engine_notice = engine_notice;
    info.watch_notice = watch_notice;

    let read_title = crate::start::read_title(&root_dir, root_file.as_deref());
    info.title = crate::start::title_to_show(app, &root_dir, root_file.as_deref(), read_title.as_deref());
    project.title = info.title.clone();
    // pdf.js loads the PDF over asset://, which only serves paths inside an allowed scope. The
    // whole state folder rather than `build/` alone: S9.9's draft PDF is under `draft/` and
    // S11.4c's comparison under `latexdiff/`, and with only `build/` allowed neither could load.
    // Here and not below: Tauri looks on the disk where the folder really is.
    app.asset_protocol_scope()
        .allow_directory(project.state_dir(), true)
        .map_err(to_message)?;

    // Everything slow is done, and nothing after `adopt` may touch a disk: *Stop* no longer works then,
    // and the window waits for the answer. From here to the end of the block the project lock is held,
    // so two openings can never finish in the wrong order, and nothing can be refused after `adopt`.
    let old_watcher = {
        let mut open = state.project.lock().unwrap();
        if !opening.adopt() {
            return Err(OPENING_STOPPED.to_string());
        }
        // Both lanes: a comparison compiles with the project's own engine too (design interview A2).
        state.diff_lane.orchestrator.set_engine(engine.clone());
        state.orchestrator.set_engine(engine);
        // Replace the old watcher *after* the new one exists, so there is never a gap. When there is
        // no new one, the old project's must still go: it would report that project's files as this
        // one's.
        let old_watcher = std::mem::replace(&mut *state.watcher.lock().unwrap(), watcher);
        *open = Some(project);
        old_watcher
    };
    // Recorded only now that it is open: what was refused or stopped is not a recent document (S17.3).
    // On a thread that nobody waits for, because it asks other drives.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::start::record(&app, &root_dir, root_file.as_deref(), read_title.as_deref())
    });
    // Nor is this waited for: a watcher's thread may be reading the old document's drive, and dropping
    // the watcher can wait for it. Not under the lock either: that thread takes it.
    tauri::async_runtime::spawn_blocking(move || drop(old_watcher));
    Ok(info)
}

/// Re-list the file tree (after an external change, for instance).
#[tauri::command]
pub fn refresh_tree(state: State<'_, AppState>) -> CommandResult<ProjectInfo> {
    with_project(&state, |project| Ok(project.info()))
}

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> CommandResult<String> {
    with_project(&state, |project| {
        let absolute = project.resolve(&path)?;
        // `project::read_text` refuses what is not UTF-8 text: a Latin-1 file opened with its
        // letters replaced would lose them on the first save (R7.8).
        crate::project::read_text(&absolute)
    })
}

/// Write a file the author edited. Atomic, and remembered so the watcher ignores the echo.
#[tauri::command]
pub fn write_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    contents: String,
) -> CommandResult<()> {
    with_project(&state, |project| {
        let absolute = project.resolve_for_write(&path)?;
        remember_write(&state.written, &absolute, contents.as_bytes());
        write_atomically(&absolute, &contents)?;
        Ok(())
    })?;
    // The watcher will ignore this write (that is what `remember_write` is for), so an edit to
    // a `.bib` or a new `\cite` in a `.tex` made in our own editor would never reach the
    // index unless it is rebuilt here. Outside `with_project`: the rebuild reads files and
    // must not run under the project lock.
    if bibliography::affects_index(Path::new(&path)) {
        emit_bibliography(&app);
    }
    // And for the same reason the Source Control view has to be told here rather than by the
    // watcher: a save from our own editor is exactly the write the echo filter drops, and it is
    // also the commonest way a file becomes a row in *Changes* (S10.3a).
    git::emit_status_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn create_file(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<ProjectInfo> {
    let info = with_project(&state, |project| {
        let absolute = project.resolve_for_write(&path)?;
        if absolute.exists() {
            anyhow::bail!("{path} already exists");
        }
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent)?;
        }
        remember_write(&state.written, &absolute, b"");
        write_atomically(&absolute, "")?;
        Ok(project.info())
    })?;
    // A new file is an untracked row in *Changes*, and our own write is invisible to the
    // watcher — same reason as `write_file` above.
    git::emit_status_changed(&app);
    Ok(info)
}

#[tauri::command]
pub fn set_root_file(state: State<'_, AppState>, path: String) -> CommandResult<ProjectInfo> {
    with_project(&state, |project| {
        project.set_root_file(&path)?;
        Ok(project.info())
    })
}

/// Start a build of the project's root file. Progress arrives as `compile` events.
///
/// `file` is the file being edited, project-relative (S9.9): when it belongs to a chapter, that
/// chapter is also drafted beside the full build, and a `draft` event may arrive first.
#[tauri::command]
pub fn compile(
    app: AppHandle,
    state: State<'_, AppState>,
    consent: State<'_, ShellEscapeConsent>,
    file: Option<String>,
) -> CommandResult<u64> {
    // Gather everything under the lock, then release it before the build starts.
    let (job, draft, latest_pdf) = with_project(&state, |project| {
        let root_file = project.root_file().ok_or_else(|| {
            anyhow::anyhow!("No root .tex file found. Create main.tex or choose a root file in the tree.")
        })?;
        let job = BuildJob {
            project_dir: project.root_dir.clone(),
            root_file,
            out_dir: project.build_dir(),
            synctex: true,
            // S9.8: this machine's consent for this folder, and nothing the project says.
            shell_escape: consent.allows(&project.root_dir),
        };
        let draft = file
            .and_then(|file| project.chapter_of(&file))
            .map(|chapter| DraftJob {
                chapter,
                dir: project.draft_dir(),
            });
        Ok((job, draft, project.latest_pdf()))
    })?;

    let emitter = app.clone();
    // S10.1. A successful compile is a recoverable state (DESIGN.md §5.7), and this is where that
    // is noticed: the orchestrator's job is builds, and a build in one of *its* tests must never
    // write a ref into whatever repository the test happened to run in.
    let snapshot_dir = job.project_dir.clone();
    let generation = state
        .orchestrator
        .request(job, draft, move |event: CompileEvent| {
            let succeeded = matches!(event, CompileEvent::Finished { success: true, .. });
            // The frontend hears first; the snapshot is never something the author waits for.
            let _ = emitter.emit("compile", &event);
            emitter.state::<AppState>().finished_pdf.note(&event, &latest_pdf);
            if succeeded {
                take_snapshot(snapshot_dir.clone());
            }
        });
    Ok(generation)
}

/// Take a snapshot of the project, off the build's path and out of its way.
///
/// `spawn_blocking` and not `spawn`: libgit2 is ordinary blocking file I/O, and hashing every
/// file of a thesis is real work that would otherwise stall whatever else an async worker owed
/// the window. Failures are logged and dropped — §5.7 asks for a safety net, and a safety net
/// that interrupts the author to announce its own failure is worse than one that quietly caught
/// nothing this time.
fn take_snapshot(project_dir: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || {
        // A poisoned lock means a previous snapshot panicked. That is worth knowing about, but
        // not worth refusing every later snapshot over, so the guard is taken either way.
        let _guard = crate::snapshots::AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match abstract_tex_snapshot::snapshot(&project_dir) {
            Ok(abstract_tex_snapshot::Snapshot::Took(id)) => tracing::debug!(%id, "snapshot taken"),
            Ok(abstract_tex_snapshot::Snapshot::Unchanged) => tracing::debug!("no snapshot: nothing changed"),
            Err(error) => tracing::warn!(%error, "no snapshot taken"),
        }
    });
}

// ---------------------------------------------------------------------------------------------
// S11.6: the Snapshots list. Reads go to `abstract-tex-snapshot` through `spawn_blocking` — a
// word count walks every `.tex` file of each version, which is real work, not a handful of `stat`s.
// ---------------------------------------------------------------------------------------------

/// The newest snapshots, newest first, with each version's word count. How many is the caller's
/// choice, so the list can be a short page and a longer one.
#[tauri::command]
pub async fn snapshot_list(
    state: State<'_, AppState>,
    limit: usize,
) -> CommandResult<Vec<abstract_tex_snapshot::SnapshotRow>> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    tauri::async_runtime::spawn_blocking(move || abstract_tex_snapshot::list(&root, limit))
        .await
        .map_err(to_message)?
        .map_err(to_message)
}

/// The `.tex` and `.bib` files one snapshot holds — the ones the list offers to show, since they
/// are the ones an author writes. Everything else is in the snapshot too, and recoverable with
/// plain `git`; it is just not offered here.
#[tauri::command]
pub async fn snapshot_files(state: State<'_, AppState>, id: String) -> CommandResult<Vec<String>> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    let all = tauri::async_runtime::spawn_blocking(move || abstract_tex_snapshot::files(&root, &id))
        .await
        .map_err(to_message)?
        .map_err(to_message)?;
    Ok(all
        .into_iter()
        .filter(|path| path.ends_with(".tex") || path.ends_with(".bib"))
        .collect())
}

/// One file as it was in one snapshot, as text. Read-only: nothing here touches the project.
#[tauri::command]
pub async fn snapshot_read(state: State<'_, AppState>, id: String, path: String) -> CommandResult<String> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    let shown = path.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || abstract_tex_snapshot::read(&root, &id, &path))
        .await
        .map_err(to_message)?
        .map_err(to_message)?
        .ok_or_else(|| format!("That version of the document has no {shown}."))?;
    String::from_utf8(bytes).map_err(|_| format!("{shown} is not a text file."))
}

/// Put one file back as it was in a snapshot, keeping the version it replaces
/// (`crate::snapshots::restore_file`). The open tab, if any, hears about it from the watcher.
#[tauri::command]
pub async fn snapshot_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> CommandResult<()> {
    let (root, absolute) = with_project(&state, |project| {
        Ok((project.root_dir.clone(), project.resolve_for_write(&path)?))
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::snapshots::restore_file(&root, &absolute, &id, &path)
    })
    .await
    .map_err(to_message)??;
    git::emit_status_changed(&app);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// S16.3: crash reports. Local files only (`crate::crash`); these two commands read and tidy them.
// ---------------------------------------------------------------------------------------------

fn crash_reports_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(to_message)?
        .join("crash-reports"))
}

/// The newest crash report the author has not been asked about, with the prefilled issue address.
#[tauri::command]
pub fn crash_pending(app: AppHandle) -> CommandResult<Option<crate::crash::PendingReport>> {
    Ok(crate::crash::newest_pending(&crash_reports_dir(&app)?))
}

/// Stop offering a report: it is kept, moved to `handled/`.
#[tauri::command]
pub fn crash_dismiss(app: AppHandle, id: String) -> CommandResult<()> {
    crate::crash::dismiss(&crash_reports_dir(&app)?, &id).map_err(to_message)
}

// ---------------------------------------------------------------------------------------------
// S14.3b: comments. Kept on `refs/abstract-tex/comments` (`crate::comments`); only `comments_sync`
// (S14.3d, after the person pressed Sync) sends or fetches them, and nothing touches the working tree.
// ---------------------------------------------------------------------------------------------

fn seconds_since_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Every comment in the project, oldest first, each reanchored against the files on disk.
#[tauri::command]
pub async fn comments_list(state: State<'_, AppState>) -> CommandResult<Vec<crate::comments::CommentRow>> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    tauri::async_runtime::spawn_blocking(move || crate::comments::list(&root))
        .await
        .map_err(to_message)?
        .map_err(to_message)
}

/// Comment on `from..to` (UTF-16 units) of `text`, the editor's buffer. Returns the comment's id.
#[tauri::command]
pub async fn comment_add(
    state: State<'_, AppState>,
    file: String,
    text: String,
    from: usize,
    to: usize,
    body: String,
) -> CommandResult<String> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::comments::add_comment(&root, &file, &text, from, to, &body, seconds_since_epoch())
    })
    .await
    .map_err(to_message)?
    .map_err(to_message)
}

/// The saved GitHub token, read on a thread from the blocking pool (R5.9).
///
/// The read asks the OS keychain, which can stop for a password. On an async worker that wait
/// would hold up every other async command sharing the worker, so the commands that send the token
/// to GitHub (`git_push`, `git_sync`, `git_clone`, `comments_sync`) get it through here. The
/// session is passed as an `Arc` because the closure sent to the pool must own what it uses; a
/// `State<'_, _>` borrowed from the command cannot be moved into it.
async fn saved_github_token(github: &Arc<crate::github::GitHubSession>) -> CommandResult<Option<String>> {
    let github = Arc::clone(github);
    tauri::async_runtime::spawn_blocking(move || github.token().map_err(to_message))
        .await
        .map_err(to_message)?
}

/// Exchange comments with `origin` (S14.3d): the frontend calls this right after a successful
/// *Sync*, so one click moves the text and the margin notes together. Reads the saved GitHub token
/// the same way `git_sync` does; nothing here runs unless the person pressed Sync.
#[tauri::command]
pub async fn comments_sync(state: State<'_, AppState>) -> CommandResult<crate::comments::CommentsSync> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    let token = saved_github_token(&state.github).await?;
    tauri::async_runtime::spawn_blocking(move || crate::comments::sync(&root, token.as_deref()))
        .await
        .map_err(to_message)?
        .map_err(to_message)
}

/// Resolve a comment, or reopen it.
#[tauri::command]
pub async fn comment_set_resolved(
    state: State<'_, AppState>,
    id: String,
    resolved: bool,
) -> CommandResult<()> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::comments::set_resolved(&root, &id, resolved, seconds_since_epoch())
    })
    .await
    .map_err(to_message)?
    .map_err(to_message)
}

/// Whether the open project's builds may run programs (S9.8). An error with no project open.
#[tauri::command]
pub fn shell_escape_allowed(
    state: State<'_, AppState>,
    consent: State<'_, ShellEscapeConsent>,
) -> CommandResult<bool> {
    with_project(&state, |project| Ok(consent.allows(&project.root_dir)))
}

/// Let the open project's builds run programs, on this machine, from now on. The frontend asks
/// the person first, in words that say what this allows; this command is only ever their answer.
#[tauri::command]
pub fn allow_shell_escape(
    state: State<'_, AppState>,
    consent: State<'_, ShellEscapeConsent>,
) -> CommandResult<()> {
    with_project(&state, |project| consent.allow(&project.root_dir))
}

#[tauri::command]
pub fn disallow_shell_escape(
    state: State<'_, AppState>,
    consent: State<'_, ShellEscapeConsent>,
) -> CommandResult<()> {
    with_project(&state, |project| consent.disallow(&project.root_dir))
}

#[tauri::command]
pub fn cancel_compile(state: State<'_, AppState>) -> CommandResult<()> {
    state.orchestrator.cancel();
    Ok(())
}

/// The raw log, for the view that is one click away and never the default (DESIGN.md §2).
#[tauri::command]
pub fn read_log(state: State<'_, AppState>) -> CommandResult<String> {
    with_project(&state, |project| {
        let root = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("no root file"))?;
        let stem = root
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".into());
        let log = project.build_dir().join(format!("{stem}.log"));
        match std::fs::read(&log) {
            Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
            Err(_) => Ok(String::new()),
        }
    })
}

/// Diff two texts into CRDT operations. Pure; lives in Rust because `similar` is there and the
/// UTF-16 conversion is the kind of detail we want in one tested place.
///
/// `async` with `spawn_blocking`: a plain `#[tauri::command]` function runs on the main thread, so
/// a diff of a large rewritten document froze the whole window (review finding R4.2). The reconciler
/// now has a time budget as well, but the thread it runs on should not be the one the author types on.
#[tauri::command]
pub async fn diff_ops(old: String, new: String) -> CommandResult<Vec<TextOp>> {
    tauri::async_runtime::spawn_blocking(move || abstract_tex_reconcile::diff_ops(&old, &new))
        .await
        .map_err(to_message)
}

// ---------------------------------------------------------------------------
// Bibliography (S7.2). `bibliography` builds the index from disk; this is the glue that decides
// when, and hands it to the window as the `bibliography:changed` event.
// ---------------------------------------------------------------------------

/// The project's bibliography index, built fresh from disk. Empty — not an error — when there
/// is no root file, because then there is no document to have a bibliography.
#[tauri::command]
pub fn bibliography_index(state: State<'_, AppState>) -> CommandResult<BibliographyIndex> {
    let located = project_root(&state)?;
    Ok(match located {
        Some((root_dir, root_file, extra_bib_files)) => {
            bibliography::build_index(&root_dir, &root_file, &extra_bib_files)
        }
        None => BibliographyIndex::default(),
    })
}

/// The project's bibliography health findings (S8.3), built fresh from disk — a second pass over
/// the same files `bibliography_index` just read, not a cached copy of that call's result, so it
/// is never stale relative to whatever the author saved last. Empty when there is no root file,
/// the same "empty, not an error" rule `bibliography_index` follows.
#[tauri::command]
pub fn bibliography_health(state: State<'_, AppState>) -> CommandResult<Vec<Finding>> {
    let located = project_root(&state)?;
    Ok(match located {
        Some((root_dir, root_file, extra_bib_files)) => {
            let index = bibliography::build_index(&root_dir, &root_file, &extra_bib_files);
            index.health(&root_dir)
        }
        None => Vec::new(),
    })
}

/// What `identify` recognised a paste as, so the frontend can offer "Cite" without repeating
/// `texbib::acquire::identify`'s own matching rules.
#[tauri::command]
pub fn identify_paste(pasted: String) -> Option<String> {
    match texbib::acquire::identify(&pasted)? {
        texbib::acquire::Identified::Doi(_) => Some("doi".to_string()),
        texbib::acquire::Identified::Arxiv(_) => Some("arxiv".to_string()),
        texbib::acquire::Identified::Isbn(_) => Some("isbn".to_string()),
    }
}

/// What a successful paste-to-cite resolves to: a key to insert as `\cite{key}`, and whether it
/// was already in the bibliography or a new entry was appended.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteCiteResult {
    pub key: String,
    pub created: bool,
}

/// Paste-to-cite (S7.6): identify `pasted`, fetch its entry from the matching source, deduplicate
/// against the project's bibliography, and — on a miss — append the new entry to the first `.bib`
/// file the document names. Async because fetching is a real network request; the request itself
/// runs on a blocking-pool thread (`texbib::acquire`'s fetchers are synchronous `reqwest::blocking`
/// calls, the same shape S2.2's outcome warned an async command must never run directly, since
/// that would stall every other command sharing this runtime's worker thread for as long as the
/// request takes).
#[tauri::command]
pub async fn paste_cite(
    app: AppHandle,
    state: State<'_, AppState>,
    pasted: String,
) -> CommandResult<PasteCiteResult> {
    let identified =
        texbib::acquire::identify(&pasted).ok_or_else(|| "Not a DOI, arXiv id, or ISBN.".to_string())?;

    let fetched = tauri::async_runtime::spawn_blocking(move || fetch_identified(identified))
        .await
        .map_err(to_message)?
        .map_err(to_message)?;

    let (root_dir, root_file, extra_bib_files) =
        project_root(&state)?.ok_or_else(|| "No root .tex file found.".to_string())?;
    let index = bibliography::build_index(&root_dir, &root_file, &extra_bib_files);

    match crate::paste::resolve_paste(&fetched, &index).map_err(to_message)? {
        crate::paste::PasteOutcome::Existing { key } => Ok(PasteCiteResult { key, created: false }),
        crate::paste::PasteOutcome::New {
            bib_file,
            keys_in_use,
        } => {
            // Read and write in one hold of the project lock, and without `remember_write`: see
            // `paste::append_new_entry` (review finding R4.4).
            let key = with_project(&state, |project| {
                let absolute = project.resolve_for_write(&bib_file)?;
                crate::paste::append_new_entry(&absolute, &fetched, &keys_in_use).map_err(anyhow::Error::msg)
            })?;
            emit_bibliography(&app);
            Ok(PasteCiteResult { key, created: true })
        }
    }
}

/// The blocking half of [`paste_cite`]: one network request, on whichever source `identified`
/// names. Kept as a plain function (not a closure inline in `spawn_blocking`) so the "this runs
/// off the async runtime" boundary is a named, readable line rather than an anonymous block.
fn fetch_identified(identified: texbib::acquire::Identified) -> Result<texbib::Entry, String> {
    match identified {
        texbib::acquire::Identified::Doi(doi) => texbib::acquire::doi::fetch_doi(&doi).map_err(to_message),
        texbib::acquire::Identified::Arxiv(id) => {
            texbib::acquire::arxiv::fetch_arxiv(&id).map_err(to_message)
        }
        texbib::acquire::Identified::Isbn(isbn) => {
            texbib::acquire::isbn::fetch_isbn(&isbn).map_err(to_message)
        }
    }
}

/// Whether Zotero, with the Better BibTeX plugin, is reachable on this machine (S8.1;
/// DESIGN.md §5.4). Async for the same reason `paste_cite` is: `texbib::acquire::zotero::detect`
/// is a synchronous `reqwest::blocking` call, and running it directly on an async command would
/// stall every other command sharing this runtime's worker thread until it times out. A manual
/// command, not a background poll — nothing calls this until the author opens whatever UI offers
/// Zotero linking, matching the card's "detection is a manual command" choice.
#[tauri::command]
pub async fn detect_zotero() -> CommandResult<texbib::acquire::zotero::ZoteroStatus> {
    tauri::async_runtime::spawn_blocking(texbib::acquire::zotero::detect)
        .await
        .map_err(to_message)
}

/// List every library and collection Zotero (with Better BibTeX) currently has, for the "pick a
/// collection to link" UI (S8.2). Same async/`spawn_blocking` shape as `detect_zotero`, for the
/// same reason: `texbib::acquire::zotero::list_libraries` is a synchronous `reqwest::blocking`
/// call.
#[tauri::command]
pub async fn list_zotero_libraries() -> CommandResult<Vec<texbib::acquire::zotero::Library>> {
    tauri::async_runtime::spawn_blocking(texbib::acquire::zotero::list_libraries)
        .await
        .map_err(to_message)?
        .map_err(to_message)
}

/// Link a Zotero collection (S8.2): ask Better BibTeX to keep `output_path` (project-relative,
/// under the project so the bibliography watcher can see it) auto-exported from `collection_path`
/// in BibTeX format, then record `output_path` in `abstract-tex.toml`'s `extra_bib_files` so the next
/// index build reads it. The two steps are not one transaction — if Better BibTeX accepts the
/// auto-export but saving the config fails, the author sees the config error and the auto-export
/// is registered but unused, which `add_extra_bib_file`'s idempotence lets a retry fix without a
/// duplicate. No JSON-RPC method that would edit the library itself is ever called; see
/// `zotero.rs`'s module doc for the read/one-write shape this app allows itself.
#[tauri::command]
pub async fn link_zotero_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    collection_path: String,
    output_path: String,
) -> CommandResult<()> {
    let absolute = with_project(&state, |project| project.resolve_for_write(&output_path))?;
    let absolute_str = absolute.to_string_lossy().into_owned();

    tauri::async_runtime::spawn_blocking(move || {
        texbib::acquire::zotero::add_autoexport(&collection_path, &absolute_str)
    })
    .await
    .map_err(to_message)?
    .map_err(to_message)?;

    with_project(&state, |project| project.add_extra_bib_file(&output_path))?;
    emit_bibliography(&app);
    Ok(())
}

/// Unlink a collection (S8.7): drop `path` from `abstract-tex.toml`'s `extra_bib_files` and re-index.
/// No request to Zotero and no file deleted — see `Project::remove_extra_bib_file` for why — so,
/// unlike linking, this works whether or not Zotero is running, which is exactly when an author
/// most needs it: a linked export that will never appear because Zotero is gone.
#[tauri::command]
pub fn unlink_bib_file(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<()> {
    with_project(&state, |project| project.remove_extra_bib_file(&path))?;
    emit_bibliography(&app);
    Ok(())
}

/// Rebuild the index and emit it as `bibliography:changed`. Called from the watcher thread and
/// from `write_file`; both take the project lock only long enough to copy two paths out.
fn emit_bibliography<R: Runtime>(app: &AppHandle<R>) {
    // `app.state()` is how a thread that was not handed `State<'_, AppState>` by Tauri — the
    // watcher's — reaches the same shared state the commands borrow.
    let state = app.state::<AppState>();
    let index = match project_root(&state) {
        Ok(Some((root_dir, root_file, extra_bib_files))) => {
            bibliography::build_index(&root_dir, &root_file, &extra_bib_files)
        }
        Ok(None) => BibliographyIndex::default(),
        Err(_) => return, // the project was closed meanwhile; nobody is listening
    };
    let _ = app.emit("bibliography:changed", index);
}

/// The open project's folder, root file, and configured extra `.bib` files (S8.2), or `None`
/// when it has no root file yet.
fn project_root(state: &AppState) -> CommandResult<Option<(PathBuf, PathBuf, Vec<String>)>> {
    with_project(state, |project| {
        Ok(project.root_file().map(|root_file| {
            (
                project.root_dir.clone(),
                root_file,
                project.config.project.extra_bib_files.clone(),
            )
        }))
    })
}

// ---------------------------------------------------------------------------
// SyncTeX (S3.4 forward, S3.5 inverse). `synctex` resolves the .synctex.gz path and turns typed
// parser errors into a sentence; this is only the Tauri glue on top of it.
// ---------------------------------------------------------------------------

/// The folder whose `.synctex.gz` matches the PDF on screen: the full build's, or the draft's
/// while a draft is showing (S9.9). The frontend knows which one it is showing; Rust does not.
fn synctex_dir(project: &Project, draft: bool) -> PathBuf {
    if draft {
        draft::build_folder(&project.draft_dir())
    } else {
        project.build_dir()
    }
}

/// Cursor in the editor → page and point in the PDF (S3.4).
#[tauri::command]
pub fn synctex_forward(state: State<'_, AppState>, query: ForwardQuery) -> CommandResult<ForwardResult> {
    with_project(&state, |project| {
        let root_file = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("No root .tex file found."))?;
        // Checked as a path in the project (and not only joined): `resolve` refuses anything outside.
        project.resolve(&query.file)?;
        let table =
            synctex::open(&synctex_dir(project, query.draft), &root_file).map_err(|e| anyhow::anyhow!(e))?;
        synctex::forward_in_project(&table, &project.root_dir, &query.file, query.line)
            .map(ForwardResult::from)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Nothing typeset for {}:{} in the last build.",
                    query.file,
                    query.line
                )
            })
    })
}

/// Click in the PDF → file and line in the source (S3.5).
#[tauri::command]
pub fn synctex_inverse(state: State<'_, AppState>, query: InverseQuery) -> CommandResult<InverseResult> {
    with_project(&state, |project| {
        let root_file = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("No root .tex file found."))?;
        let table =
            synctex::open(&synctex_dir(project, query.draft), &root_file).map_err(|e| anyhow::anyhow!(e))?;
        let position = abstract_tex_synctex::PdfPosition {
            page: query.page,
            x: query.x,
            y: query.y,
        };
        let hit = table.inverse_search(position).ok_or_else(|| {
            anyhow::anyhow!(
                "Nothing on page {} of the last build near that point.",
                query.page
            )
        })?;
        Ok(synctex::to_relative(&project.root_dir, hit))
    })
}

// ---------------------------------------------------------------------------
// Language server (S3.2). Thin, like everything else here: the session owns the
// process, `abstract-tex-lsp` owns the protocol, and these four functions only pass
// messages between the frontend and the bridge.
// ---------------------------------------------------------------------------

/// Start TexLab for the open project and return its capabilities. Safe to call twice: the
/// session stops whatever was running first.
#[tauri::command]
pub async fn lsp_start(app: AppHandle, state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    // Take what's needed out from under the lock before any `.await` — the module rule in
    // `commands`'s doc comment, and the reason this is not one `with_project` call.
    let (root_dir, build_dir) = {
        let guard = state.project.lock().unwrap();
        let project = guard.as_ref().ok_or_else(|| "No document is open.".to_string())?;
        (project.root_dir.clone(), project.build_dir())
    };

    let emitter = app.clone();
    state
        .lsp
        .start(&root_dir, &build_dir, move |event: LspEvent| {
            let _ = emitter.emit("lsp", event);
        })
        .await
}

/// Send a request and wait for the server's answer. Errors — including "the server went away" —
/// come back as a message, never as a hang.
#[tauri::command]
pub async fn lsp_request(
    state: State<'_, AppState>,
    method: String,
    params: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    let bridge = state.lsp.bridge()?;
    bridge.request(&method, params).await.map_err(to_message)
}

/// Tell the server something without waiting. `textDocument/didChange` goes through here on
/// every keystroke burst, so it must not block (DESIGN.md §2, commitment 2).
#[tauri::command]
pub fn lsp_notify(
    state: State<'_, AppState>,
    method: String,
    params: serde_json::Value,
) -> CommandResult<()> {
    state.lsp.bridge()?.notify(&method, params).map_err(to_message)
}

/// Answer a request the server made of us, quoting the `id` from the `lsp` event.
#[tauri::command]
pub fn lsp_respond(
    state: State<'_, AppState>,
    id: serde_json::Value,
    result: serde_json::Value,
) -> CommandResult<()> {
    state.lsp.bridge()?.respond(id, result).map_err(to_message)
}

// ---------------------------------------------------------------------------------------------
// S10.3a: the Source Control view's questions and its three verbs.
//
// Reads answer `None` when the project is not inside a Git repository, which the view says in a
// sentence; verbs cannot be reached without one, so they fail with it instead (`git.rs`). Every
// verb ends by emitting `git:status-changed`, so the panel has one refresh path whether the
// change came from a button here or from `git` in a terminal.
// ---------------------------------------------------------------------------------------------

/// What has changed, in the three lists DESIGN.md §6 draws them in.
#[tauri::command]
pub fn git_status(state: State<'_, AppState>) -> CommandResult<Option<GitStatus>> {
    git::with_repository(&state, abstract_tex_git::status)
}

/// S11.3c: large-file candidates for the Source Control view's "track with Git LFS" banner.
///
/// 5 MB is this panel's own decision, settled with the maintainer alongside the banner's shape —
/// well under GitHub's 50 MB warning line and `abstract_tex_git`'s own 100 MB hard push block
/// (S11.3a), which answers a different question for a different reason and stays where it is.
#[tauri::command]
pub fn git_large_files(state: State<'_, AppState>) -> CommandResult<Option<Vec<LargeFile>>> {
    const LFS_THRESHOLD_BYTES: u64 = 5 * 1024 * 1024;
    git::with_repository(&state, |repository| {
        abstract_tex_git::large_files(repository, LFS_THRESHOLD_BYTES)
    })
}

/// Add one path to the index, or record its deletion there.
#[tauri::command]
pub fn git_stage(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<()> {
    git::in_repository(&state, |repository| abstract_tex_git::stage(repository, &path))?;
    git::emit_status_changed(&app);
    Ok(())
}

/// The two texts a row's side-by-side view compares (S11.7): `staged == false` is the index against
/// the working tree — what *Stage* would add — and `true` is `HEAD` against the index. Reads only.
#[tauri::command]
pub fn git_diff_sides(state: State<'_, AppState>, path: String, staged: bool) -> CommandResult<DiffSides> {
    git::in_repository(&state, |repository| {
        abstract_tex_git::diff_sides(repository, &path, staged)
    })
}

/// Put the index entry back to what `HEAD` has, leaving the file on disk alone.
#[tauri::command]
pub fn git_unstage(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<()> {
    git::in_repository(&state, |repository| abstract_tex_git::unstage(repository, &path))?;
    git::emit_status_changed(&app);
    Ok(())
}

/// Throw away the working-tree changes to one path, and say which of the two things that was.
///
/// The frontend has already confirmed with the author by the time this runs — "Discard always
/// confirms" (the Source Control design notes) — and the answer is what lets the notice
/// afterwards say *restored* or *deleted* rather than something that covers both.
#[tauri::command]
pub fn git_discard(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<Discarded> {
    let done = git::in_repository(&state, |repository| abstract_tex_git::discard(repository, &path))?;
    git::emit_status_changed(&app);
    Ok(done)
}

// ---------------------------------------------------------------------------------------------
// S10.3b: committing, and where the branch stands.
// ---------------------------------------------------------------------------------------------

/// Which branch this is and how far it has drifted from its upstream — the status bar's line.
#[tauri::command]
pub fn git_branch(state: State<'_, AppState>) -> CommandResult<Option<BranchState>> {
    git::with_repository(&state, abstract_tex_git::branch_state)
}

/// Every local branch, the current one first, for the picker (B4). Empty where there is no
/// repository or no commit yet.
#[tauri::command]
pub fn git_branches(state: State<'_, AppState>) -> CommandResult<Vec<BranchRow>> {
    Ok(git::with_repository(&state, abstract_tex_git::branches)?.unwrap_or_default())
}

/// Start a branch at the current commit and switch to it (B4). Every refusal — a name Git cannot
/// use, a name taken, no commit yet — is the command's error, in the crate's own sentence.
#[tauri::command]
pub fn git_create_branch(app: AppHandle, state: State<'_, AppState>, name: String) -> CommandResult<()> {
    git::in_repository(&state, |repository| {
        abstract_tex_git::create_branch(repository, &name)
    })?;
    git::emit_status_changed(&app);
    Ok(())
}

/// Switch to another local branch, rewriting the files on disk to its version (B4). Refuses, and
/// touches nothing, while tracked files have uncommitted changes. The files that change are seen by
/// the watcher like any other outside edit, so open tabs reload through the usual flow.
#[tauri::command]
pub fn git_switch_branch(app: AppHandle, state: State<'_, AppState>, name: String) -> CommandResult<()> {
    git::in_repository(&state, |repository| {
        abstract_tex_git::switch_branch(repository, &name)
    })?;
    git::emit_status_changed(&app);
    Ok(())
}

/// Give a commit a name the Graph will show beside it (A11): a lightweight, local tag. The refusal
/// for a name Git cannot use, or one already taken, is the command's error, in the crate's sentence.
#[tauri::command]
pub fn git_create_tag(
    app: AppHandle,
    state: State<'_, AppState>,
    commit: String,
    name: String,
) -> CommandResult<()> {
    git::in_repository(&state, |repository| {
        abstract_tex_git::create_tag(repository, &commit, &name)
    })?;
    git::emit_status_changed(&app);
    Ok(())
}

/// One page of the history for the Graph section. `skip` rows in, at most `limit` rows out.
#[tauri::command]
pub fn git_log(state: State<'_, AppState>, skip: usize, limit: usize) -> CommandResult<Vec<CommitRow>> {
    Ok(git::with_repository(&state, |repository| {
        abstract_tex_git::log(repository, skip, limit)
    })?
    .unwrap_or_default())
}

/// Commit whatever is staged, and answer with the new commit's id.
///
/// Every refusal the crate can return — no message, nothing staged, no identity — arrives here
/// as its own sentence and is shown under the commit box. None of them is a dialog: they are all
/// fixed where the author is already standing.
#[tauri::command]
pub fn git_commit(app: AppHandle, state: State<'_, AppState>, message: String) -> CommandResult<String> {
    let id = git::in_repository(&state, |repository| {
        abstract_tex_git::commit(repository, &message)
    })?;
    git::emit_status_changed(&app);
    Ok(id)
}

/// Stage one file and commit it alone: the offer after a live session (S15.2). Refuses, in a
/// sentence, if other files are staged.
#[tauri::command]
pub fn git_commit_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    message: String,
) -> CommandResult<String> {
    let id = git::in_repository(&state, |repository| {
        abstract_tex_git::commit_file(repository, &path, &message)
    })?;
    git::emit_status_changed(&app);
    Ok(id)
}

/// Replace `HEAD` with a new commit carrying `message` and whatever is staged — the Commit
/// dropdown's *Amend* (S11.1c). Disk-only, like `git_commit`, so this needs no `spawn_blocking`.
///
/// The panel is what decides whether this item is even offered — `canAmend` in `git.svelte.ts`,
/// reading the same `ahead_behind` the Sync button does — so a call that reaches here at all is
/// already believed safe; nothing about "already pushed" is checked again on this side.
#[tauri::command]
pub fn git_amend(app: AppHandle, state: State<'_, AppState>, message: String) -> CommandResult<String> {
    let id = git::in_repository(&state, |repository| abstract_tex_git::amend(repository, &message))?;
    git::emit_status_changed(&app);
    Ok(id)
}

/// What has changed since the last commit, in a writer's units (S10.3c).
///
/// Numbers and section names; the sentence the commit box is filled with is the frontend's, the
/// same split `CommitRow::time` already makes.
#[tauri::command]
pub fn git_prose_summary(state: State<'_, AppState>) -> CommandResult<Option<ProseSummary>> {
    git::with_repository(&state, abstract_tex_git::prose_summary)
}

/// Make the open project a Git repository (S10.5a): `init`, a `.gitignore`, and a first commit.
///
/// Refuses a folder already inside a repository, naming it — `git init` in a subfolder of one is
/// almost never what anyone meant. The answer says whether the commit happened, because on a
/// machine where Git has no identity yet it deliberately does not.
#[tauri::command]
pub fn git_initialise(app: AppHandle, state: State<'_, AppState>) -> CommandResult<Initialised> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    let done = abstract_tex_git::initialise(&root).map_err(to_message)?;
    git::emit_status_changed(&app);
    Ok(done)
}

/// Whether this repository already tells Git to ignore `.abstract-tex/` (S10.5a).
///
/// `Some(false)` is the state S10.2a's outcome note left open: a repository that existed before
/// this app, whose build folder the panel filters out rather than relying on a line for. `None`
/// is a project with no repository at all, where the question does not arise.
#[tauri::command]
pub fn git_our_folder_is_ignored(state: State<'_, AppState>) -> CommandResult<Option<bool>> {
    git::with_repository(&state, |repository| {
        Ok(abstract_tex_git::our_folder_is_ignored(repository))
    })
}

/// Add our lines to the repository's `.gitignore` — only ever after the author said yes.
#[tauri::command]
pub fn git_ignore_our_folder(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    git::in_repository(&state, abstract_tex_git::ignore_our_folder)?;
    git::emit_status_changed(&app);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// S10.4b: signing in to GitHub.
//
// Four commands, and none of them ever returns a token: what the window is told is a code to
// type, a URL to type it into, and afterwards a login name (`github.rs` says why).
// ---------------------------------------------------------------------------------------------

/// Whose GitHub account this machine is signed in to, or `None`.
///
/// `None` rather than an error when nobody has signed in: not being signed in is the normal
/// state of a fresh install, and the panel shows an offer rather than a problem.
#[tauri::command]
pub async fn github_account(state: State<'_, AppState>) -> CommandResult<Option<Account>> {
    crate::github::account(&state.github).await.map_err(to_message)
}

/// Start signing in. Returns as soon as GitHub has issued a code; the rest arrives as
/// `github:sign-in` events, because the middle of this is somebody walking to their browser.
#[tauri::command]
pub async fn github_sign_in(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    crate::github::start_sign_in(app, &state.github).map_err(to_message)
}

/// Stop waiting. Not an error when nothing is waiting: the panel can be closed at any moment.
#[tauri::command]
pub fn github_cancel_sign_in(state: State<'_, AppState>) -> CommandResult<()> {
    state.github.cancel();
    Ok(())
}

/// Forget the token. The keychain entry is removed, so the next start is signed out.
///
/// `async` with `spawn_blocking`, so the keychain call (which can stop for a password) is not made on
/// the thread that draws the window (review finding R5.9). A `State<'_, _>` borrows from the app and
/// cannot move into that thread, so the command clones the `Arc` around the session first.
#[tauri::command]
pub async fn github_sign_out(state: State<'_, AppState>) -> CommandResult<()> {
    let github = Arc::clone(&state.github);
    tauri::async_runtime::spawn_blocking(move || github.sign_out().map_err(to_message))
        .await
        .map_err(to_message)?
}

/// Create a repository on GitHub for the open project and set it as `origin` (S10.5b).
///
/// `visibility` carries its own confirmation: a public repository cannot be asked for without
/// the answer to the dialog §5.7 requires, and both this layer and the crate refuse one without
/// it. Nothing is pushed — `Sync` is S11.1.
#[tauri::command]
pub async fn github_create_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    visibility: Visibility,
    description: Option<String>,
) -> CommandResult<GitHubRepository> {
    let project_dir = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    let wanted = NewRepository {
        name,
        visibility,
        description,
    };
    let created = crate::github::create_repository(&state.github, project_dir, wanted)
        .await
        .map_err(to_message)?;
    git::emit_status_changed(&app);
    Ok(created)
}

/// The repositories the signed-in account can clone, most recently pushed first (S11.5b).
#[tauri::command]
pub async fn github_list_repositories(state: State<'_, AppState>) -> CommandResult<Vec<GitHubRepository>> {
    crate::github::list_repositories(&state.github)
        .await
        .map_err(to_message)
}

/// Clone `url` into a new folder inside `parent_dir`, named `folder_name` or, by default, after
/// the repository. Answers with the folder, for the window to open as it opens any other (S11.5b).
///
/// No project needs to be open, and none is touched: the window decides to open the result.
#[tauri::command]
pub async fn git_clone<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    url: String,
    parent_dir: String,
    folder_name: Option<String>,
) -> CommandResult<String> {
    let destination = git::clone_destination(Path::new(&parent_dir), &url, folder_name.as_deref())?;
    let about_to_be_made = destination.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A folder with something in it is refused below, and keeps what it was allowed.
        let holds_something =
            std::fs::read_dir(&about_to_be_made).is_ok_and(|mut inside| inside.next().is_some());
        if holds_something {
            return Ok(());
        }
        crate::start::forget_permissions(&app, &about_to_be_made)
    })
    .await
    .map_err(to_message)??;
    let token = saved_github_token(&state.github).await?;
    let cloned = git::clone_blocking(url.trim().to_string(), destination, token).await?;
    Ok(cloned.to_string_lossy().into_owned())
}

/// The starter templates, for the New project window's cards (S11.11).
#[tauri::command]
pub fn template_list() -> CommandResult<Vec<crate::templates::TemplateInfo>> {
    crate::templates::list()
}

/// Make a new project from a starter template inside `parent_dir`, in a folder named after the
/// title, and answer with the folder for the window to open (S11.11).
///
/// No project needs to be open, and none is touched.
#[tauri::command]
pub async fn template_create<R: Runtime>(
    app: AppHandle<R>,
    template_id: String,
    parent_dir: String,
    fields: std::collections::BTreeMap<String, String>,
) -> CommandResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let folder = crate::templates::destination_for(Path::new(&parent_dir), &fields, &template_id)?;
        // Before the folder is made: if this fails, there is nothing that could have them.
        crate::start::forget_permissions(&app, &folder)?;
        crate::templates::create(&folder, &template_id, &fields)?;
        // The title as it was typed (S17.3): a letter has no `\title` to read it back from.
        if let Some(recents) = app.try_state::<RecentDocuments>() {
            let root_file = crate::project::detect_root(&folder).unwrap_or_else(|| PathBuf::from("main.tex"));
            let title = fields.get("title").map(String::as_str);
            if let Err(error) = recents.record(&folder, &root_file, title, crate::start::now()) {
                tracing::warn!(%error, "could not record the new document");
            }
        }
        Ok(folder.to_string_lossy().into_owned())
    })
    .await
    .map_err(to_message)?
}

/// The URL of this project's `origin`, or `None` when it has no remote — and `None` too when
/// there is no repository at all, which the panel tells apart by `git_status` (S10.5b).
#[tauri::command]
pub fn git_origin_url(state: State<'_, AppState>) -> CommandResult<Option<String>> {
    Ok(git::with_repository(&state, |repository| Ok(abstract_tex_git::origin_url(repository)))?.flatten())
}

// ---------------------------------------------------------------------------------------------
// S11.1b: the Sync Changes button, and the Commit dropdown's *Commit & Push* / *Commit & Sync*.
//
// Both talk to a network, so both go through `git::in_repository_blocking` rather than
// `in_repository` — the distinction that module's own doc comment explains. Both read the token
// the same way `github_create_repository` does: `None` on a signed-out machine, offered to the
// remote only if it actually asks for one (`abstract_tex_git`'s own doc comment on `credentials`).
// ---------------------------------------------------------------------------------------------

/// Push the current branch to `origin` — the Commit dropdown's *Commit & Push*.
///
/// No fetch first: an author choosing this over *Commit & Sync* is saying "I know where this
/// goes." It can still fail — libgit2 refuses a stale branch before sending anything — but it can
/// never silently discard a coauthor's commit (checked by hand, `abstract-tex-git`'s own tests).
#[tauri::command]
pub async fn git_push(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    let token = saved_github_token(&state.github).await?;
    git::in_repository_blocking(&state, move |repository| {
        abstract_tex_git::push(repository, token.as_deref())
    })
    .await?;
    git::emit_status_changed(&app);
    Ok(())
}

/// The one-verb path (DESIGN.md §5.7): fetch, then push, fast-forward, or refuse a genuine
/// divergence. Both the standalone *Sync Changes ↑n ↓m* button and the dropdown's *Commit & Sync*
/// call this — the panel turns the answer into the sentence that goes under the button.
#[tauri::command]
pub async fn git_sync(app: AppHandle, state: State<'_, AppState>) -> CommandResult<SyncOutcome> {
    let token = saved_github_token(&state.github).await?;
    let outcome = git::in_repository_blocking(&state, move |repository| {
        abstract_tex_git::sync(repository, token.as_deref())
    })
    .await?;
    git::emit_status_changed(&app);
    Ok(outcome)
}

// ---------------------------------------------------------------------------------------------
// S11.3c: the Source Control view's "track with Git LFS" banner action.
// ---------------------------------------------------------------------------------------------

/// Track every path in `paths` with Git LFS. Rejects with a sentence — usually that Git LFS is
/// not installed on this machine — through the same refusal slot every other verb uses; the
/// banner is not a dialog, and neither is what happens when it is wrong.
#[tauri::command]
pub async fn git_track_with_lfs(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CommandResult<()> {
    let root = with_project(&state, |project| Ok(project.root_dir.clone()))?;
    crate::lfs::track(&root, &paths).await.map_err(to_message)?;
    git::emit_status_changed(&app);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// S11.4c: change review between two commits, compiled (DESIGN.md §5.7).
// ---------------------------------------------------------------------------------------------

/// Compare two commits as a marked-up PDF. `a` and `b` are full commit ids in the order the
/// author clicked them; the comparison is always older → newer.
///
/// Every refusal comes back as this command's error, in a sentence, before anything is written:
/// no `latexdiff`, no root file in one of the two commits. The compile itself then reports as
/// `compile-diff` events with the generation this returns. No snapshot here, unlike `compile` —
/// a marked-up document is not a recoverable state of anything the author wrote.
#[tauri::command]
pub async fn compare_revisions(
    app: AppHandle,
    state: State<'_, AppState>,
    consent: State<'_, ShellEscapeConsent>,
    a: String,
    b: String,
    simpler: Option<bool>,
) -> CommandResult<ComparisonStarted> {
    let request = with_project(&state, |project| {
        let root_file = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("This document has no main .tex file to compare."))?;
        Ok(ComparisonRequest {
            project_dir: project.root_dir.clone(),
            root_file,
            latexdiff_dir: project.latexdiff_dir(),
            shell_escape: consent.allows(&project.root_dir),
            a,
            b,
            markup: latexdiff::markup_from(simpler),
        })
    })?;
    latexdiff::compare(&state.diff_lane, request, move |event| {
        let _ = app.emit(latexdiff::COMPILE_DIFF, event);
    })
    .await
}

/// Stop the comparison in progress, because the author left it (*Back to live PDF*, ×, `Esc`).
/// Safe with nothing running. The live build is a different lane and is never touched.
#[tauri::command]
pub fn cancel_comparison(state: State<'_, AppState>) -> CommandResult<()> {
    state.diff_lane.cancel();
    Ok(())
}

/// The full TeX transcript of the comparison on disk, for the drawer's "Raw output" — one click
/// away and never the default (DESIGN.md §2). Empty when there is none; takes no path, because the
/// folder holds only the latest comparison.
#[tauri::command]
pub fn read_comparison_log(state: State<'_, AppState>) -> CommandResult<String> {
    with_project(&state, |project| {
        let root_file = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("This document has no main .tex file."))?;
        Ok(latexdiff::read_comparison_log(
            &project.latexdiff_dir(),
            &root_file,
        ))
    })
}

/// Copy the comparison of `older` and `newer` to `destination`, a path the author just picked in
/// the save dialog (design interview A9). Named by its pair rather than "whatever is showing", so
/// a comparison replaced while the dialog was open can never be saved under the other one's name.
#[tauri::command]
pub fn save_comparison_pdf(
    state: State<'_, AppState>,
    older: String,
    newer: String,
    destination: String,
    simpler: Option<bool>,
) -> CommandResult<()> {
    let (latexdiff_dir, root_file) = with_project(&state, |project| {
        let root_file = project
            .root_file()
            .ok_or_else(|| anyhow::anyhow!("This document has no main .tex file."))?;
        Ok((project.latexdiff_dir(), root_file))
    })?;
    let dirs = latexdiff::ComparisonDirs::new(
        &latexdiff_dir,
        latexdiff::parse_commit(&older)?,
        latexdiff::parse_newer(&newer)?,
        latexdiff::markup_from(simpler),
    );
    let pdf = dirs.pdf(&root_file);
    if !pdf.is_file() {
        return Err("That comparison is no longer on disk; compare the two commits again, then save.".into());
    }
    std::fs::copy(&pdf, &destination).map(|_| ()).map_err(to_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> + use<> {
        list.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn a_quoted_path_with_spaces_opens() {
        let dir = tempfile::tempdir().unwrap();
        let with_spaces = dir.path().join("My Thesis");
        std::fs::create_dir(&with_spaces).unwrap();
        let whole = with_spaces.to_string_lossy().into_owned();
        assert_eq!(folder_from_args(args(&[&whole])), Some(whole));
    }

    /// The case from the smoke run: the shell split the path on its spaces before we saw it.
    #[test]
    fn an_unquoted_path_with_spaces_is_rejoined() {
        let dir = tempfile::tempdir().unwrap();
        let with_spaces = dir.path().join("My Thesis");
        std::fs::create_dir(&with_spaces).unwrap();
        let whole = with_spaces.to_string_lossy().into_owned();
        let split: Vec<&str> = whole.split(' ').collect();
        assert_eq!(folder_from_args(args(&split)), Some(whole));
    }

    #[test]
    fn a_path_without_spaces_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().to_string_lossy().into_owned();
        assert_eq!(folder_from_args(args(&[&plain])), Some(plain));
    }

    #[test]
    fn nothing_is_opened_when_the_argument_is_not_a_directory() {
        assert_eq!(folder_from_args(args(&["/definitely/not/here"])), None);
        assert_eq!(folder_from_args(args(&[])), None);
    }

    #[test]
    fn flags_are_not_mistaken_for_paths() {
        assert_eq!(folder_from_args(args(&["--devtools"])), None);
    }

    /// A token store that notes which thread cleared it (see the assistant's `ThreadNotingKeys`).
    struct ThreadNotingStore {
        cleared_on: Arc<std::sync::Mutex<Vec<std::thread::ThreadId>>>,
    }

    impl abstract_tex_github::SecretStore for ThreadNotingStore {
        fn save(&self, _token: &str) -> Result<(), abstract_tex_github::GitHubError> {
            Ok(())
        }
        fn read(&self) -> Result<Option<String>, abstract_tex_github::GitHubError> {
            Ok(None)
        }
        fn clear(&self) -> Result<(), abstract_tex_github::GitHubError> {
            self.cleared_on.lock().unwrap().push(std::thread::current().id());
            Ok(())
        }
    }

    // The same finding as the assistant's keychain commands (R5.9): deleting the token asks the OS
    // keychain, which can stop for a password, and a plain command runs on the thread that draws the
    // window. Driven through Tauri's mock runtime, where a sync command runs on the calling thread
    // and an async one does not (the assistant's test explains the setup).
    #[test]
    fn signing_out_does_not_ask_the_keychain_on_the_thread_that_called_it() {
        let cleared_on = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state = AppState {
            github: Arc::new(crate::github::GitHubSession::with_store(Box::new(
                ThreadNotingStore {
                    cleared_on: Arc::clone(&cleared_on),
                },
            ))),
            ..AppState::new()
        };
        let app = tauri::test::mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![github_sign_out])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "github_sign_out".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({})),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .unwrap_or_else(|error| panic!("github_sign_out failed: {error}"));

        let threads = cleared_on.lock().unwrap();
        assert_eq!(threads.len(), 1, "signing out should clear the store once");
        assert_ne!(
            threads[0],
            std::thread::current().id(),
            "the token was cleared on the calling thread"
        );
    }

    // S17.2. `abstract-tex.toml` is a pipe nobody writes to, so reading it never returns: the nearest
    // thing to a drive that stopped answering. `on_message` runs in a thread of its own, so a command
    // that reads on the thread that called it fails the first assertion instead of hanging the test.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_never_answers_leaves_the_window_free_and_can_be_stopped() {
        use std::sync::mpsc;
        use std::time::Duration;

        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let pipe = folder.path().join(crate::project::CONFIG_FILE);
        let made = std::process::Command::new("mkfifo").arg(&pipe).status().unwrap();
        assert!(made.success());

        let app = tauri::test::mock_builder()
            .manage(AppState::new())
            .invoke_handler(tauri::generate_handler![open_project])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let (asked, has_asked) = mpsc::channel();
        let (answer, answered) = mpsc::channel();
        let path = folder.path().to_string_lossy().into_owned();
        std::thread::spawn(move || {
            webview.on_message(
                tauri::webview::InvokeRequest {
                    cmd: "open_project".into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(serde_json::json!({ "path": path })),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
                Box::new(move |_webview, _command, response, _callback, _error| {
                    let _ = answer.send(response);
                }),
            );
            let _ = asked.send(());
        });

        assert!(
            has_asked.recv_timeout(Duration::from_secs(2)).is_ok(),
            "the thread that asked did not wait"
        );
        assert!(
            answered.recv_timeout(Duration::from_millis(300)).is_err(),
            "still reading"
        );
        assert!(app.state::<AppState>().openings.stop());
        assert!(matches!(
            answered.recv_timeout(Duration::from_secs(2)),
            Ok(tauri::ipc::InvokeResponse::Err(_))
        ));
        assert!(app.state::<AppState>().project.lock().unwrap().is_none());
        drop(std::fs::OpenOptions::new().write(true).open(&pipe)); // lets the stuck read end
    }

    // The other half of *Stop*: the read that was left behind can still end, long after, and must then
    // change nothing.
    #[test]
    fn a_folder_that_answers_after_it_was_stopped_does_not_become_the_open_document() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("main.tex"), "\\documentclass{article}").unwrap();
        let app = tauri::test::mock_builder()
            .manage(AppState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let state = app.state::<AppState>();

        let stopped = state.openings.begin();
        assert!(state.openings.stop());
        assert!(open_folder(app.handle(), folder.path(), &stopped).is_err());
        assert!(state.project.lock().unwrap().is_none());
        assert!(state.watcher.lock().unwrap().is_none());

        let opening = state.openings.begin();
        open_folder(app.handle(), folder.path(), &opening).unwrap();
        assert!(state.project.lock().unwrap().is_some());
    }

    /// An app with an empty recent list of its own, for the two tests below.
    fn app_with_recents(config: &tempfile::TempDir) -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(AppState::new())
            .manage(RecentDocuments::at(config.path().join("recent-documents.toml")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap()
    }

    #[test]
    fn opening_records_the_document_under_its_title_and_a_stopped_or_rootless_one_is_not_recorded() {
        let (config, folder, empty) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        std::fs::write(
            folder.path().join("main.tex"),
            "\\documentclass{article}\\title{On Bees}",
        )
        .unwrap();
        let app = app_with_recents(&config);
        let state = app.state::<AppState>();
        let recents = app.state::<RecentDocuments>();

        let stopped = state.openings.begin();
        state.openings.stop();
        assert!(open_folder(app.handle(), folder.path(), &stopped).is_err());
        open_folder(app.handle(), empty.path(), &state.openings.begin()).unwrap();
        assert_eq!(recents.list(), []);

        let info = open_folder(app.handle(), folder.path(), &state.openings.begin()).unwrap();
        assert_eq!(info.title, "On Bees");
        assert_eq!(listed_after_a_while(&app, 1)[0].title, "On Bees");
        // A refreshed tree carries it too.
        let refreshed = state.project.lock().unwrap().as_mut().unwrap().info();
        assert_eq!(refreshed.title, "On Bees");
    }

    /// The place for new documents can be on a drive that stopped answering, while the document is not.
    #[test]
    fn a_dead_drive_elsewhere_does_not_hold_up_an_opening() {
        use std::time::{Duration, Instant};
        let (config, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        std::fs::write(
            folder.path().join("main.tex"),
            "\\documentclass{article}\\title{On Bees}",
        )
        .unwrap();
        let preferences = crate::preferences::PreferencesFile::at(config.path().join("preferences.toml"));
        let dead = config.path().join("network");
        preferences
            .change(|kept| kept.place = Some(dead.to_string_lossy().into_owned()))
            .unwrap();
        let app = app_with_recents(&config);
        app.manage(preferences);
        app.manage(abstract_tex_import::SlowPlaces::asking(move |place| {
            if place == dead {
                std::thread::sleep(Duration::from_secs(20));
            }
            place.canonicalize().ok()
        }));
        let asked = Instant::now();
        let opening = app.state::<AppState>().openings.begin();
        let info = open_folder(app.handle(), folder.path(), &opening).unwrap();
        assert!(
            asked.elapsed() < Duration::from_millis(800),
            "{:?}",
            asked.elapsed()
        );
        assert_eq!(info.title, "On Bees");
        assert_eq!(listed_after_a_while(&app, 1)[0].title, "On Bees");
        // Nor does what is asked after every build, or a file somebody chose.
        let asked = Instant::now();
        let file = folder.path().join("main.tex").to_string_lossy().into_owned();
        tauri::async_runtime::block_on(async {
            for _ in 0..5 {
                crate::start::document_title(app.handle().clone()).await.unwrap();
                crate::start::inspect_choice(app.handle().clone(), file.clone())
                    .await
                    .unwrap();
            }
        });
        assert!(asked.elapsed() < Duration::from_secs(1), "{:?}", asked.elapsed());
    }

    /// The list, once it has `wanted` documents: they are recorded after the opening has answered.
    fn listed_after_a_while(
        app: &tauri::App<tauri::test::MockRuntime>,
        wanted: usize,
    ) -> Vec<crate::recents::RecentDocument> {
        for _ in 0..500 {
            let listed = app.state::<RecentDocuments>().list();
            if listed.len() == wanted {
                return listed;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("{wanted} documents were never listed");
    }

    #[test]
    fn a_new_letter_is_recorded_under_the_title_that_was_typed() {
        let (config, parent) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with_recents(&config);
        let fields = [("title", "My request"), ("author", "Ana")]
            .map(|(id, value)| (id.to_string(), value.to_string()));
        let folder = tauri::async_runtime::block_on(template_create(
            app.handle().clone(),
            "letter".into(),
            parent.path().to_string_lossy().into_owned(),
            fields.into(),
        ))
        .unwrap();

        let recents = app.state::<RecentDocuments>();
        assert_eq!(recents.list()[0].folder, folder);
        assert_eq!(recents.list()[0].title, "My request");
        let opening = app.state::<AppState>().openings.begin();
        let info = open_folder(app.handle(), Path::new(&folder), &opening).unwrap();
        assert_eq!(
            info.title, "My request",
            "a letter has no title to read it back from"
        );
    }

    /// An app that has permissions to inherit, and a letter to make in `parent`.
    fn app_with_permissions(config: &Path) -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(AppState::new())
            .manage(ShellEscapeConsent::at(config.join("shell-escape.toml")))
            .manage(crate::assistant::AssistantSettings::at(
                config.join("assistant.json"),
            ))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap()
    }

    fn make_letter(app: &tauri::App<tauri::test::MockRuntime>, parent: &Path) -> CommandResult<String> {
        let fields = [("title", "My request"), ("author", "Ana")]
            .map(|(id, value)| (id.to_string(), value.to_string()));
        tauri::async_runtime::block_on(template_create(
            app.handle().clone(),
            "letter".into(),
            parent.to_string_lossy().into_owned(),
            fields.into(),
        ))
    }

    #[test]
    fn a_new_document_inherits_no_permission_from_a_folder_that_used_to_be_there() {
        let (config, parent) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with_permissions(config.path());
        let consent = app.state::<ShellEscapeConsent>();
        let settings = app.state::<crate::assistant::AssistantSettings>();
        let (folder, other) = (parent.path().join("my-request"), parent.path().join("other"));
        for allowed in [&folder, &other] {
            consent.allow(allowed).unwrap();
            crate::assistant::set_enabled(&settings, allowed, true).unwrap();
        }
        // Reached by a path that is not the one it was allowed under, as a picker may answer.
        make_letter(&app, &parent.path().join(".")).unwrap();

        assert!(!consent.allows(&folder));
        assert!(!settings.is_enabled_for(&folder));
        assert!(
            consent.allows(&other) && settings.is_enabled_for(&other),
            "only that folder's"
        );
    }

    // Found by the sprint's final review: what arrives from a Git address is the least known of all.
    #[test]
    fn a_clone_inherits_no_permission_and_one_that_is_refused_takes_none_away() {
        let (config, parent) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // A keychain that refuses, so that nothing is fetched: what is asked here happens before.
        let state = AppState {
            github: Arc::new(crate::github::GitHubSession::with_store(Box::new(
                WorkerNotingStore {
                    reads_on_an_async_worker: Arc::default(),
                },
            ))),
            ..AppState::new()
        };
        let app = tauri::test::mock_builder()
            .manage(state)
            .manage(ShellEscapeConsent::at(config.path().join("shell-escape.toml")))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let consent = app.state::<ShellEscapeConsent>();
        let (gone, kept) = (parent.path().join("gone"), parent.path().join("kept"));
        std::fs::create_dir(&kept).unwrap();
        std::fs::write(kept.join("main.tex"), "mine").unwrap();
        for folder in [&gone, &kept] {
            consent.allow(folder).unwrap();
            let name = crate::recents::folder_name(folder);
            let answer = tauri::async_runtime::block_on(git_clone(
                app.handle().clone(),
                app.state::<AppState>(),
                "https://github.com/ada/thesis.git".into(),
                parent.path().to_string_lossy().into_owned(),
                Some(name),
            ));
            assert!(answer.is_err());
        }
        assert!(!consent.allows(&gone));
        assert!(
            consent.allows(&kept),
            "a folder with something in it is not cloned into"
        );
    }

    #[cfg(unix)] // Windows says "not found" of a path under a file, and then there is nothing to drop.
    #[test]
    fn when_the_permissions_cannot_be_dropped_nothing_is_made() {
        let (config, parent) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let config_folder = config.path().join("app");
        std::fs::write(&config_folder, "a file where the folder should be").unwrap();
        let app = app_with_permissions(&config_folder);
        assert!(make_letter(&app, parent.path()).is_err());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    /// Whether the calling thread is one of the async runtime's workers rather than a thread from
    /// its blocking pool. Waiting is allowed only on the second kind: starting a runtime inside an
    /// async worker panics, and inside a blocking-pool thread it does not.
    fn is_on_an_async_worker() -> bool {
        std::panic::catch_unwind(|| tauri::async_runtime::block_on(async {})).is_err()
    }

    /// A token store that notes, for each read, whether it ran on an async worker; and then
    /// refuses with a sentence, so the command under test stops before it touches a network.
    struct WorkerNotingStore {
        reads_on_an_async_worker: Arc<std::sync::Mutex<Vec<bool>>>,
    }

    impl abstract_tex_github::SecretStore for WorkerNotingStore {
        fn save(&self, _token: &str) -> Result<(), abstract_tex_github::GitHubError> {
            Ok(())
        }
        fn read(&self) -> Result<Option<String>, abstract_tex_github::GitHubError> {
            self.reads_on_an_async_worker
                .lock()
                .unwrap()
                .push(is_on_an_async_worker());
            Err(abstract_tex_github::GitHubError::Keychain(
                "the test keychain is closed".to_string(),
            ))
        }
        fn clear(&self) -> Result<(), abstract_tex_github::GitHubError> {
            Ok(())
        }
    }

    #[test]
    fn the_probe_tells_an_async_worker_from_a_blocking_thread() {
        let on_a_worker = tauri::async_runtime::block_on(async {
            tauri::async_runtime::spawn(async { is_on_an_async_worker() })
                .await
                .unwrap()
        });
        let on_a_blocking_thread = tauri::async_runtime::block_on(async {
            tauri::async_runtime::spawn_blocking(is_on_an_async_worker)
                .await
                .unwrap()
        });
        assert!(on_a_worker, "the probe should see an async worker as one");
        assert!(
            !on_a_blocking_thread,
            "the probe should see a blocking thread as one"
        );
    }

    // The tidy left over from R5.9. `git_push`, `git_sync`, `git_clone` and `comments_sync` read the
    // saved token (a keychain call, which can stop for a password) before they hand their work to
    // a blocking thread, so the wait landed on an async worker, and every other async command
    // sharing that worker waited with it. `git_push` and `git_sync` take an `AppHandle`, which the
    // mock runtime cannot supply, so they are covered through the helper they share, below.
    #[test]
    fn the_commands_that_send_the_token_to_github_do_not_read_it_on_an_async_worker() {
        let reads_on_an_async_worker = Arc::new(std::sync::Mutex::new(Vec::new()));
        let folder = tempfile::tempdir().unwrap();
        let state = AppState {
            github: Arc::new(crate::github::GitHubSession::with_store(Box::new(
                WorkerNotingStore {
                    reads_on_an_async_worker: Arc::clone(&reads_on_an_async_worker),
                },
            ))),
            project: std::sync::Mutex::new(Some(Project::open(folder.path()).unwrap())),
            ..AppState::new()
        };
        let app = tauri::test::mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![git_clone, comments_sync])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let invoke = |command: &str, body: serde_json::Value| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: command.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
        };

        let clone_error = invoke(
            "git_clone",
            serde_json::json!({
                "url": "https://github.com/ada/thesis.git",
                "parentDir": folder.path().to_string_lossy(),
            }),
        )
        .expect_err("the closed test keychain should stop the clone");
        let comments_error = invoke("comments_sync", serde_json::json!({}))
            .expect_err("the closed test keychain should stop the comments exchange");
        assert!(clone_error.to_string().contains("the test keychain is closed"));
        assert!(comments_error.to_string().contains("the test keychain is closed"));

        // The helper `git_push` and `git_sync` use, called the way they call it.
        let github = Arc::clone(&app.state::<AppState>().github);
        let helper_error = tauri::async_runtime::block_on(saved_github_token(&github))
            .expect_err("the closed test keychain should stop the helper");
        assert!(helper_error.contains("the test keychain is closed"));

        let reads = reads_on_an_async_worker.lock().unwrap();
        assert_eq!(
            reads.len(),
            3,
            "each of the three callers should read the store once"
        );
        assert_eq!(
            *reads,
            vec![false, false, false],
            "the token was read on an async worker (clone, comments, helper in that order)"
        );
    }
}
