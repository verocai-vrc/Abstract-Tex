//! S10.2: the project's Git repository, as the Source Control view (DESIGN.md §6) needs it.
//!
//! This crate owns the question "what has changed, and what can I do about it" and nothing else.
//! It never talks to a network — S10.4 adds that, with credentials, somewhere else — and it never
//! knows about Tauri, so every behaviour here is testable against a temporary repository with
//! `cargo test`.
//!
//! **What it must never do:**
//!
//! - Never change the index or the working tree except when a named verb was called. `status`
//!   reads. `stage`, `unstage`, `discard` and `commit` write, and they are the only ones.
//! - Never present our own folder as the author's work. `.abstract-tex/` is ours (DESIGN.md
//!   §5.8) and is filtered out of everything here, whether or not a `.gitignore` says so.
//! - Never invent an identity. A commit signed by a stand-in is worse than a commit refused with
//!   a sentence, so a repository with no `user.name` gets [`GitError::NoIdentity`].
//!
//! Errors are a `thiserror` enum, the split `crates/abstract-tex-engine/src/lib.rs` explains
//! once: typed errors in a library, `anyhow` only at the app edge.

use std::path::{Path, PathBuf};

use git2::{Repository, Status as GitStatus, StatusOptions};

/// Folders whose contents are never part of the author's changes, whatever Git thinks.
///
/// `.abstract-tex` is ours by DESIGN.md §5.8 — the build folder, the draft folder, the warm
/// marker, and S10.1's snapshot repository. A project that already had Git before it met this
/// app has no `.gitignore` line for any of that, so without this filter the very first refresh
/// of the Source Control view would bury the manuscript under build junk. `.preamble` is the
/// same folder under the name it had before the rename.
const NEVER_A_CHANGE: [&str; 2] = [".abstract-tex/", ".preamble/"];

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("this folder is not inside a Git repository")]
    NotARepository,

    #[error("Git reported: {0}")]
    Git(#[from] git2::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// Not a `git2::Error`, because it is the one failure here with an answer the author can act
    /// on, and the app shows it as a sentence rather than as "Git reported: …".
    #[error("Git does not know who you are yet. Set a name and e-mail with `git config --global user.name` and `user.email`, then commit again.")]
    NoIdentity,
}

/// What one changed file is, in the words the row in the view uses.
///
/// The letters are DESIGN.md §6's row shape (`name · dir · M/U/A/D/R`). `Untracked` is `U` and
/// `Conflicted` has no letter of its own in that shape because a conflict is never shown as a
/// row with a letter — S11.2 shows it as two paragraphs — but it must be *distinguishable* here
/// from the start, or it arrives as "modified" and is discovered late.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// One row of the Changes or Staged Changes list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Project-relative, with `/` separators — the spelling Git uses and the frontend expects.
    pub path: String,
    /// Where it came from, when Git detected a rename. `None` otherwise.
    pub renamed_from: Option<String>,
    pub kind: ChangeKind,
}

/// Everything the Source Control view draws, in the three lists it draws them in.
///
/// Three, not one list with a flag: a file staged and then edited again is in *Staged Changes*
/// **and** in *Changes* at the same time, with a different letter in each, which is exactly what
/// VS Code shows and what an author needs to see to understand what a commit would contain.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub conflicted: Vec<FileChange>,
}

impl Status {
    /// What the activity bar's badge counts: every path with something to say about it, counted
    /// once however many lists it is in.
    pub fn changed_file_count(&self) -> usize {
        let mut paths: Vec<&str> =
            self.staged.iter().chain(&self.unstaged).chain(&self.conflicted).map(|change| change.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        paths.len()
    }
}

/// The repository a project folder belongs to.
///
/// `discover` and not `open`, for the reason S10.1 gives: a paper is often a subfolder of the
/// repository it lives in, and its history belongs wherever the author put it.
pub fn open(project_dir: &Path) -> Result<Repository, GitError> {
    Repository::discover(project_dir).map_err(|_| GitError::NotARepository)
}

/// What has changed, in the three lists the view draws.
pub fn status(repository: &Repository) -> Result<Status, GitError> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        // Without this an untracked folder is one row saying `figures/`, which is not something
        // an author can stage a file from.
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        // Both halves, because a rename is detected between HEAD and the index *or* between the
        // index and the working tree, and a chapter renamed on disk is the second one.
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let mut status = Status::default();
    for entry in repository.statuses(Some(&mut options))?.iter() {
        let flags = entry.status();

        // A conflict is not a staged change and not an unstaged one; it is its own thing, and
        // Git reports it alongside flags that would otherwise read as "modified".
        if flags.is_conflicted() {
            if let Some(change) = to_change(entry.path(), None, ChangeKind::Conflicted) {
                status.conflicted.push(change);
            }
            continue;
        }

        if let Some(kind) = staged_kind(flags) {
            let diff = entry.head_to_index();
            let path = diff.as_ref().and_then(|d| d.new_file().path()).map(path_string).or_else(|| entry.path().map(str::to_string));
            let from = diff.as_ref().and_then(|d| d.old_file().path()).map(path_string);
            if let Some(change) = to_change(path.as_deref(), from.filter(|_| kind == ChangeKind::Renamed), kind) {
                status.staged.push(change);
            }
        }
        if let Some(kind) = unstaged_kind(flags) {
            let diff = entry.index_to_workdir();
            let path = diff.as_ref().and_then(|d| d.new_file().path()).map(path_string).or_else(|| entry.path().map(str::to_string));
            let from = diff.as_ref().and_then(|d| d.old_file().path()).map(path_string);
            if let Some(change) = to_change(path.as_deref(), from.filter(|_| kind == ChangeKind::Renamed), kind) {
                status.unstaged.push(change);
            }
        }
    }
    Ok(status)
}

/// The letter for the index half of a status, or `None` when nothing is staged for this path.
fn staged_kind(flags: GitStatus) -> Option<ChangeKind> {
    // Order matters: Git sets several bits for one path (a rename is `RENAMED` *and* `MODIFIED`
    // when the content changed too), and the most specific one is the one the row should show.
    if flags.is_index_renamed() {
        Some(ChangeKind::Renamed)
    } else if flags.is_index_new() {
        Some(ChangeKind::Added)
    } else if flags.is_index_deleted() {
        Some(ChangeKind::Deleted)
    } else if flags.is_index_modified() || flags.is_index_typechange() {
        Some(ChangeKind::Modified)
    } else {
        None
    }
}

/// The letter for the working-tree half. Untracked lives here and only here: a file Git has
/// never seen cannot be in the index.
fn unstaged_kind(flags: GitStatus) -> Option<ChangeKind> {
    if flags.is_wt_new() {
        Some(ChangeKind::Untracked)
    } else if flags.is_wt_renamed() {
        Some(ChangeKind::Renamed)
    } else if flags.is_wt_deleted() {
        Some(ChangeKind::Deleted)
    } else if flags.is_wt_modified() || flags.is_wt_typechange() {
        Some(ChangeKind::Modified)
    } else {
        None
    }
}

/// A row, unless the path is one of ours.
fn to_change(path: Option<&str>, renamed_from: Option<String>, kind: ChangeKind) -> Option<FileChange> {
    let path = path?;
    if is_ours(path) {
        return None;
    }
    Some(FileChange { path: path.to_string(), renamed_from, kind })
}

fn is_ours(path: &str) -> bool {
    NEVER_A_CHANGE.iter().any(|ours| path.starts_with(ours))
}

/// Git always reports `/`, whatever the platform; this only exists because `Path` does not.
fn path_string(path: &Path) -> String {
    path.components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join("/")
}

/// Stage one path: add it to the index, or record its deletion there.
///
/// `add_path` cannot record a deletion — it reads the file — so a path that is gone from the
/// working tree goes through `remove_path` instead. Getting this wrong is the classic "staging a
/// deleted file does nothing" bug, and it is invisible until someone deletes a chapter.
pub fn stage(repository: &Repository, path: &str) -> Result<(), GitError> {
    let mut index = repository.index()?;
    let relative = Path::new(path);
    if workdir_path(repository, path).is_some_and(|full| full.exists()) {
        index.add_path(relative)?;
    } else {
        index.remove_path(relative)?;
    }
    index.write()?;
    Ok(())
}

/// Unstage one path: put the index entry back to what `HEAD` has, leaving the working tree alone.
///
/// This is `git restore --staged`, and `reset_default` is its libgit2 spelling — it resets the
/// *index* entry from a commit and never touches the file on disk, which is the whole distinction
/// between unstaging and discarding. A repository with no commits yet has no `HEAD` to reset
/// from, so there the entry is simply removed, which leaves the file untracked again.
pub fn unstage(repository: &Repository, path: &str) -> Result<(), GitError> {
    match repository.head().and_then(|head| head.peel_to_commit()) {
        Ok(commit) => repository.reset_default(Some(commit.as_object()), [path])?,
        Err(_) => {
            let mut index = repository.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}

/// What a discard actually did, so the view's confirmation can say the true thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Discarded {
    /// A tracked file, put back to the version in the index or `HEAD`.
    Restored,
    /// An untracked file, deleted. There is nothing to put it back from: this is the one action
    /// in the crate that destroys work Git has never seen, which is why the design notes say
    /// "Discard always confirms" and why this variant exists rather than a bare `Ok(())`.
    Deleted,
}

/// Throw away the working-tree changes to one path.
pub fn discard(repository: &Repository, path: &str) -> Result<Discarded, GitError> {
    let tracked = repository.status_file(Path::new(path)).map(|flags| !flags.is_wt_new()).unwrap_or(false);
    if !tracked {
        if let Some(full) = workdir_path(repository, path) {
            if full.exists() {
                std::fs::remove_file(full)?;
            }
        }
        return Ok(Discarded::Deleted);
    }

    let mut checkout = git2::build::CheckoutBuilder::new();
    // `force` is the point: without it a checkout refuses to overwrite a modified file, which is
    // exactly the file being discarded. The pathspec keeps it to this one path — a checkout of
    // the whole tree would discard everything else the author has open.
    checkout.force().path(path);
    repository.checkout_index(None, Some(&mut checkout))?;
    Ok(Discarded::Restored)
}

/// The absolute path of a project-relative one, or `None` for a bare repository, which has no
/// working tree for a path to be relative to.
fn workdir_path(repository: &Repository, path: &str) -> Option<PathBuf> {
    repository.workdir().map(|dir| dir.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A repository with one committed file, so there is a `HEAD` to compare against.
    fn repo() -> (tempfile::TempDir, Repository) {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "the manuscript\n").unwrap();
        commit_all(&repository, "first");
        (tmp, repository)
    }

    /// `super::status`, spelled so a local `let status = …` cannot shadow the function.
    fn changes(repository: &Repository) -> Status {
        super::status(repository).unwrap()
    }

    fn commit_all(repository: &Repository, message: &str) {
        let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();
        let mut index = repository.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let parents = repository.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repository.commit(Some("HEAD"), &who, &who, message, &tree, &parents).unwrap();
    }

    /// Every list, every letter, in one repository — the card's first done-when.
    #[test]
    fn each_kind_of_change_lands_in_the_right_list_with_the_right_letter() {
        let (tmp, repository) = repo();
        // `both.tex` has to be in HEAD to be "modified" later; everything else is created after
        // this commit, so that "new" and "untracked" really are.
        fs::write(tmp.path().join("both.tex"), "first version\n").unwrap();
        commit_all(&repository, "second");

        // Staged: a file Git has never seen, added to the index.
        fs::write(tmp.path().join("staged.tex"), "new and staged\n").unwrap();
        fs::write(tmp.path().join("untracked.tex"), "never seen\n").unwrap();
        stage(&repository, "staged.tex").unwrap();
        // Staged *and* then edited again: the case one list with a flag cannot express.
        fs::write(tmp.path().join("both.tex"), "staged version\n").unwrap();
        stage(&repository, "both.tex").unwrap();
        fs::write(tmp.path().join("both.tex"), "and edited again\n").unwrap();
        // Deleted, and staged as deleted.
        fs::remove_file(tmp.path().join("main.tex")).unwrap();
        stage(&repository, "main.tex").unwrap();

        let status = changes(&repository);
        let staged: Vec<_> = status.staged.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert!(staged.contains(&("main.tex", ChangeKind::Deleted)), "{staged:?}");
        assert!(staged.contains(&("staged.tex", ChangeKind::Added)), "{staged:?}");
        assert!(staged.contains(&("both.tex", ChangeKind::Modified)), "{staged:?}");

        let unstaged: Vec<_> = status.unstaged.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert!(unstaged.contains(&("untracked.tex", ChangeKind::Untracked)), "{unstaged:?}");
        assert!(unstaged.contains(&("both.tex", ChangeKind::Modified)), "{unstaged:?}");

        // `both.tex` really is in two lists at once, with a letter in each — the case one list
        // and a flag cannot express — and the badge still counts four paths, not five.
        assert_eq!(status.changed_file_count(), 4, "{status:?}");
    }

    /// The card's last done-when, and the reason this filter exists rather than a `.gitignore`
    /// line: a repository that predates this app has no such line.
    #[test]
    fn our_own_folder_is_never_a_change_even_with_no_gitignore() {
        let (tmp, repository) = repo();
        fs::create_dir_all(tmp.path().join(".abstract-tex/build")).unwrap();
        fs::write(tmp.path().join(".abstract-tex/build/main.pdf"), "junk").unwrap();
        fs::write(tmp.path().join(".abstract-tex/shell-escape.toml"), "junk").unwrap();
        assert!(!tmp.path().join(".gitignore").exists());

        let status = changes(&repository);
        assert_eq!(status, Status::default(), "our folder reached the author's Changes list");
    }

    /// Staging and unstaging move a path between lists and change nothing on disk.
    #[test]
    fn staging_and_unstaging_move_a_path_and_leave_the_file_alone() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "edited\n").unwrap();

        stage(&repository, "main.tex").unwrap();
        let status = changes(&repository);
        assert_eq!(status.staged.len(), 1);
        assert!(status.unstaged.is_empty(), "{status:?}");

        unstage(&repository, "main.tex").unwrap();
        let status = changes(&repository);
        assert!(status.staged.is_empty(), "{status:?}");
        assert_eq!(status.unstaged.len(), 1);

        // The edit itself survived both moves. Unstaging is not discarding.
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "edited\n");
    }

    /// Staging a *deleted* file has to go through `remove_path`; `add_path` would fail on a file
    /// that is not there, and the row would silently never move.
    #[test]
    fn staging_a_deleted_file_records_the_deletion() {
        let (tmp, repository) = repo();
        fs::remove_file(tmp.path().join("main.tex")).unwrap();

        stage(&repository, "main.tex").unwrap();

        let status = changes(&repository);
        assert_eq!(status.staged.iter().map(|c| c.kind).collect::<Vec<_>>(), vec![ChangeKind::Deleted]);
        assert!(status.unstaged.is_empty(), "{status:?}");
    }

    /// Discard is two operations wearing one word, and the caller is told which it got — that is
    /// what lets the confirmation the design notes require say the true thing.
    #[test]
    fn discarding_restores_a_tracked_file_and_deletes_an_untracked_one() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "a change to throw away\n").unwrap();
        fs::write(tmp.path().join("scratch.tex"), "never committed\n").unwrap();

        assert_eq!(discard(&repository, "main.tex").unwrap(), Discarded::Restored);
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "the manuscript\n");

        assert_eq!(discard(&repository, "scratch.tex").unwrap(), Discarded::Deleted);
        assert!(!tmp.path().join("scratch.tex").exists());

        assert_eq!(changes(&repository), Status::default());
    }

    /// Discarding one file must not discard the others the author has open.
    #[test]
    fn discarding_one_path_leaves_every_other_change_alone() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("other.tex"), "another file\n").unwrap();
        commit_all(&repository, "second");
        fs::write(tmp.path().join("main.tex"), "throw this away\n").unwrap();
        fs::write(tmp.path().join("other.tex"), "but keep this\n").unwrap();

        discard(&repository, "main.tex").unwrap();

        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "the manuscript\n");
        assert_eq!(fs::read_to_string(tmp.path().join("other.tex")).unwrap(), "but keep this\n");
    }

    /// A renamed chapter is one row, not a delete and an add — the design asks for `R`, and an
    /// author who sees their chapter as "deleted" stops trusting the panel.
    #[test]
    fn a_rename_is_one_row_that_remembers_where_it_came_from() {
        let (tmp, repository) = repo();
        fs::rename(tmp.path().join("main.tex"), tmp.path().join("thesis.tex")).unwrap();
        stage(&repository, "main.tex").unwrap();
        stage(&repository, "thesis.tex").unwrap();

        let status = changes(&repository);
        let renamed: Vec<_> = status.staged.iter().filter(|c| c.kind == ChangeKind::Renamed).collect();
        assert_eq!(renamed.len(), 1, "{status:?}");
        assert_eq!(renamed[0].path, "thesis.tex");
        assert_eq!(renamed[0].renamed_from.as_deref(), Some("main.tex"));
    }

    /// A folder with no repository anywhere above it is a sentence, not a crash — the Source
    /// Control view shows it before S10.5 offers to create one.
    #[test]
    fn a_folder_with_no_repository_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(open(tmp.path()), Err(GitError::NotARepository)));
    }
}
