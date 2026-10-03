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

use std::path::{Path, PathBuf};

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

/// Same promise as [`in_repository`], for a verb that talks to a network and so might take real
/// wall-clock time rather than a handful of `stat`s — `push` and `sync` (S11.1b), unlike every
/// verb above.
///
/// `spawn_blocking`: `git2`'s calls are synchronous, and running one on an async worker thread
/// would stall every other command sharing it for as long as the network takes. The closure reopens
/// the repository itself rather than being handed one, for the same reason [`with_repository`]
/// never keeps one between commands — `git2::Repository` is not `Send` in any case, so one made on
/// this thread could not cross into the spawned one.
pub async fn in_repository_blocking<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&Repository) -> Result<T, GitError> + Send + 'static,
) -> Result<T, String> {
    let root = {
        let guard = state.project.lock().unwrap();
        let project = guard.as_ref().ok_or_else(|| "No project is open.".to_string())?;
        project.root_dir.clone()
    };
    tauri::async_runtime::spawn_blocking(move || abstract_tex_git::open(&root).and_then(|repository| f(&repository)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// Where a clone of `url` goes: a folder named `name` (or, when the author gave none, the one
/// `git clone` would pick) inside `parent`.
///
/// Pure, so the refusals are tested without a window. `name` is typed by the author and joined to
/// a path, so it must be one folder's name and nothing that walks elsewhere: no separators, no
/// `..`, and none of the characters Windows refuses in a name, which is also where this
/// app's projects get synced to.
pub fn clone_destination(parent: &Path, url: &str, name: Option<&str>) -> Result<PathBuf, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Paste the address of a repository to clone.".to_string());
    }
    let chosen = match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => abstract_tex_git::folder_name_for(url)
            .ok_or_else(|| "That address does not name a repository. Give the folder a name to clone into.".to_string())?,
    };
    let is_one_plain_name = chosen != "."
        && chosen != ".."
        && !chosen.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    if !is_one_plain_name {
        return Err(format!("\"{chosen}\" cannot be used as a folder name."));
    }
    Ok(parent.join(chosen))
}

/// Download `url` into `destination` on a blocking thread — the same reason
/// [`in_repository_blocking`] gives — and return the folder to open.
///
/// There is no project and so no repository to open first, which is why this is not an
/// `in_repository_blocking` call. `token` is the GitHub sign-in or `None`; the git crate offers it
/// only to github.com (`abstract_tex_git::is_github_https`), so passing it for any URL is safe.
pub async fn clone_blocking(url: String, destination: PathBuf, token: Option<String>) -> Result<PathBuf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        abstract_tex_git::clone(&url, &destination, token.as_deref()).map(|_| destination)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_named_after_the_repository_unless_the_author_says_otherwise() {
        let parent = Path::new("/papers");
        assert_eq!(clone_destination(parent, "https://github.com/ada/thesis.git", None).unwrap(), parent.join("thesis"));
        assert_eq!(clone_destination(parent, "https://github.com/ada/thesis.git", Some("  draft  ")).unwrap(), parent.join("draft"));
        assert_eq!(clone_destination(parent, "https://github.com/ada/thesis.git", Some("")).unwrap(), parent.join("thesis"));
    }

    #[test]
    fn an_address_with_nothing_to_name_a_folder_asks_for_a_name_and_a_given_one_is_enough() {
        let parent = Path::new("/papers");
        assert!(clone_destination(parent, "https://github.com/", None).unwrap_err().contains("Give the folder a name"));
        assert!(clone_destination(parent, "https://github.com/", Some("paper")).is_ok());
        assert!(clone_destination(parent, "   ", None).unwrap_err().contains("Paste the address"));
    }

    #[test]
    fn a_name_that_is_not_one_plain_folder_is_refused() {
        let parent = Path::new("/papers");
        for bad in ["..", "a/b", r"a\b", "../elsewhere", "C:", "what?", "a|b"] {
            assert!(clone_destination(parent, "https://github.com/ada/t.git", Some(bad)).is_err(), "{bad} was accepted");
        }
        // The URL's own name goes through the same check.
        assert!(clone_destination(parent, "https://example.invalid/ada/..", None).is_err());
    }
}
