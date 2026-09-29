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

    #[error("A commit needs a message.")]
    EmptyMessage,

    #[error("Nothing is staged, so there is nothing to commit.")]
    NothingStaged,
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

// ---------------------------------------------------------------------------------------------
// S10.2b: the history, and where this branch stands.
// ---------------------------------------------------------------------------------------------

/// One row of the Graph section (DESIGN.md §6: "a single-lane list of the first 200 commits with
/// lazy loading").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRow {
    pub id: String,
    /// What the row shows: the first seven characters, the length `git log --oneline` uses.
    pub short_id: String,
    /// The first line of the message, which is the only part a one-line row can show.
    pub summary: String,
    pub author: String,
    /// Seconds since the Unix epoch, UTC. The frontend formats it; a library that guessed at a
    /// locale here would be guessing for every caller.
    pub time: i64,
    /// The branch and remote names pointing at exactly this commit, for the tags the design asks
    /// for. Never our own snapshot ref — `tags_by_commit` says why that needs saying.
    pub tags: Vec<String>,
}

/// Where the current branch stands against the place it came from — everything
/// `Sync Changes ↑n ↓m` (DESIGN.md §5.7) is drawn from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchState {
    /// `None` on a detached `HEAD`, and on a repository with no commits yet.
    pub name: Option<String>,
    /// How far ahead of and behind its upstream this branch is.
    ///
    /// `None` means there is no upstream at all, which is *not* zero and not an error: "no
    /// remote yet" and "nothing to sync" are different sentences, and §5.7's one-verb button
    /// only belongs under one of them.
    pub ahead_behind: Option<(usize, usize)>,
    /// True before the first commit, when `HEAD` points at a branch that does not exist yet.
    /// The view says "no commits yet" rather than showing an empty graph with no explanation.
    pub unborn: bool,
}

/// A page of the history, walking back from `HEAD`.
///
/// Paged rather than whole, as the design settles: `skip` rows in, at most `limit` rows out. It
/// walks `HEAD` and so never sees S10.1's snapshot commits, which is the same reason `git log`
/// does not.
pub fn log(repository: &Repository, skip: usize, limit: usize) -> Result<Vec<CommitRow>, GitError> {
    if repository.head().is_err() {
        return Ok(Vec::new()); // no commits yet: an empty page, not an error
    }
    let tags = tags_by_commit(repository);

    let mut walk = repository.revwalk()?;
    walk.push_head()?;
    // Topological *and* by time: a single-lane list drawn in raw commit order puts a merged
    // branch's commits wherever their clocks happened to fall.
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    let mut rows = Vec::new();
    for id in walk.skip(skip).take(limit) {
        let commit = repository.find_commit(id?)?;
        let id = commit.id().to_string();
        rows.push(CommitRow {
            short_id: id.chars().take(7).collect(),
            summary: commit.summary().unwrap_or_default().to_string(),
            author: commit.author().name().unwrap_or_default().to_string(),
            time: commit.time().seconds(),
            tags: tags.iter().filter(|(commit_id, _)| *commit_id == id).map(|(_, name)| name.clone()).collect(),
            id,
        });
    }
    Ok(rows)
}

/// Every ref the graph may label a commit with, as `(commit id, short name)`.
///
/// Built by enumerating refs, which is why S10.1's ref needs excluding by name: `git log` never
/// shows a snapshot commit because it walks `HEAD`, but a commit that happens to *also* be a
/// snapshot would be tagged "abstract-tex/snapshots" here. A hidden ref that shows up as a tag
/// is not hidden.
fn tags_by_commit(repository: &Repository) -> Vec<(String, String)> {
    let Ok(references) = repository.references() else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for reference in references.flatten() {
        let Some(name) = reference.name() else { continue };
        if name == abstract_tex_snapshot::SNAPSHOT_REF {
            continue;
        }
        if !(reference.is_branch() || reference.is_remote() || reference.is_tag()) {
            continue;
        }
        if let Ok(commit) = reference.peel_to_commit() {
            let short = reference.shorthand().unwrap_or(name).to_string();
            tags.push((commit.id().to_string(), short));
        }
    }
    tags
}

/// Which branch this is, and how far it has drifted from its upstream.
pub fn branch_state(repository: &Repository) -> Result<BranchState, GitError> {
    let Ok(head) = repository.head() else {
        // `head()` fails before the first commit, where `HEAD` names a branch with no commit on
        // it. The branch's *name* is still there and still worth showing.
        let name = repository.find_reference("HEAD").ok().and_then(|head| head.symbolic_target().map(shorthand_of));
        return Ok(BranchState { name, ahead_behind: None, unborn: true });
    };

    let name = head.shorthand().map(str::to_string).filter(|_| head.is_branch());
    let ahead_behind = name.as_deref().and_then(|name| upstream_drift(repository, name, &head));
    Ok(BranchState { name, ahead_behind, unborn: false })
}

fn upstream_drift(repository: &Repository, name: &str, head: &git2::Reference<'_>) -> Option<(usize, usize)> {
    let branch = repository.find_branch(name, git2::BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    let local = head.target()?;
    let remote = upstream.get().target()?;
    repository.graph_ahead_behind(local, remote).ok()
}

fn shorthand_of(full: &str) -> String {
    full.strip_prefix("refs/heads/").unwrap_or(full).to_string()
}

/// Commit whatever is staged.
///
/// Two refusals, both because the panel makes the mistake easy and neither is fixed by pressing
/// the button again: an empty message, and a commit that would change nothing. The identity is
/// the repository's own `user.name`/`user.email`; when Git has none, this is
/// [`GitError::NoIdentity`] rather than a commit signed by a stand-in, because a history
/// attributed to "Abstract-Tex" is worse than one commit that did not happen yet.
pub fn commit(repository: &Repository, message: &str) -> Result<String, GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyMessage);
    }
    let who = repository.signature().map_err(|_| GitError::NoIdentity)?;

    let mut index = repository.index()?;
    let tree_id = index.write_tree()?;
    let tree = repository.find_tree(tree_id)?;

    let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
    if parent.as_ref().is_some_and(|commit| commit.tree_id() == tree_id) {
        return Err(GitError::NothingStaged);
    }
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();

    // `Some("HEAD")` here, unlike the snapshot crate's `None`: this *is* the author's own commit,
    // and it is supposed to move their branch.
    let id = repository.commit(Some("HEAD"), &who, &who, message, &tree, &parents)?;
    Ok(id.to_string())
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

    // --- S10.2b -----------------------------------------------------------------------------

    /// The card's first done-when: pages, tags, and no snapshot ref among them.
    #[test]
    fn the_log_pages_and_labels_rows_with_the_branch_that_points_at_them() {
        let (tmp, repository) = repo();
        for n in 2..=5 {
            fs::write(tmp.path().join("main.tex"), format!("version {n}\n")).unwrap();
            commit_all(&repository, &format!("commit {n}"));
        }

        let page = log(&repository, 0, 2).unwrap();
        assert_eq!(page.iter().map(|row| row.summary.as_str()).collect::<Vec<_>>(), ["commit 5", "commit 4"]);
        let next = log(&repository, 2, 2).unwrap();
        assert_eq!(next.iter().map(|row| row.summary.as_str()).collect::<Vec<_>>(), ["commit 3", "commit 2"]);
        // Paging must not overlap or skip: five commits, read two at a time, are five distinct ids.
        let all = log(&repository, 0, 100).unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].short_id.len(), 7);
        assert_eq!(all[0].author, "Ada");

        // The newest commit carries the branch that points at it, and nothing else does.
        let branch = branch_state(&repository).unwrap().name.unwrap();
        assert_eq!(all[0].tags, vec![branch]);
        assert!(all[1].tags.is_empty(), "{:?}", all[1]);
    }

    /// A hidden ref that shows up as a tag is not hidden. `log` walks `HEAD` so it never *lists*
    /// a snapshot commit, but the tags come from enumerating refs, which is the leak.
    #[test]
    fn a_snapshot_ref_never_becomes_a_tag_on_a_row() {
        let (tmp, repository) = repo();
        // S10.1 snapshots the working tree, which here is exactly what HEAD has — so the snapshot
        // ref ends up pointing at a commit whose tree the author's own commit also has, and a
        // naive tag walk would label the row.
        abstract_tex_snapshot::snapshot(tmp.path()).unwrap();
        // Point the snapshot ref straight at HEAD, the worst case this filter exists for.
        let head = repository.head().unwrap().peel_to_commit().unwrap();
        repository.reference(abstract_tex_snapshot::SNAPSHOT_REF, head.id(), true, "test").unwrap();

        let rows = log(&repository, 0, 10).unwrap();
        assert!(
            !rows.iter().any(|row| row.tags.iter().any(|tag| tag.contains("snapshot"))),
            "the snapshot ref leaked into the graph: {rows:?}"
        );
    }

    /// `None` is not zero. "No remote yet" and "nothing to sync" are different sentences, and
    /// §5.7's one-verb button only belongs under one of them.
    #[test]
    fn a_branch_with_no_upstream_reports_none_rather_than_zero() {
        let (_tmp, repository) = repo();
        let state = branch_state(&repository).unwrap();
        assert!(state.name.is_some());
        assert_eq!(state.ahead_behind, None);
        assert!(!state.unborn);
    }

    /// Before the first commit `HEAD` names a branch that does not exist. The view says "no
    /// commits yet" rather than drawing an empty graph with no explanation.
    #[test]
    fn a_repository_with_no_commits_yet_says_so_and_still_names_its_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();

        let state = branch_state(&repository).unwrap();
        assert!(state.unborn);
        assert!(state.name.is_some(), "the branch name exists before the commit does: {state:?}");
        assert!(log(&repository, 0, 10).unwrap().is_empty());
    }

    /// The card's second done-when: committing writes what the log then shows, and empties the
    /// staged list.
    #[test]
    fn committing_writes_the_history_the_log_then_shows_and_clears_the_staged_list() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("chapter.tex"), "a new chapter\n").unwrap();
        stage(&repository, "chapter.tex").unwrap();

        let id = commit(&repository, "  Add a chapter  ").unwrap();

        let rows = log(&repository, 0, 1).unwrap();
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].summary, "Add a chapter", "the message is trimmed");
        assert!(changes(&repository).staged.is_empty());
    }

    /// Both refusals, because a panel makes both mistakes easy and pressing the button again
    /// fixes neither.
    #[test]
    fn an_empty_message_and_an_empty_commit_are_both_refused_in_a_sentence() {
        let (tmp, repository) = repo();

        let blank = commit(&repository, "   ").unwrap_err();
        assert!(matches!(blank, GitError::EmptyMessage), "{blank:?}");

        let nothing = commit(&repository, "nothing changed").unwrap_err();
        assert!(matches!(nothing, GitError::NothingStaged), "{nothing:?}");
        // And the refusal really did refuse: no commit was written.
        assert_eq!(log(&repository, 0, 10).unwrap().len(), 1);

        // A file that is only edited, never staged, is still nothing to commit.
        fs::write(tmp.path().join("main.tex"), "edited but not staged\n").unwrap();
        assert!(matches!(commit(&repository, "still nothing"), Err(GitError::NothingStaged)));
    }

    /// A folder with no repository anywhere above it is a sentence, not a crash — the Source
    /// Control view shows it before S10.5 offers to create one.
    #[test]
    fn a_folder_with_no_repository_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(open(tmp.path()), Err(GitError::NotARepository)));
    }
}
