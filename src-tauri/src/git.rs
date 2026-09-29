//! The open project's Git repository, for the Source Control view (DESIGN.md §6).
//!
//! Owns one question — *which repository is this project in* — and the event that tells the
//! frontend to ask its questions again. Everything else is `abstract_tex_git`'s: this module
//! exists so that "which repository" has a single answer and so that `commands.rs` stays as
//! thin about Git as it is about everything else.
//!
//! **What it must never do:**
//!
//! - Never keep a repository between commands. See [`with_repository`] for why that is a rule
//!   and not an optimisation left undone.
//! - Never turn "this folder is not in Git" into an error. It is the normal state of a folder
//!   nobody has run `git init` in — `Ok(None)` here, one sentence in the view, and the button
//!   that fixes it arrives in S10.5.

use abstract_tex_git::{GitError, Repository};
use tauri::{AppHandle, Emitter};

use crate::AppState;

/// The event that tells the frontend the answers may have moved.
///
/// No payload. The view asks three separate questions — the file lists, the branch, the graph —
/// and a payload would have to guess which of them this particular change touched; the frontend
/// asks for what it is currently showing instead. Emitted from the watcher (S10.3a: an edit, or
/// `git add` in a terminal) and from each verb below, so there is exactly one refresh path
/// rather than one per button.
pub const STATUS_CHANGED: &str = "git:status-changed";

pub fn emit_status_changed(app: &AppHandle) {
    // A failed emit means the window has gone; there is nobody left to tell.
    let _ = app.emit(STATUS_CHANGED, ());
}

/// Run `f` against the repository the open project belongs to.
///
/// `Ok(None)` is "this project is not inside a Git repository"; `Err` is reserved for no project
/// open and for a Git failure worth reporting as a sentence.
///
/// The repository is opened here and dropped when the call ends, and is deliberately not cached
/// in [`AppState`]: a kept handle would outlive the author switching project, running `git init`
/// in a terminal or deleting `.git`, and it carries an in-memory index that would then disagree
/// with the file on disk — rule 1 (plain files are the truth) inverted. `discover` costs a few
/// `stat`s, and nothing here is on the keystroke path.
pub fn with_repository<T>(
    state: &AppState,
    f: impl FnOnce(&Repository) -> Result<T, GitError>,
) -> Result<Option<T>, String> {
    // The project lock is taken for the length of a clone and released before libgit2 touches
    // the disk: a `status` on a large repository is milliseconds, but holding this lock across
    // it would make every other command wait on it.
    let root = {
        let guard = state.project.lock().unwrap();
        let project = guard.as_ref().ok_or_else(|| "No project is open.".to_string())?;
        project.root_dir.clone()
    };
    match abstract_tex_git::open(&root) {
        Ok(repository) => f(&repository).map(Some).map_err(|error| error.to_string()),
        Err(GitError::NotARepository) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Same, for the verbs. A stage, discard or commit can only be asked for from a row the view
/// drew, and it drew it because there *is* a repository — so a missing one here is a bug or a
/// repository deleted mid-session, and either way the honest answer is the sentence.
pub fn in_repository<T>(
    state: &AppState,
    f: impl FnOnce(&Repository) -> Result<T, GitError>,
) -> Result<T, String> {
    with_repository(state, f)?.ok_or_else(|| GitError::NotARepository.to_string())
}
