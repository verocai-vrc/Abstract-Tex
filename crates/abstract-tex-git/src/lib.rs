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

use git2::{Status as GitStatus, StatusOptions};

/// The handle [`open`] hands back, re-exported so that a caller can name it without depending on
/// `git2` itself. The app crate does exactly that (`src-tauri/src/git.rs`): every Git question it
/// asks goes through this crate, so libgit2 stays one crate's business and one crate's version.
pub use git2::Repository;

/// A commit id, re-exported for the same reason `Repository` is — `abstract-tex-latexdiff`
/// (S11.4b) names one in [`export_tree`]'s own signature and should not need its own `git2`
/// dependency just to spell the type its only Git dependency already hands it.
pub use git2::Oid;

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

    /// S11.1c: `amend` on a repository with no commits yet — an unborn branch has no `HEAD`
    /// commit to replace, which is a different sentence from an empty message.
    #[error("There is no commit yet to amend.")]
    NothingToAmend,

    /// S11.2a: `commit` while the index still has a conflicted path in it. Checked before
    /// `repository.state()` even, because a stray conflict outside a merge is still not
    /// something to build a tree out of and call finished.
    #[error("There are unresolved conflicts. Resolve them, then commit.")]
    UnresolvedConflicts,

    /// S10.5a: `git init` inside a folder that is already in a repository. Names what was found,
    /// because the answer depends on which repository it is — often one the author forgot they
    /// had, occasionally a monorepo where the paper is meant to live.
    #[error("This folder is already inside a Git repository ({0}).")]
    AlreadyARepository(String),

    /// S11.1a: `HEAD` is detached, or this is a repository with no commits yet. Neither has a
    /// branch to push, fetch or sync, and that is a different sentence from "no remote".
    #[error("There is no branch checked out to sync.")]
    NoBranch,

    /// S11.1a: this project has no `origin` yet. The panel should never ask — `branch_state`'s
    /// `ahead_behind` is `None` until a remote exists — but the crate still answers in a sentence
    /// rather than `GitError::Git`'s "remote 'origin' does not exist".
    #[error("This project has no remote named \"origin\" to sync with.")]
    NoRemote,

    /// S11.1a: libgit2 reports a rejected push through a callback, not through `push`'s own
    /// `Result` — this is that callback's message, turned into a refusal a caller can match on.
    #[error("The remote refused the push: {0}")]
    PushRejected(String),

    /// S11.3a: a blob among the commits this push would send is over GitHub's hard per-file
    /// limit. Caught here, before the network call, rather than left for GitHub to refuse —
    /// that refusal names no file and leaves the push half-sent. The size is already rounded up
    /// to a whole MB at the call site, so "100 MB" in the sentence always means "really over."
    /// S11.5a: `clone` into a folder that already has something in it. libgit2 would refuse too,
    /// but with a message about its own bookkeeping; the author needs to hear which folder, and
    /// that nothing was touched.
    #[error("{0} already has files in it, so nothing was downloaded there. Pick an empty folder, or a new name.")]
    DestinationNotEmpty(String),

    /// S11.7: a side-by-side view of a file that is not text. The view is offered for `.tex` and
    /// `.bib` only, so this is a file that claims to be one and is not valid UTF-8 — a lossy
    /// decode would show text the file does not contain.
    #[error("{0} is not a text file, so there is no side-by-side view of it.")]
    NotText(String),

    /// S11.7: a path that leaves the repository. The frontend only ever passes the paths a row
    /// gave it; one that is absolute or climbs out is a bug, refused rather than read.
    #[error("{0} is not a path inside this project.")]
    OutsideProject(String),

    #[error("{0} is over {1} MB — GitHub refuses any file over 100 MB. Push refused; remove it from history or track it with Git LFS first.")]
    FileTooLarge(String, u64),
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

/// S11.3b: one candidate for the Source Control view's "track with Git LFS" banner.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub size_bytes: u64,
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

/// S11.3b: every changed path over `threshold_bytes` that is not already routed through Git LFS.
///
/// Reads `status` rather than walking the working tree itself, so this agrees with the three
/// lists the view already draws and never flags `.abstract-tex/` or a path mid-rename twice.
/// Deletions are skipped — there is no file left to track. `get_attr` asks the question the
/// banner actually needs answered, "would a commit of this path go through the `lfs` filter right
/// now," by resolving `.gitattributes` the same way Git itself would, patterns and precedence
/// included, rather than this function re-reading and matching the patterns by hand.
pub fn large_files(repository: &Repository, threshold_bytes: u64) -> Result<Vec<LargeFile>, GitError> {
    let status = status(repository)?;
    let Some(workdir) = repository.workdir() else {
        return Ok(Vec::new()); // a bare repository has no working-tree file to size up
    };

    let mut seen = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    for change in status.staged.iter().chain(&status.unstaged).chain(&status.conflicted) {
        if change.kind == ChangeKind::Deleted || !seen.insert(change.path.clone()) {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(workdir.join(&change.path)) else { continue };
        if metadata.len() <= threshold_bytes {
            continue;
        }
        let already_tracked = matches!(
            repository.get_attr(Path::new(&change.path), "filter", git2::AttrCheckFlags::default()),
            Ok(Some("lfs"))
        );
        if !already_tracked {
            found.push(LargeFile { path: change.path.clone(), size_bytes: metadata.len() });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
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
    /// Words of prose this commit added, against its first parent — negative when it cut more
    /// than it wrote (S10.3c). Counted over `.tex` files only, by `texwords`, which is what
    /// makes the graph a progress log rather than a list of diffs (DESIGN.md §6).
    pub word_delta: i64,
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
    /// The commit `HEAD` points at, or `None` on an unborn branch.
    ///
    /// Here so that a caller can tell whether the *history* moved, which is not the same question
    /// as whether a file changed (S10.3c). A page of the graph carries a word count per row and
    /// costs real work to build — measured in `tests/against_real_git.rs` — while `branch_state`
    /// is a ref lookup. Comparing this against the id a page was built from is what lets the
    /// panel re-read the graph on a commit and not on every save.
    pub head: Option<String>,
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
            word_delta: word_delta(repository, &commit),
            id,
        });
    }
    Ok(rows)
}

/// Write one commit's entire tree to `dest_dir` as real files — S11.4a, for a `latexdiff` review
/// between two revisions, which needs each one as a document on disk, not as a diff of blobs.
///
/// `Tree::walk` rather than hand-written recursion: libgit2 already knows how to descend a tree,
/// and the callback sees every entry, files and folders alike, as `(root, entry)` with `root`
/// already carrying the trailing `/` — `format!("{root}{name}")` is a full relative path with no
/// joining logic of this function's own to get wrong. Only blobs are written; a directory entry
/// needs no file of its own, and `create_dir_all` on a blob's own parent is enough to make every
/// folder along the way.
///
/// The callback can only report back to libgit2 itself — a `TreeWalkResult`, not a `Result` of
/// ours — so the first real error is captured here and read back out once `walk` returns, the
/// same `RefCell`-into-a-shared-closure shape `push`'s own `rejected` uses, and for the same
/// reason: an `FnMut` closure already borrowing `dest_dir` and `repository` cannot also take
/// `&mut` of a plain local without the borrow checker correctly refusing it.
pub fn export_tree(repository: &Repository, commit: git2::Oid, dest_dir: &Path) -> Result<(), GitError> {
    let tree = repository.find_commit(commit)?.tree()?;
    let failure: std::cell::RefCell<Option<std::io::Error>> = std::cell::RefCell::new(None);

    let walked = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return git2::TreeWalkResult::Ok;
        }
        let Some(name) = entry.name() else { return git2::TreeWalkResult::Ok };
        let full_path = dest_dir.join(format!("{root}{name}"));
        let write = entry
            .to_object(repository)
            .ok()
            .and_then(|object| object.into_blob().ok())
            .ok_or_else(|| std::io::Error::other("not a readable blob"))
            .and_then(|blob| {
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&full_path, blob.content())
            });
        match write {
            Ok(()) => git2::TreeWalkResult::Ok,
            Err(error) => {
                *failure.borrow_mut() = Some(error);
                git2::TreeWalkResult::Abort
            }
        }
    });

    if let Some(error) = failure.into_inner() {
        return Err(GitError::Io(error));
    }
    walked?;
    Ok(())
}

/// Whether `path` — `/`-separated, relative to the repository's top — names a file in `commit`'s
/// tree. S11.4c asks this of both revisions before exporting either, so a comparison whose root
/// file did not exist yet is refused for the price of two tree lookups, not two whole exports.
pub fn has_path(repository: &Repository, commit: git2::Oid, path: &str) -> Result<bool, GitError> {
    let tree = repository.find_commit(commit)?.tree()?;
    // `get_path` errors for "not there", and also for a real failure to read the tree; only the
    // first is an answer. `NotFound` is how libgit2 spells it.
    match tree.get_path(Path::new(path)) {
        Ok(entry) => Ok(entry.kind() == Some(git2::ObjectType::Blob)),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// `a` and `b` as `(older, newer)` — S11.4c's "always older → newer, whatever order they were
/// clicked in" (DESIGN.md §5.7).
///
/// Ancestry first, time only as a fallback: a commit made within the same second as its parent
/// has the same timestamp, so time alone cannot order the very pair a single-lane graph shows
/// most often. Two commits neither of which descends from the other (a merge's two sides) have no
/// ancestry to go by, and there the committer's clock is the only answer there is.
pub fn older_first(repository: &Repository, a: git2::Oid, b: git2::Oid) -> Result<(git2::Oid, git2::Oid), GitError> {
    if repository.graph_descendant_of(a, b)? {
        return Ok((b, a));
    }
    if repository.graph_descendant_of(b, a)? {
        return Ok((a, b));
    }
    let a_time = repository.find_commit(a)?.time().seconds();
    let b_time = repository.find_commit(b)?.time().seconds();
    Ok(if b_time < a_time { (b, a) } else { (a, b) })
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
        return Ok(BranchState { name, ahead_behind: None, unborn: true, head: None });
    };

    let name = head.shorthand().map(str::to_string).filter(|_| head.is_branch());
    let ahead_behind = name.as_deref().and_then(|name| upstream_drift(repository, name, &head));
    let head_id = head.target().map(|id| id.to_string());
    Ok(BranchState { name, ahead_behind, unborn: false, head: head_id })
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
/// Three refusals, all because the panel makes the mistake easy and none of them is fixed by
/// pressing the button again: an empty message, a commit that would change nothing, and —
/// S11.2a — anything still conflicted. The identity is the repository's own
/// `user.name`/`user.email`; when Git has none, this is [`GitError::NoIdentity`] rather than a
/// commit signed by a stand-in, because a history attributed to "Abstract-Tex" is worse than one
/// commit that did not happen yet.
///
/// **This is also how a merge finishes (S11.2a).** `sync`'s own clean merges call this function
/// rather than duplicating it, and so does an author who has resolved every conflict and staged
/// each file — the same button, with no "finish merge" of its own, exactly as plain `git commit`
/// needs no flag to end a merge either.
pub fn commit(repository: &Repository, message: &str) -> Result<String, GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyMessage);
    }
    let mut index = repository.index()?;
    if index.has_conflicts() {
        return Err(GitError::UnresolvedConflicts);
    }
    let who = repository.signature().map_err(|_| GitError::NoIdentity)?;
    let tree_id = index.write_tree()?;
    let tree = repository.find_tree(tree_id)?;

    let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
    let merging = repository.state() == git2::RepositoryState::Merge;
    if !merging && parent.as_ref().is_some_and(|commit| commit.tree_id() == tree_id) {
        return Err(GitError::NothingStaged);
    }
    let mut parents: Vec<git2::Commit<'_>> = parent.into_iter().collect();
    if merging {
        // `MERGE_HEAD` can name more than one commit (an octopus merge), though `sync` only ever
        // starts one; read whatever is actually there rather than assuming. A plain file read,
        // not `git2`'s own `mergehead_foreach`, because that needs `&mut Repository` and every
        // verb in this crate is handed a shared `&Repository` (`git.rs`'s own rule, so that no
        // command ever has to decide who else might be touching the repository at the same time).
        let contents = std::fs::read_to_string(repository.path().join("MERGE_HEAD")).unwrap_or_default();
        for line in contents.lines() {
            let id = git2::Oid::from_str(line.trim())?;
            parents.push(repository.find_commit(id)?);
        }
    }
    let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

    // `Some("HEAD")` here, unlike the snapshot crate's `None`: this *is* the author's own commit,
    // and it is supposed to move their branch.
    let id = repository.commit(Some("HEAD"), &who, &who, message, &tree, &parent_refs)?;
    if merging {
        repository.cleanup_state()?;
    }
    Ok(id.to_string())
}

/// Replace `HEAD` with a new commit carrying `message` and whatever is staged, keeping the same
/// parents — the Commit dropdown's *Amend* (S11.1c).
///
/// Only one refusal, unlike `commit`: an empty message. Amending with nothing staged is not
/// nothing — it is a reword, and a reword is the whole reason this exists — so there is no
/// `NothingStaged` check here. Whether `HEAD` has already been pushed is not this function's
/// business either: the panel decides that by reading `branch_state`'s `ahead_behind` before it
/// ever offers the item, the same guardrail the Source Control design notes ask for.
pub fn amend(repository: &Repository, message: &str) -> Result<String, GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyMessage);
    }
    let head_commit = repository.head().map_err(|_| GitError::NothingToAmend)?.peel_to_commit()?;
    let who = repository.signature().map_err(|_| GitError::NoIdentity)?;

    let mut index = repository.index()?;
    let tree = repository.find_tree(index.write_tree()?)?;

    // `amend` keeps the original commit's parents untouched — passing a tree and a message but no
    // parent list is what makes this a reword-or-reshape of the same commit rather than a new one.
    let id = head_commit.amend(Some("HEAD"), Some(&who), Some(&who), None, Some(message), Some(&tree))?;
    Ok(id.to_string())
}

// ---------------------------------------------------------------------------------------------
// S10.3c: what a writer counts.
//
// DESIGN.md §6: "the commit-message box is pre-filled with a summary built from the outline and
// the diff — *\"Revised §3.2 Methods, +240 words\"* — with no model involved", and "each graph row
// carries its word-count delta, so the graph doubles as a progress log". Both numbers come from
// `texwords`, because a `.tex` line diff counts markup and a writer counts prose: wrapping an
// equation in `\begin{align}` is four lines and no words, and rewording a paragraph in place is
// one line and twenty.
//
// Only `.tex` files are counted. A `.bib` entry, a figure and a `Makefile` are all real work and
// none of them is prose the author wrote in sentences.
// ---------------------------------------------------------------------------------------------

/// Everything a suggested commit message is built from.
///
/// Numbers and names only: the *sentence* is the frontend's, the same split `CommitRow::time`
/// already makes. "Revised Methods, +240 words" is a phrasing decision, and phrasing belongs
/// where the person reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProseSummary {
    /// Words of prose in the `.tex` files involved, as `HEAD` has them.
    pub words_before: usize,
    /// Words of prose in the same files, as the working tree has them.
    pub words_after: usize,
    /// The titles of the sections a changed line falls in, in the order met, without repeats.
    /// Empty when nothing could be attributed — a new file, or a change above the first heading.
    pub sections: Vec<String>,
    /// The `.tex` files involved, project-relative.
    pub paths: Vec<String>,
    /// Those of them Git has not got at all yet, so the message can say *Added* rather than
    /// *Revised*.
    pub added_paths: Vec<String>,
}

/// What has changed since the last commit, in a writer's units.
///
/// The comparison is `HEAD` against the working tree *with the index folded in*, which is the
/// same span the Changes and Staged Changes lists cover between them: the suggested message has
/// to describe what a commit would contain, and an author who has staged half their work is
/// still writing about all of it.
pub fn prose_summary(repository: &Repository) -> Result<ProseSummary, GitError> {
    let head_tree = repository.head().ok().and_then(|head| head.peel_to_tree().ok());

    let mut options = git2::DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        // A new file's own content, so its sections can be attributed like any other file's.
        .show_untracked_content(true)
        // No context: a hunk's line range should name the lines that changed and not the three
        // either side of them, or a one-line edit at a section boundary lands in both sections.
        .context_lines(0);
    let diff = repository.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))?;

    let mut summary = ProseSummary::default();
    for (index, delta) in diff.deltas().enumerate() {
        let Some(path) = delta.new_file().path().map(path_string).or_else(|| delta.old_file().path().map(path_string))
        else {
            continue;
        };
        if is_ours(&path) || !is_tex(&path) {
            continue;
        }

        let before = blob_text(repository, delta.old_file().id());
        // The working tree, not the index: rule 1 — the file on disk is the truth. A deleted file
        // reads as empty, which is exactly the count it should contribute.
        let after = workdir_text(repository, &path);
        summary.words_before += texwords::count_prose(&before);
        summary.words_after += texwords::count_prose(&after);
        summary.paths.push(path.clone());
        if matches!(delta.status(), git2::Delta::Added | git2::Delta::Untracked) {
            summary.added_paths.push(path.clone());
        }

        // Which sections those words landed in. `Patch::from_diff` is per delta, so this is the
        // only place the hunks of one file can be walked without a callback that would have to
        // borrow `summary` twice.
        let headings = texwords::headings(&after);
        if headings.is_empty() {
            continue;
        }
        if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, index) {
            for hunk_index in 0..patch.num_hunks() {
                let Ok((hunk, _)) = patch.hunk(hunk_index) else { continue };
                if let Some(section) = texwords::section_of(&headings, hunk.new_start() as usize) {
                    if !summary.sections.contains(&section.title) {
                        summary.sections.push(section.title.clone());
                    }
                }
            }
        }
    }
    Ok(summary)
}

/// How many words of prose one commit added, against its first parent.
///
/// First parent, because a merge's second parent is someone else's writing and counting it as
/// this commit's progress would flatter the log. A root commit counts everything in it.
///
/// This runs once per row of [`log`], which is up to a page of history at a time. It is a tree
/// diff and a prose scan of each changed `.tex` file — microseconds each on a manuscript, and
/// measured on a 300-commit repository in `tests/against_real_git.rs` before it was left in the
/// hot path of the panel's refresh.
fn word_delta(repository: &Repository, commit: &git2::Commit<'_>) -> i64 {
    let new_tree = match commit.tree() {
        Ok(tree) => tree,
        Err(_) => return 0,
    };
    let old_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());

    let mut options = git2::DiffOptions::new();
    let Ok(diff) = repository.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut options)) else {
        return 0;
    };

    let mut delta: i64 = 0;
    for change in diff.deltas() {
        let path = change.new_file().path().map(path_string).or_else(|| change.old_file().path().map(path_string));
        let Some(path) = path else { continue };
        if is_ours(&path) || !is_tex(&path) {
            continue;
        }
        let before = texwords::count_prose(&blob_text(repository, change.old_file().id()));
        let after = texwords::count_prose(&blob_text(repository, change.new_file().id()));
        delta += after as i64 - before as i64;
    }
    delta
}

/// A blob's text, or `""` when there is none — a file being added has no old blob, and a binary
/// blob has no text worth counting. Lossy, like `read_file` in the app: a `.tex` file written in
/// Latin-1 should give a slightly odd count rather than none at all.
fn blob_text(repository: &Repository, id: git2::Oid) -> String {
    if id.is_zero() {
        return String::new();
    }
    match repository.find_blob(id) {
        Ok(blob) => String::from_utf8_lossy(blob.content()).into_owned(),
        Err(_) => String::new(),
    }
}

/// A file's text as it is on disk right now, or `""` if it is not there.
fn workdir_text(repository: &Repository, path: &str) -> String {
    let Some(dir) = repository.workdir() else { return String::new() };
    std::fs::read(dir.join(path)).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
}

fn is_tex(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(|extension| extension.eq_ignore_ascii_case("tex"))
}

// ---------------------------------------------------------------------------------------------
// S10.5a: making a folder a repository.
// ---------------------------------------------------------------------------------------------

/// What `.gitignore` gets, and why each half of it is there.
///
/// `.abstract-tex/` is ours by DESIGN.md §5.8, and [`NEVER_A_CHANGE`] already keeps it out of the
/// panel — this is the same promise written where `git` itself can read it, so a coauthor who
/// clones the project does not get our build folder in their first `git status` either.
///
/// The rest is what a hand-run `pdflatex` leaves behind. Builds from *this* app never write those
/// into the source tree (S9.12 moved shell-escape builds into the build folder for exactly that
/// reason), but an author who runs the engine in a terminal will, and a *Changes* list full of
/// `.aux` files is what DESIGN.md §6 means by shouting when nothing is wrong.
const GITIGNORE_LINES: &str = "\
# Abstract-Tex's own folder: builds, drafts and snapshots (DESIGN.md §5.8)
.abstract-tex/

# What a TeX run leaves next to the manuscript when it is not run by Abstract-Tex
*.aux
*.bbl
*.bcf
*.blg
*.fdb_latexmk
*.fls
*.lof
*.log
*.lot
*.nav
*.out
*.run.xml
*.snm
*.synctex.gz
*.toc
*.vrb
";

/// The line every check for "is our folder ignored" looks for.
const OUR_FOLDER_LINE: &str = ".abstract-tex/";

/// What [`initialise`] did, so the panel can say the true thing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Initialised {
    /// The first commit's id, or `None` when Git does not know who the author is yet.
    pub first_commit: Option<String>,
    /// Set when there is no commit: the sentence naming what to do about it. The repository and
    /// the `.gitignore` exist regardless, and everything is staged, so the author's own first
    /// commit is one `git config` away.
    pub needs_identity: bool,
}

/// Turn a folder into a Git repository: `init`, a `.gitignore`, stage everything, commit.
///
/// §5.7 asks for one action that ends in an initial commit, and S10.2b's rule is that this app
/// never invents an identity. Both hold here: the repository and its `.gitignore` are made either
/// way, and the commit happens only when `user.name` is set. A repository with a staged tree and
/// no commit is a real, recoverable state; a commit signed by a stand-in is not.
///
/// Refuses a folder that is *already* inside a repository, naming what it found: `git init` in a
/// subfolder of a repository is almost never what anyone meant, and it is unpleasant to undo.
pub fn initialise(project_dir: &Path) -> Result<Initialised, GitError> {
    if let Ok(existing) = Repository::discover(project_dir) {
        let found = existing.workdir().map(|dir| dir.display().to_string()).unwrap_or_else(|| "a Git repository".into());
        return Err(GitError::AlreadyARepository(found));
    }

    let repository = Repository::init(project_dir)?;
    write_gitignore(project_dir)?;
    stage_and_commit_everything(&repository)
}

/// Stage the whole folder and make the first commit — the half of [`initialise`] that can refuse.
///
/// Split out so that the "Git does not know who you are" path can be tested on a machine that
/// *does* know: the test initialises a repository, empties its own local `user.name`, and calls
/// this. Taking the machine's global identity away for the length of a test would mean mutating
/// process-wide state that every other test in the same process reads.
fn stage_and_commit_everything(repository: &Repository) -> Result<Initialised, GitError> {
    // `add_all` respects the `.gitignore` written just before it, which is the whole reason the
    // order is that way round: staging and *then* ignoring would leave the build folder in the
    // first commit for ever, where `git rm --cached` is the only way out.
    let mut index = repository.index()?;
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;

    match commit(repository, "Initial commit") {
        Ok(id) => Ok(Initialised { first_commit: Some(id), needs_identity: false }),
        // The one refusal that is not a failure here: everything else about this worked, and the
        // staged tree is waiting for the author's own first commit.
        Err(GitError::NoIdentity) => Ok(Initialised { first_commit: None, needs_identity: true }),
        // An empty folder has nothing to commit, and that is fine too — a repository with no
        // commits is where S10.2b's `unborn` branch state comes from.
        Err(GitError::NothingStaged) => Ok(Initialised { first_commit: None, needs_identity: false }),
        Err(error) => Err(error),
    }
}

/// Whether this repository already tells Git to ignore our folder.
///
/// `is_path_ignored` and not a read of `.gitignore`: the rule may be in a global ignore file, in
/// `.git/info/exclude`, or in a `.gitignore` three folders up, and all of those are answers.
pub fn our_folder_is_ignored(repository: &Repository) -> bool {
    repository.is_path_ignored(".abstract-tex/build").unwrap_or(false)
}

/// Add our lines to this repository's `.gitignore`, keeping whatever is already in it.
///
/// Only ever called after the author said yes (S10.2a's outcome note, point 3): a repository that
/// existed before this app has no line for `.abstract-tex/`, and editing someone's `.gitignore`
/// unasked is the kind of co-author §5.7 is careful not to be.
pub fn ignore_our_folder(repository: &Repository) -> Result<(), GitError> {
    let Some(workdir) = repository.workdir() else {
        return Err(GitError::NotARepository); // a bare repository has no `.gitignore` to write
    };
    write_gitignore(workdir)
}

/// Write the lines, or append them to an existing file.
///
/// Appending rather than replacing, because the file may be the author's — and checking for
/// [`OUR_FOLDER_LINE`] rather than for the whole block, because the block may already have been
/// added by hand, by a coauthor, or by a previous version of this function.
fn write_gitignore(dir: &Path) -> Result<(), GitError> {
    let path = dir.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == OUR_FOLDER_LINE) {
        return Ok(());
    }
    let mut contents = existing;
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    if !contents.is_empty() {
        contents.push('\n');
    }
    contents.push_str(GITIGNORE_LINES);
    std::fs::write(&path, contents)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// S10.5b: the remote.
// ---------------------------------------------------------------------------------------------

/// The name Git gives the remote it was cloned from, and the one this app sets.
pub const ORIGIN: &str = "origin";

/// The URL of `origin`, or `None` when this repository has no remote yet.
///
/// What the panel needs to know before it offers to create one: a project that already has a
/// remote is not a project to publish, whatever else might be true about it.
pub fn origin_url(repository: &Repository) -> Option<String> {
    repository.find_remote(ORIGIN).ok().and_then(|remote| remote.url().map(str::to_string))
}

/// Point this repository at `url` as `origin`, replacing whatever was there.
///
/// `remote_set_url` when one exists rather than delete-and-add, because deleting a remote also
/// deletes its remote-tracking branches (`refs/remotes/origin/*`) and its fetch refspec — and an
/// author who had a remote and now has a different one should not silently lose what Git knew
/// about the first.
///
/// Nothing is sent anywhere by this: it only ever writes a line to `.git/config`. `push` and
/// `sync`, below, are what actually talk to the network this points at.
pub fn set_origin(repository: &Repository, url: &str) -> Result<(), GitError> {
    if repository.find_remote(ORIGIN).is_ok() {
        repository.remote_set_url(ORIGIN, url)?;
    } else {
        repository.remote(ORIGIN, url)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// S11.1a: push, fetch, and the sync decision.
// ---------------------------------------------------------------------------------------------

/// What [`sync`] did, so the panel can say the true thing rather than a generic "synced".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SyncOutcome {
    /// Nothing to push and nothing to pull: the branch and its remote already agree.
    UpToDate,
    /// Local commits went to the remote. No commits came back, so nothing moved here.
    Pushed { ahead: usize },
    /// Remote commits were fast-forwarded in. Nothing local was waiting to go out.
    FastForwarded { behind: usize },
    /// S11.2a: both sides had moved, libgit2's merge resolved every file on its own, and the
    /// result was committed and pushed in the same breath.
    Merged,
    /// S11.2a: both sides had moved and at least one file could not be merged automatically.
    /// Real conflict markers are in the working tree and a real `MERGE_HEAD` is set, the same
    /// state a terminal `git merge` would leave — `status`'s own `conflicted` list already names
    /// which files, so this carries nothing further.
    Conflicted,
}

/// The branch `HEAD` is on, or [`GitError::NoBranch`] for a detached `HEAD` or a repository with
/// no commits yet — neither has anything to push, fetch, or sync.
fn current_branch(repository: &Repository) -> Result<String, GitError> {
    let head = repository.head().map_err(|_| GitError::NoBranch)?;
    head.shorthand().filter(|_| head.is_branch()).map(str::to_string).ok_or(GitError::NoBranch)
}

/// `origin`, or [`GitError::NoRemote`] rather than `git2`'s "remote 'origin' does not exist" —
/// one more sentence the panel never has to translate.
fn origin(repository: &Repository) -> Result<git2::Remote<'_>, GitError> {
    repository.find_remote(ORIGIN).map_err(|_| GitError::NoRemote)
}

/// Offer `token` to a remote only when it actually asks for one. A `file://` remote, which is
/// every test in this module, never does — so every one of them runs with `token: None` and
/// still proves the credential path is never touched by accident.
fn credentials(token: Option<&str>) -> git2::RemoteCallbacks<'_> {
    let token = token.map(str::to_string);
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(move |url, _username_from_url, allowed| {
        if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) && is_github_https(url) {
            if let Some(token) = &token {
                // GitHub's own convention for an OAuth token offered over HTTPS: the token is the
                // username or the password, not both at once as a real login would be. Nothing
                // here is GitHub-specific otherwise, but this is the one place a comment has to
                // say which convention it followed.
                return git2::Cred::userpass_plaintext(token, "x-oauth-basic");
            }
        }
        Err(git2::Error::from_str("this remote asked for a credential and none was given"))
    });
    callbacks
}

/// Whether `url` is an HTTPS address on github.com — the only place the GitHub token belongs.
///
/// The token is an OAuth token for *GitHub*. A remote on GitLab, a university server, or a
/// lookalike host that a pasted URL named (S11.5b lets the author clone from any URL) would be
/// handed it the moment it asked for a password, so the callback checks where it is going rather
/// than trusting that the author's remote is GitHub's. Exact host only: `github.com.evil.example`
/// and `evilgithub.com` are different hosts, and user-info tricks (`https://github.com@evil/…`)
/// are caught because the host is read after the `@`.
fn is_github_https(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    host.eq_ignore_ascii_case("github.com")
}

/// Update `origin`'s remote-tracking ref for the current branch, without touching anything else.
///
/// Only the current branch, not every branch `origin` has: this crate is one repository's one
/// working branch, not a general mirroring tool, and fetching branches nobody asked about would
/// cost time on every sync for refs the view never shows.
pub fn fetch(repository: &Repository, token: Option<&str>) -> Result<(), GitError> {
    let branch_name = current_branch(repository)?;
    let mut remote = origin(repository)?;
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(credentials(token));
    remote.fetch(&[&branch_name], Some(&mut options), None)?;
    Ok(())
}

/// GitHub rejects any pushed file over this many bytes outright (100 MiB, its documented hard
/// limit — the 50 MiB figure elsewhere on that page is only a warning). `oversized_blobs` checks
/// against this before `push` sends anything, so the refusal below names the file instead of
/// whatever opaque message the server's own rejection would have given.
const GITHUB_FILE_LIMIT_BYTES: u64 = 100 * 1024 * 1024;

/// Push the current branch to `origin`.
///
/// Sets the upstream tracking relationship on a first push (`git push -u`'s own behaviour), and
/// updates `refs/remotes/origin/<branch>` to match — so `branch_state`'s ahead/behind reads right
/// immediately afterwards, with no second fetch needed before the Sync button's arrows catch up.
pub fn push(repository: &Repository, token: Option<&str>) -> Result<(), GitError> {
    let branch_name = current_branch(repository)?;
    let target = repository.head()?.target().ok_or(GitError::NoBranch)?;

    // S11.3a: checked against exactly the commits this push is about to send — `already_on_remote`
    // is `None` on a branch's first push, the same "whole history is ahead" case `sync` already
    // walks in `commits_since`.
    let remote_ref = format!("refs/remotes/{ORIGIN}/{branch_name}");
    let already_on_remote = repository.find_reference(&remote_ref).ok().and_then(|r| r.target());
    let oversized = oversized_blobs(repository, already_on_remote, target, GITHUB_FILE_LIMIT_BYTES)?;
    if let Some((path, size)) = oversized.into_iter().max_by_key(|(_, size)| *size) {
        let megabytes = size.div_ceil(1024 * 1024);
        return Err(GitError::FileTooLarge(path, megabytes));
    }

    let mut remote = origin(repository)?;

    // A stale local branch is caught by libgit2 itself, before anything is sent, and arrives
    // through the `?` below as an ordinary `GitError::Git`. This callback is for the other kind
    // of refusal: a server-side policy (a protected branch, a pre-receive hook) that `push`'s own
    // `Result` says nothing about — it comes back `Ok` while the remote quietly refused the ref,
    // and only this per-ref status says so. Checked by hand against a real local remote, because
    // the two refusals read alike in the docs and do not behave alike.
    // `RefCell`: the callback below is an `Fn` closure (libgit2 may call it more than once) that
    // needs to write into this from inside a shared, non-`mut` borrow — ordinary field mutation
    // cannot do that, so the check that would normally happen at compile time happens at
    // `borrow_mut()` instead. Safe here because nothing else touches `rejected` while it runs.
    let rejected = std::cell::RefCell::new(None);
    let mut callbacks = credentials(token);
    callbacks.push_update_reference(|_refname, status| {
        if let Some(message) = status {
            *rejected.borrow_mut() = Some(message.to_string());
        }
        Ok(())
    });
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(callbacks);
    let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");
    remote.push(&[&refspec], Some(&mut options))?;
    // `options` holds the closure borrowing `rejected`; it has to go before `rejected` can be
    // read back out, or the borrow checker sees a destructor that might still touch it.
    drop(options);
    if let Some(message) = rejected.into_inner() {
        return Err(GitError::PushRejected(message));
    }

    repository.reference(
        &format!("refs/remotes/{ORIGIN}/{branch_name}"),
        target,
        true,
        "abstract-tex: push",
    )?;

    let mut branch = repository.find_branch(&branch_name, git2::BranchType::Local)?;
    if branch.upstream().is_err() {
        branch.set_upstream(Some(&format!("{ORIGIN}/{branch_name}")))?;
    }
    Ok(())
}

/// Move the current branch, and the working tree, straight to `target` — safe only because the
/// branch has no commit of its own that this would throw away.
///
/// `checkout_tree`'s default strategy refuses rather than overwrites a file with an uncommitted
/// change in it, the same refusal `git pull --ff-only` gives — this never forces past it.
fn fast_forward(repository: &Repository, branch_name: &str, target: git2::Oid) -> Result<(), GitError> {
    let commit = repository.find_commit(target)?;
    repository.checkout_tree(commit.as_object(), None)?;
    repository.reference(&format!("refs/heads/{branch_name}"), target, true, "abstract-tex: sync")?;
    Ok(())
}

/// The one-verb path (DESIGN.md §5.7): fetch, then do whatever that leaves to do.
///
/// Composed of two other public functions rather than its own push, because the design's Commit
/// dropdown asks for both on their own: `push` alone is *Commit & Push* (an author who knows where
/// this is going); this function is *Commit & Sync* and the standalone *Sync Changes ↑n ↓m*
/// button (an author who does not want to think about it). A branch and its remote that have each
/// moved since they last agreed is merged with libgit2's own three-way merge (S11.2a) rather than
/// refused — cleanly if it can be, left as a real conflict for the author if it cannot.
pub fn sync(repository: &Repository, token: Option<&str>) -> Result<SyncOutcome, GitError> {
    let branch_name = current_branch(repository)?;
    let local = repository.head()?.target().ok_or(GitError::NoBranch)?;
    fetch(repository, token)?;

    let remote_ref = format!("refs/remotes/{ORIGIN}/{branch_name}");
    let Some(remote_target) = repository.find_reference(&remote_ref).ok().and_then(|r| r.target()) else {
        // `origin` exists, but has never seen this branch: everything local is "ahead", counted
        // by walking it, the same way `log`'s own paging does.
        let ahead = commits_since(repository, None, local)?;
        push(repository, token)?;
        return Ok(SyncOutcome::Pushed { ahead });
    };

    let (ahead, behind) = repository.graph_ahead_behind(local, remote_target)?;
    match (ahead, behind) {
        (0, 0) => Ok(SyncOutcome::UpToDate),
        (ahead, 0) => {
            push(repository, token)?;
            Ok(SyncOutcome::Pushed { ahead })
        }
        (0, behind) => {
            fast_forward(repository, &branch_name, remote_target)?;
            Ok(SyncOutcome::FastForwarded { behind })
        }
        (_, _) => merge(repository, remote_target, token),
    }
}

/// The diverged case: a real three-way merge, libgit2's own — the same engine `git merge` itself
/// calls, not a hand-rolled one.
///
/// `Repository::merge` always checks the result out to the working tree, whether it resolved
/// cleanly or not — there is no in-between step where this function could decide to hide a
/// conflict marker from disk even if it wanted to. A clean result is finished by `commit`, the
/// very function the Commit button already calls, and pushed back in the same breath; a
/// conflicted one is left exactly as a terminal `git merge` would leave it, because a future
/// conflict view reading anything other than what `git status` already agrees on would be two
/// kinds of Git disagreeing with each other.
fn merge(repository: &Repository, their_target: git2::Oid, token: Option<&str>) -> Result<SyncOutcome, GitError> {
    let their_commit = repository.find_annotated_commit(their_target)?;
    repository.merge(&[&their_commit], None, None)?;

    if repository.index()?.has_conflicts() {
        return Ok(SyncOutcome::Conflicted);
    }

    // libgit2 writes `MERGE_MSG` itself as part of `merge`, above — the same sentence `git commit`
    // would default to ("Merge branch 'origin/main'..."), read back rather than invented again.
    let message = repository.message().unwrap_or_else(|_| format!("Merge {ORIGIN} into the local branch"));
    commit(repository, &message)?;
    push(repository, token)?;
    Ok(SyncOutcome::Merged)
}

/// How many commits reach `to` and not `from` — `None` for "the whole history", which is what a
/// branch `origin` has never seen is ahead by.
fn commits_since(repository: &Repository, from: Option<git2::Oid>, to: git2::Oid) -> Result<usize, GitError> {
    let mut walk = repository.revwalk()?;
    walk.push(to)?;
    if let Some(from) = from {
        walk.hide(from)?;
    }
    Ok(walk.count())
}

/// Every blob, among the commits that reach `to` and not `from`, over `limit_bytes` — the same
/// range `commits_since` counts, because a blob a push sends is exactly a blob a commit *in that
/// range* added or changed. Diffed against each commit's first parent, the way `word_delta`
/// already does, rather than `to`'s tree against `from`'s: a file added and then deleted again
/// within the unpushed range is still a real object the push has to transfer, and a whole-tree
/// diff would never see it.
///
/// `DiffFile::size` is always 0 here — checked by hand before this was written as a comment
/// rather than assumed: libgit2 only fills that field from a workdir `stat`, and a tree entry
/// carries no size at all, only a mode and an id. `Odb::read_header` is the real answer, and the
/// right one to reach for: it asks the object database for the object's length from its header,
/// the same few bytes `git cat-file -s` reads, never inflating the blob's full content just to
/// measure it.
fn oversized_blobs(
    repository: &Repository,
    from: Option<git2::Oid>,
    to: git2::Oid,
    limit_bytes: u64,
) -> Result<Vec<(String, u64)>, GitError> {
    let mut walk = repository.revwalk()?;
    walk.push(to)?;
    if let Some(from) = from {
        walk.hide(from)?;
    }
    let odb = repository.odb()?;

    let mut found = Vec::new();
    for oid in walk {
        let commit = repository.find_commit(oid?)?;
        let new_tree = commit.tree()?;
        let old_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
        let diff = repository.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
        for change in diff.deltas() {
            if change.status() == git2::Delta::Deleted {
                continue;
            }
            let id = change.new_file().id();
            if id.is_zero() {
                continue;
            }
            let (size, _kind) = odb.read_header(id)?;
            let size = size as u64;
            if size > limit_bytes {
                if let Some(path) = change.new_file().path().map(path_string) {
                    found.push((path, size));
                }
            }
        }
    }
    Ok(found)
}

// ---------------------------------------------------------------------------------------------
// S11.7: the two sides of a file's diff, for the side-by-side view.
// ---------------------------------------------------------------------------------------------

/// The two texts a change row's side-by-side view compares.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSides {
    /// The older side: what the other side would replace. Empty for a file that did not exist there.
    pub before: String,
    /// The newer side. Empty for a file that no longer exists there.
    pub after: String,
}

/// What *Stage* (or *Unstage*) on this path would change, as two texts.
///
/// - `staged == false` — a row in *Changes*: the index against the working tree. Exactly what
///   *Stage* would add to the next commit (design interview B8).
/// - `staged == true` — a row in *Staged Changes*: `HEAD` against the index. Exactly what the next
///   commit would contain.
///
/// A side that does not exist (an untracked file's index entry, a deleted file's working copy, an
/// unborn branch's `HEAD`) is empty text, so the view shows a pure addition or a pure removal.
/// Reads only: nothing here touches the index or the working tree.
pub fn diff_sides(repository: &Repository, path: &str, staged: bool) -> Result<DiffSides, GitError> {
    let relative = Path::new(path);
    let stays_inside = !relative.is_absolute()
        && relative.components().all(|component| matches!(component, std::path::Component::Normal(_)));
    if !stays_inside {
        return Err(GitError::OutsideProject(path.to_string()));
    }

    let in_index = || -> Result<String, GitError> {
        let index = repository.index()?;
        match index.get_path(relative, 0) {
            Some(entry) => text_of_blob(repository, entry.id, path),
            None => Ok(String::new()),
        }
    };

    if staged {
        let before = match repository.head().and_then(|head| head.peel_to_tree()) {
            Ok(tree) => match tree.get_path(relative) {
                Ok(entry) => text_of_blob(repository, entry.id(), path)?,
                Err(_) => String::new(),
            },
            Err(_) => String::new(), // an unborn branch has no `HEAD` tree
        };
        Ok(DiffSides { before, after: in_index()? })
    } else {
        let after = match workdir_path(repository, path) {
            Some(file) if file.is_file() => {
                String::from_utf8(std::fs::read(&file)?).map_err(|_| GitError::NotText(path.to_string()))?
            }
            _ => String::new(),
        };
        Ok(DiffSides { before: in_index()?, after })
    }
}

/// A blob as text, refusing what is not UTF-8. (`blob_text`, above, is lossy on purpose: it feeds
/// a word count, where a replacement character costs nothing. Here the text is *shown*.)
fn text_of_blob(repository: &Repository, id: git2::Oid, path: &str) -> Result<String, GitError> {
    let blob = repository.find_blob(id)?;
    String::from_utf8(blob.content().to_vec()).map_err(|_| GitError::NotText(path.to_string()))
}

// ---------------------------------------------------------------------------------------------
// S11.5a: clone.
// ---------------------------------------------------------------------------------------------

/// A folder name for a repository URL: its last path segment without `.git`, which is what
/// `git clone` itself picks. `None` for a URL that has no usable segment (`https://github.com/`),
/// so the caller asks the author for a name rather than creating a folder called `""`.
///
/// Both separators are split on, because a local path on Windows is a repository URL too, and
/// `scp`-style `host:owner/repo.git` addresses end in the same way.
pub fn folder_name_for(url: &str) -> Option<String> {
    let url = url.trim();
    // `https://host/…`: only what follows the host can name a folder, so `https://github.com/`
    // has none rather than the host's own name.
    let path = match url.split_once("://") {
        Some((_scheme, rest)) => rest.split_once('/').map_or("", |(_host, path)| path),
        None => url,
    };
    let last = path.trim_end_matches(['/', '\\']).rsplit(['/', '\\', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last).trim();
    if name.is_empty() || name == "." || name == ".." {
        None
    } else {
        Some(name.to_string())
    }
}

/// Download the repository at `url` into `destination`, which must not exist yet or must be empty.
///
/// The second half of the exit demo's "continue on another machine, with no terminal": everything
/// after this is the same project the rest of the crate already knows how to open. `token` is
/// offered the way `fetch` and `push` offer it, and only when the remote asks.
///
/// **Leaves nothing behind on failure.** A clone that stops halfway (a wrong URL, a refused
/// credential, a network that went away) would otherwise leave a folder with a half-built `.git`
/// in it, which the next attempt would then refuse as "not empty". A folder this call created is
/// removed whole; an empty one it was given is emptied and left in place, since it was the
/// author's.
///
/// An *empty* remote (a repository just created on GitHub, nothing pushed) clones fine, into a
/// repository with no commits: the same unborn state `initialise` can leave, which the rest of
/// the crate already answers in sentences.
pub fn clone(url: &str, destination: &Path, token: Option<&str>) -> Result<Repository, GitError> {
    let existed = destination.exists();
    if existed && (!destination.is_dir() || std::fs::read_dir(destination)?.next().is_some()) {
        return Err(GitError::DestinationNotEmpty(destination.display().to_string()));
    }

    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(credentials(token));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(options);

    match builder.clone(url, destination) {
        Ok(repository) => Ok(repository),
        Err(error) => {
            // Best effort, and deliberately silent about its own failure: the clone's error is
            // the one the author needs, and a cleanup that also failed must not replace it.
            if existed {
                if let Ok(entries) = std::fs::read_dir(destination) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                    }
                }
            } else {
                let _ = std::fs::remove_dir_all(destination);
            }
            Err(error.into())
        }
    }
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

    // --- S11.3b -------------------------------------------------------------------------------

    /// The card's first done-when: an untracked file over the threshold is a candidate, and one
    /// at or under it is not.
    #[test]
    fn an_untracked_file_over_the_threshold_is_a_candidate_and_a_small_one_is_not() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("figure.png"), vec![0u8; 20]).unwrap();
        fs::write(tmp.path().join("icon.png"), vec![0u8; 5]).unwrap();

        let found = large_files(&repository, 10).unwrap();

        assert_eq!(found, vec![LargeFile { path: "figure.png".to_string(), size_bytes: 20 }]);
    }

    /// A large file already routed through the `lfs` filter is not a candidate — the whole point
    /// of the banner is to offer tracking, not to nag about a file that already is tracked.
    #[test]
    fn a_large_file_already_covered_by_gitattributes_is_not_a_candidate() {
        let (tmp, repository) = repo();
        // `.gitattributes` itself must stay under the threshold too, or it becomes a spurious
        // candidate of its own — it has no `filter=lfs` line naming itself.
        fs::write(tmp.path().join(".gitattributes"), "*.png filter=lfs diff=lfs merge=lfs -text\n").unwrap();
        fs::write(tmp.path().join("figure.png"), vec![0u8; 100]).unwrap();

        assert!(large_files(&repository, 50).unwrap().is_empty());
    }

    /// A deleted file has no working-tree bytes left to size up, and a staged large file is still
    /// found exactly once, not once per list it appears in.
    #[test]
    fn a_deleted_file_is_never_a_candidate_and_a_staged_one_is_found_once() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("figure.png"), vec![0u8; 20]).unwrap();
        stage(&repository, "figure.png").unwrap();
        fs::remove_file(tmp.path().join("main.tex")).unwrap();

        let found = large_files(&repository, 10).unwrap();

        assert_eq!(found, vec![LargeFile { path: "figure.png".to_string(), size_bytes: 20 }]);
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

    // --- S11.4a: exporting a historical tree ------------------------------------------------

    /// The card's first done-when: every file in a commit's tree, nested folders included, lands
    /// on disk with the content that commit actually has.
    #[test]
    fn exporting_a_commit_writes_every_file_in_its_tree() {
        let (tmp, repository) = repo();
        fs::create_dir(tmp.path().join("sections")).unwrap();
        fs::write(tmp.path().join("sections/intro.tex"), "the introduction\n").unwrap();
        commit_all(&repository, "add a section");
        let commit = repository.head().unwrap().target().unwrap();

        let dest = tempfile::tempdir().unwrap();
        export_tree(&repository, commit, dest.path()).unwrap();

        assert_eq!(fs::read_to_string(dest.path().join("main.tex")).unwrap(), "the manuscript\n");
        assert_eq!(fs::read_to_string(dest.path().join("sections/intro.tex")).unwrap(), "the introduction\n");
    }

    /// An older commit's export never sees what a later one added — this is reading history, not
    /// copying the working tree, which is the entire reason `latexdiff` needs it this way.
    #[test]
    fn exporting_an_older_commit_does_not_see_a_later_addition() {
        let (tmp, repository) = repo();
        let before = repository.head().unwrap().target().unwrap();
        fs::write(tmp.path().join("notes.tex"), "added later\n").unwrap();
        commit_all(&repository, "add notes");

        let dest = tempfile::tempdir().unwrap();
        export_tree(&repository, before, dest.path()).unwrap();

        assert!(dest.path().join("main.tex").exists());
        assert!(!dest.path().join("notes.tex").exists());
    }

    /// A file deleted by the commit being exported is absent from the export — the tree really
    /// is read as it was at that commit, not merged with whatever came before or after it.
    #[test]
    fn exporting_a_commit_that_deleted_a_file_leaves_it_out() {
        let (tmp, repository) = repo();
        fs::remove_file(tmp.path().join("main.tex")).unwrap();
        commit_all(&repository, "remove the manuscript");
        let commit = repository.head().unwrap().target().unwrap();

        let dest = tempfile::tempdir().unwrap();
        export_tree(&repository, commit, dest.path()).unwrap();

        assert!(!dest.path().join("main.tex").exists());
    }

    // --- S11.4c: the two questions a comparison asks before exporting anything ---------------

    /// A file a later commit added is not in an earlier one, a nested path is found by its full
    /// name, and a folder is not mistaken for a file.
    #[test]
    fn has_path_answers_for_the_commit_it_is_asked_about() {
        let (tmp, repository) = repo();
        let before = repository.head().unwrap().target().unwrap();
        fs::create_dir(tmp.path().join("paper")).unwrap();
        fs::write(tmp.path().join("paper/main.tex"), "moved in later\n").unwrap();
        commit_all(&repository, "add a paper folder");
        let after = repository.head().unwrap().target().unwrap();

        assert!(has_path(&repository, before, "main.tex").unwrap());
        assert!(!has_path(&repository, before, "paper/main.tex").unwrap());
        assert!(has_path(&repository, after, "paper/main.tex").unwrap());
        assert!(!has_path(&repository, after, "paper").unwrap(), "a folder is not a root file");
    }

    /// Whichever order the two are handed over in, the parent comes first — even when both were
    /// committed within the same second, which is the case a timestamp alone gets wrong.
    #[test]
    fn older_first_puts_the_ancestor_first_whatever_the_order() {
        let (tmp, repository) = repo();
        let parent = repository.head().unwrap().target().unwrap();
        fs::write(tmp.path().join("main.tex"), "the manuscript, revised\n").unwrap();
        commit_all(&repository, "revise");
        let child = repository.head().unwrap().target().unwrap();

        assert_eq!(older_first(&repository, parent, child).unwrap(), (parent, child));
        assert_eq!(older_first(&repository, child, parent).unwrap(), (parent, child));
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

    // -----------------------------------------------------------------------------------------
    // S10.3c: the writer's numbers.
    // -----------------------------------------------------------------------------------------

    /// A manuscript with two sections, so a change can be attributed to one of them.
    const TWO_SECTIONS: &str = "\\section{Methods}\nWe sampled forty people.\n\\section{Results}\nThe effect held.\n";

    #[test]
    fn the_summary_counts_prose_and_names_the_section_the_change_landed_in() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), TWO_SECTIONS).unwrap();
        commit_all(&repository, "first");

        // Four words added, all of them under Results.
        fs::write(
            tmp.path().join("main.tex"),
            "\\section{Methods}\nWe sampled forty people.\n\\section{Results}\nThe effect held for every single participant.\n",
        )
        .unwrap();

        let summary = prose_summary(&repository).unwrap();
        assert_eq!(summary.words_after as i64 - summary.words_before as i64, 4);
        assert_eq!(summary.sections, vec!["Results".to_string()]);
        assert_eq!(summary.paths, vec!["main.tex".to_string()]);
        assert!(summary.added_paths.is_empty());
    }

    #[test]
    fn a_new_file_is_named_as_added_and_all_of_its_words_are_new() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("notes.tex"), "\\section{Notes}\nOne two three four.\n").unwrap();

        let summary = prose_summary(&repository).unwrap();
        assert_eq!(summary.added_paths, vec!["notes.tex".to_string()]);
        assert_eq!(summary.words_before, 0);
        assert_eq!(summary.words_after, 5);
    }

    /// The whole reason `texwords` exists rather than a line count.
    #[test]
    fn markup_that_adds_lines_and_no_words_is_not_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "\\section{Theory}\nIt follows that x = y.\n").unwrap();
        commit_all(&repository, "first");

        // Four more lines, one fewer prose word ("x = y" becomes maths).
        fs::write(
            tmp.path().join("main.tex"),
            "\\section{Theory}\nIt follows that\n\\begin{align}\n  x &= y \\\\\n\\end{align}\n",
        )
        .unwrap();

        let summary = prose_summary(&repository).unwrap();
        assert!(
            summary.words_after < summary.words_before,
            "before {} after {}",
            summary.words_before,
            summary.words_after
        );
    }

    #[test]
    fn a_bib_file_a_figure_and_the_build_folder_are_not_prose() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("refs.bib"), "@article{a, title = {Several words of title here}}\n").unwrap();
        fs::write(tmp.path().join("figure.pdf"), b"%PDF and some words").unwrap();
        fs::create_dir_all(tmp.path().join(".abstract-tex/build")).unwrap();
        fs::write(tmp.path().join(".abstract-tex/build/main.tex"), "words words words words\n").unwrap();

        let summary = prose_summary(&repository).unwrap();
        assert_eq!(summary.paths, Vec::<String>::new());
        assert_eq!(summary.words_after, 0);
    }

    // -----------------------------------------------------------------------------------------
    // S10.5a: making a folder a repository.
    // -----------------------------------------------------------------------------------------

    /// A folder with a manuscript, a build folder and no Git at all — which is the state of
    /// every project this app opens until someone does something about it.
    fn folder_with_a_manuscript() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.tex"), "\\section{Methods}\nWords.\n").unwrap();
        fs::create_dir_all(tmp.path().join("sections")).unwrap();
        fs::write(tmp.path().join("sections/intro.tex"), "More words.\n").unwrap();
        fs::create_dir_all(tmp.path().join(".abstract-tex/build")).unwrap();
        fs::write(tmp.path().join(".abstract-tex/build/main.pdf"), b"%PDF").unwrap();
        fs::write(tmp.path().join("main.aux"), "\\relax\n").unwrap();
        tmp
    }

    /// Whatever `git ls-tree -r --name-only HEAD` would print, as a sorted list.
    fn files_in_head(repository: &Repository) -> Vec<String> {
        let tree = repository.head().unwrap().peel_to_tree().unwrap();
        let mut names = Vec::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                names.push(format!("{root}{}", entry.name().unwrap_or_default()));
            }
            git2::TreeWalkResult::Ok
        })
        .unwrap();
        names.sort();
        names
    }

    #[test]
    fn a_folder_becomes_a_repository_whose_first_commit_is_the_manuscript() {
        let tmp = folder_with_a_manuscript();
        let done = initialise(tmp.path()).unwrap();
        assert!(done.first_commit.is_some(), "a machine with an identity commits");
        assert!(!done.needs_identity);

        let repository = open(tmp.path()).unwrap();
        assert_eq!(files_in_head(&repository), vec![".gitignore", "main.tex", "sections/intro.tex"]);
        // The two things that must not be in it: our folder, and a stray `.aux`.
        assert!(our_folder_is_ignored(&repository));
        assert!(repository.is_path_ignored("main.aux").unwrap());
        // And the panel is clean immediately afterwards, which is the point of ignoring first.
        assert_eq!(changes(&repository), Status::default());
    }

    #[test]
    fn a_machine_with_no_identity_gets_a_repository_a_staged_tree_and_a_sentence() {
        let tmp = folder_with_a_manuscript();
        let repository = Repository::init(tmp.path()).unwrap();
        write_gitignore(tmp.path()).unwrap();

        // The state under test, reached through this repository's *own* config rather than by
        // taking the machine's identity away: libgit2 refuses to sign with an empty name, which
        // is the same refusal a machine with no `user.name` at all produces.
        let mut config = repository.config().unwrap();
        config.set_str("user.name", "").unwrap();
        config.set_str("user.email", "").unwrap();

        let done = stage_and_commit_everything(&repository).unwrap();
        assert_eq!(done.first_commit, None);
        assert!(done.needs_identity, "the author is told, rather than signed for");

        // Everything is staged, so their own first commit is one `git config` away.
        let status = changes(&repository);
        let staged: Vec<&str> = status.staged.iter().map(|change| change.path.as_str()).collect();
        assert_eq!(staged, vec![".gitignore", "main.tex", "sections/intro.tex"]);
    }

    #[test]
    fn an_empty_folder_becomes_a_repository_with_nothing_in_it_and_no_complaint() {
        let tmp = tempfile::tempdir().unwrap();
        let done = initialise(tmp.path()).unwrap();
        // The `.gitignore` is the only thing there, so there *is* a commit — the point of this
        // test is that the path does not error on a folder with no manuscript yet.
        assert!(done.first_commit.is_some());
        assert!(!done.needs_identity);
    }

    #[test]
    fn a_folder_already_inside_a_repository_is_refused_by_name() {
        let (tmp, _repository) = repo();
        let paper = tmp.path().join("paper");
        fs::create_dir(&paper).unwrap();

        let error = initialise(&paper).unwrap_err();
        assert!(matches!(error, GitError::AlreadyARepository(_)), "{error}");
        assert!(error.to_string().contains("already inside a Git repository"), "{error}");
    }

    #[test]
    fn an_existing_gitignore_is_added_to_and_never_replaced() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join(".gitignore"), "# mine\nscratch/\n").unwrap();
        assert!(!our_folder_is_ignored(&repository), "a repository that predates this app");

        ignore_our_folder(&repository).unwrap();

        let written = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(written.starts_with("# mine\nscratch/\n"), "the author's lines come first: {written}");
        assert!(written.contains(".abstract-tex/"));
        assert!(our_folder_is_ignored(&repository));

        // Asked twice, written once: no duplicate block.
        ignore_our_folder(&repository).unwrap();
        let again = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(again.matches(".abstract-tex/").count(), 1, "{again}");
    }

    // -----------------------------------------------------------------------------------------
    // S10.5b: the remote.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn a_fresh_repository_has_no_origin_and_then_has_the_one_it_was_given() {
        let (_tmp, repository) = repo();
        assert_eq!(origin_url(&repository), None, "nothing to publish to yet");

        set_origin(&repository, "https://github.com/ada/thesis.git").unwrap();
        assert_eq!(origin_url(&repository).as_deref(), Some("https://github.com/ada/thesis.git"));
    }

    #[test]
    fn setting_origin_twice_changes_the_url_and_keeps_what_git_knew() {
        let (_tmp, repository) = repo();
        set_origin(&repository, "https://github.com/ada/first.git").unwrap();

        // A remote-tracking branch, as a fetch would have left behind.
        let head = repository.head().unwrap().peel_to_commit().unwrap();
        repository.reference("refs/remotes/origin/main", head.id(), true, "test").unwrap();

        set_origin(&repository, "https://github.com/ada/second.git").unwrap();

        assert_eq!(origin_url(&repository).as_deref(), Some("https://github.com/ada/second.git"));
        // Delete-and-add would have taken this with it.
        assert!(repository.find_reference("refs/remotes/origin/main").is_ok(), "tracking refs survive");
    }

    #[test]
    fn a_graph_row_carries_the_words_its_own_commit_added() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "One two three.\n").unwrap();
        commit_all(&repository, "three words");
        fs::write(tmp.path().join("main.tex"), "One two three four five.\n").unwrap();
        commit_all(&repository, "two more");
        fs::write(tmp.path().join("main.tex"), "One.\n").unwrap();
        commit_all(&repository, "cut it back");

        let rows = log(&repository, 0, 10).unwrap();
        assert_eq!(rows[0].summary, "cut it back");
        assert_eq!(rows[0].word_delta, -4, "a commit that cuts prose reads as negative");
        assert_eq!(rows[1].word_delta, 2);
        // The root commit has no parent, so everything in it is new.
        assert_eq!(rows[2].word_delta, 3);
    }

    // -----------------------------------------------------------------------------------------
    // S11.1a: push, fetch, and the sync decision.
    //
    // A bare repository stands in for GitHub: a real transport (local, not `file://`, but the
    // same push/fetch machinery any transport uses), with no credentials involved, which is what
    // lets every test below pass `token: None` and still prove the credential path is never
    // touched by accident — `credentials()` returns an `Err` the moment it is asked for one it
    // does not have, so a test that reached that branch would fail loudly, not quietly.
    // -----------------------------------------------------------------------------------------

    /// A bare repository with nothing in it — the local stand-in for a freshly created GitHub
    /// repository before anything has ever been pushed to it.
    fn bare_remote() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        Repository::init_bare(tmp.path()).unwrap();
        tmp
    }

    /// A repository with one commit, pointed at an empty bare "remote" named `origin`, and the
    /// branch name both of them will use (whatever this machine's Git defaults to — never
    /// hardcoded, the same way `current_branch` never hardcodes it).
    fn repo_with_empty_remote() -> (tempfile::TempDir, Repository, tempfile::TempDir, String) {
        let (tmp, repository) = repo();
        let bare = bare_remote();
        set_origin(&repository, bare.path().to_str().unwrap()).unwrap();
        let branch = repository.head().unwrap().shorthand().unwrap().to_string();
        (tmp, repository, bare, branch)
    }

    /// Write a commit straight into a bare repository, standing in for a coauthor's `git push` —
    /// there is no working tree here to `fs::write` into, which is the whole reason this cannot
    /// just reuse `commit_all`.
    fn commit_into_bare(bare_path: &Path, branch: &str, file: &str, contents: &str, message: &str) -> git2::Oid {
        let bare = Repository::open_bare(bare_path).unwrap();
        let branch_ref = format!("refs/heads/{branch}");
        let parent = bare.find_reference(&branch_ref).unwrap().peel_to_commit().unwrap();
        let mut builder = bare.treebuilder(Some(&parent.tree().unwrap())).unwrap();
        let blob = bare.blob(contents.as_bytes()).unwrap();
        builder.insert(file, blob, 0o100_644).unwrap();
        let tree = bare.find_tree(builder.write().unwrap()).unwrap();
        let who = git2::Signature::now("Coauthor", "coauthor@example.invalid").unwrap();
        bare.commit(Some(&branch_ref), &who, &who, message, &tree, &[&parent]).unwrap()
    }

    /// The branch ref a bare repository has, read the same way a real clone would.
    fn bare_tip(bare_path: &Path, branch: &str) -> git2::Oid {
        Repository::open_bare(bare_path).unwrap().find_reference(&format!("refs/heads/{branch}")).unwrap().target().unwrap()
    }

    /// The card's first done-when: a push moves the remote and its own tracking ref, with no
    /// second fetch, and sets up the tracking relationship `branch_state` reads.
    #[test]
    fn pushing_moves_the_remote_and_its_own_tracking_ref_with_no_second_fetch() {
        let (_tmp, repository, bare, branch) = repo_with_empty_remote();
        let local_head = repository.head().unwrap().target().unwrap();

        push(&repository, None).unwrap();

        assert_eq!(bare_tip(bare.path(), &branch), local_head, "the remote did not move");
        let tracking = repository.find_reference(&format!("refs/remotes/origin/{branch}")).unwrap().target();
        assert_eq!(tracking, Some(local_head), "the local tracking ref was not updated by the push itself");

        // The upstream relationship is set, which is what lets `branch_state` read ahead/behind
        // at all rather than reporting `None` ("no remote yet" by S10.2b's own distinction).
        let state = branch_state(&repository).unwrap();
        assert_eq!(state.ahead_behind, Some((0, 0)));
    }

    /// `sync` on a branch `origin` has never seen pushes everything, by name, and then has
    /// nothing left to do — the card's second done-when.
    #[test]
    fn syncing_an_unpushed_branch_pushes_everything_and_then_reports_up_to_date() {
        let (_tmp, repository, _bare, _branch) = repo_with_empty_remote();

        let first = sync(&repository, None).unwrap();
        assert_eq!(first, SyncOutcome::Pushed { ahead: 1 });

        let second = sync(&repository, None).unwrap();
        assert_eq!(second, SyncOutcome::UpToDate);
    }

    /// Fetching moves only the remote-tracking ref. `HEAD`, the branch, and the working tree are
    /// not `sync`'s to touch — that is the half of the card `fast_forward` owns, tested below.
    #[test]
    fn fetching_updates_only_the_remote_tracking_ref() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        let local_head = repository.head().unwrap().target().unwrap();
        let coauthor_commit = commit_into_bare(bare.path(), &branch, "main.tex", "a coauthor's words\n", "coauthor");

        fetch(&repository, None).unwrap();

        assert_eq!(repository.head().unwrap().target(), Some(local_head), "HEAD moved on a fetch");
        let tracking = repository.find_reference(&format!("refs/remotes/origin/{branch}")).unwrap().target();
        assert_eq!(tracking, Some(coauthor_commit));
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "the manuscript\n", "the working tree moved on a fetch");
    }

    /// The card's third done-when: a remote that moved ahead, with nothing local to lose, fast-
    /// forwards the branch *and* the working tree — not just a ref update nobody can see.
    #[test]
    fn syncing_fast_forwards_the_branch_and_the_working_tree_when_only_the_remote_moved() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        let coauthor_commit = commit_into_bare(bare.path(), &branch, "main.tex", "a coauthor's words\n", "coauthor");

        let outcome = sync(&repository, None).unwrap();

        assert_eq!(outcome, SyncOutcome::FastForwarded { behind: 1 });
        assert_eq!(repository.head().unwrap().target(), Some(coauthor_commit));
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "a coauthor's words\n");
        assert_eq!(changes(&repository), Status::default(), "a fast-forward must not look like a change");
    }

    /// The other half of the same decision: nothing to pull, something to push.
    #[test]
    fn syncing_pushes_when_only_the_local_side_moved() {
        let (_tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        fs::write(_tmp.path().join("main.tex"), "a new local sentence\n").unwrap();
        commit_all(&repository, "local edit");
        let local_head = repository.head().unwrap().target().unwrap();

        let outcome = sync(&repository, None).unwrap();

        assert_eq!(outcome, SyncOutcome::Pushed { ahead: 1 });
        assert_eq!(bare_tip(bare.path(), &branch), local_head);
    }

    // -----------------------------------------------------------------------------------------
    // S11.2a: real merges.
    // -----------------------------------------------------------------------------------------

    /// Both sides moved but touched different files: libgit2 merges them with no conflict, and
    /// `sync` finishes the merge commit itself and pushes it on in the same call — §5.7's
    /// "commit, pull, rebase, push" as one verb, all the way through.
    #[test]
    fn syncing_a_clean_divergence_merges_and_pushes_it_on() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        let remote_commit = commit_into_bare(bare.path(), &branch, "notes.tex", "a coauthor's notes\n", "coauthor");
        fs::write(tmp.path().join("chapter.tex"), "a local chapter\n").unwrap();
        commit_all(&repository, "local edit");
        let local_head = repository.head().unwrap().target().unwrap();

        let outcome = sync(&repository, None).unwrap();

        assert_eq!(outcome, SyncOutcome::Merged);
        let merged = repository.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(merged.parent_count(), 2, "a merge commit has both parents");
        assert!(merged.parent_ids().any(|id| id == local_head));
        assert!(merged.parent_ids().any(|id| id == remote_commit));
        assert_eq!(fs::read_to_string(tmp.path().join("notes.tex")).unwrap(), "a coauthor's notes\n");
        assert_eq!(fs::read_to_string(tmp.path().join("chapter.tex")).unwrap(), "a local chapter\n");
        assert_eq!(bare_tip(bare.path(), &branch), merged.id(), "the merge reached the remote");
        assert_eq!(repository.state(), git2::RepositoryState::Clean);
    }

    /// Both sides edited the same line: libgit2 cannot resolve it on its own, and `sync` leaves
    /// exactly the state a terminal `git merge` would — real markers on disk, a real `MERGE_HEAD`,
    /// nothing committed and nothing pushed.
    #[test]
    fn syncing_a_real_conflict_leaves_real_markers_and_merge_head() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        commit_into_bare(bare.path(), &branch, "main.tex", "the coauthor's version\n", "coauthor");
        fs::write(tmp.path().join("main.tex"), "the local version\n").unwrap();
        commit_all(&repository, "local edit");

        let outcome = sync(&repository, None).unwrap();

        assert_eq!(outcome, SyncOutcome::Conflicted);
        assert_eq!(repository.state(), git2::RepositoryState::Merge);
        assert!(repository.path().join("MERGE_HEAD").exists());
        let on_disk = fs::read_to_string(tmp.path().join("main.tex")).unwrap();
        assert!(on_disk.contains("<<<<<<<"), "a real conflict marker belongs on disk: {on_disk}");
        assert!(changes(&repository).conflicted.iter().any(|c| c.path == "main.tex"));
    }

    /// Resolving needs no new verb: fix the file, stage it the ordinary way, commit — and the
    /// merge finishes with a two-parent commit, `MERGE_HEAD` gone, `state()` clean again.
    #[test]
    fn resolving_a_conflict_and_committing_finishes_the_merge() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        let remote_commit = commit_into_bare(bare.path(), &branch, "main.tex", "the coauthor's version\n", "coauthor");
        fs::write(tmp.path().join("main.tex"), "the local version\n").unwrap();
        commit_all(&repository, "local edit");
        let local_head = repository.head().unwrap().target().unwrap();
        assert_eq!(sync(&repository, None).unwrap(), SyncOutcome::Conflicted);

        fs::write(tmp.path().join("main.tex"), "the resolved version\n").unwrap();
        stage(&repository, "main.tex").unwrap();
        assert!(changes(&repository).conflicted.is_empty(), "staging the fixed file resolves it");

        let id = commit(&repository, "resolve the conflict").unwrap();

        let merged = repository.find_commit(git2::Oid::from_str(&id).unwrap()).unwrap();
        assert_eq!(merged.parent_count(), 2);
        assert!(merged.parent_ids().any(|parent_id| parent_id == local_head));
        assert!(merged.parent_ids().any(|parent_id| parent_id == remote_commit));
        assert_eq!(repository.state(), git2::RepositoryState::Clean, "MERGE_HEAD should be gone");
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "the resolved version\n");
    }

    /// The card's one new refusal: a commit attempted while anything is still conflicted.
    #[test]
    fn committing_while_still_conflicted_is_refused() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        commit_into_bare(bare.path(), &branch, "main.tex", "the coauthor's version\n", "coauthor");
        fs::write(tmp.path().join("main.tex"), "the local version\n").unwrap();
        commit_all(&repository, "local edit");
        assert_eq!(sync(&repository, None).unwrap(), SyncOutcome::Conflicted);

        let error = commit(&repository, "too soon").unwrap_err();

        assert!(matches!(error, GitError::UnresolvedConflicts), "{error:?}");
    }

    /// A stale local branch is refused before anything is sent — checked against a real local
    /// remote (`ffcheck`, by hand, is what found this) rather than assumed from the docs, because
    /// it is what makes a plain `push` (the Commit dropdown's *Commit & Push*) safe to call
    /// without fetching first: it can fail, but it can never silently discard a coauthor's commit.
    #[test]
    fn pushing_a_stale_branch_is_refused_rather_than_overwriting_the_remote() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        let remote_commit = commit_into_bare(bare.path(), &branch, "main.tex", "a coauthor's words\n", "coauthor");
        // A local commit built on the *old* tip, so this push really is non-fast-forward and not
        // a no-op.
        fs::write(tmp.path().join("notes.tex"), "a local addition\n").unwrap();
        commit_all(&repository, "local edit");

        let error = push(&repository, None).unwrap_err();

        assert!(matches!(error, GitError::Git(_)), "{error:?}");
        assert_eq!(bare_tip(bare.path(), &branch), remote_commit, "the rejected push must not have moved the remote");
    }

    // -----------------------------------------------------------------------------------------
    // S11.3a: the oversize catch.
    // -----------------------------------------------------------------------------------------

    /// `oversized_blobs` is tested against a tiny limit rather than GitHub's real 100 MiB one —
    /// the behaviour under test is the *range* (which commits count) and the *diff-against-parent*
    /// logic (word_delta's own pattern), neither of which needs a megabyte-scale fixture to prove.
    #[test]
    fn a_blob_over_the_limit_is_found_even_if_a_later_unpushed_commit_deletes_it() {
        let (tmp, repository) = repo();
        let before_large_file = repository.head().unwrap().target().unwrap();

        fs::write(tmp.path().join("figure.png"), vec![0u8; 20]).unwrap();
        commit_all(&repository, "add a large figure");
        fs::remove_file(tmp.path().join("figure.png")).unwrap();
        commit_all(&repository, "remove it again");
        let head = repository.head().unwrap().target().unwrap();

        // Still found: the blob was a real object in a commit this range covers, even though the
        // final tree at `head` no longer contains it — a whole-tree diff would have missed it.
        let found = oversized_blobs(&repository, Some(before_large_file), head, 10).unwrap();
        assert_eq!(found, vec![("figure.png".to_string(), 20)]);

        // Not found with the real limit raised above the fixture's size.
        let none = oversized_blobs(&repository, Some(before_large_file), head, 20).unwrap();
        assert!(none.is_empty(), "{none:?}");
    }

    /// `oversized_blobs` walks only the unpushed range — `commits_since`'s own range — so a large
    /// file already on the remote is never re-flagged on a later, unrelated push.
    #[test]
    fn a_blob_already_on_the_remote_is_not_flagged_again() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("figure.png"), vec![0u8; 20]).unwrap();
        commit_all(&repository, "add a large figure");
        let already_pushed = repository.head().unwrap().target().unwrap();

        fs::write(tmp.path().join("notes.tex"), "edit\n").unwrap();
        commit_all(&repository, "later, small edit");
        let head = repository.head().unwrap().target().unwrap();

        let found = oversized_blobs(&repository, Some(already_pushed), head, 10).unwrap();
        assert!(found.is_empty(), "{found:?}");
    }

    /// The card's done-when: a push that would send a file over GitHub's real limit is refused by
    /// name, before the network call, and the remote is left untouched.
    #[test]
    fn pushing_a_file_over_githubs_limit_is_refused_by_name_and_touches_no_remote() {
        let (tmp, repository, bare, branch) = repo_with_empty_remote();
        let oversized = GITHUB_FILE_LIMIT_BYTES as usize + 1;
        fs::write(tmp.path().join("figure.png"), vec![0u8; oversized]).unwrap();
        commit_all(&repository, "add an oversized figure");

        let error = push(&repository, None).unwrap_err();

        match error {
            GitError::FileTooLarge(path, megabytes) => {
                assert_eq!(path, "figure.png");
                assert!(megabytes > 100, "{megabytes} MB should read as over the 100 MB limit");
            }
            other => panic!("{other:?}"),
        }
        assert!(repository.find_reference(&format!("refs/remotes/origin/{branch}")).is_err(), "a refused push must not update the tracking ref");
        assert!(Repository::open_bare(bare.path()).unwrap().find_reference(&format!("refs/heads/{branch}")).is_err(), "the bare remote must still have no branch at all");
    }

    /// `push` and `fetch` both refuse the same way on a repository with no commits yet — there is
    /// no branch for either verb to act on.
    #[test]
    fn pushing_or_fetching_with_no_branch_checked_out_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        let bare = bare_remote();
        set_origin(&repository, bare.path().to_str().unwrap()).unwrap();

        assert!(matches!(push(&repository, None), Err(GitError::NoBranch)));
        assert!(matches!(fetch(&repository, None), Err(GitError::NoBranch)));
    }

    /// A repository with a branch but no remote gets a sentence of its own, not `git2`'s "remote
    /// 'origin' does not exist".
    #[test]
    fn syncing_with_no_remote_says_so_by_name() {
        let (_tmp, repository) = repo();
        assert!(matches!(sync(&repository, None), Err(GitError::NoRemote)));
    }

    // -----------------------------------------------------------------------------------------
    // S11.1c: amend.
    // -----------------------------------------------------------------------------------------

    /// The card's first done-when: a reword with nothing staged changes the message and nothing
    /// about the tree or the parent list.
    #[test]
    fn amending_with_nothing_staged_rewords_in_place() {
        let (tmp, repository) = repo();
        let before = repository.head().unwrap().peel_to_commit().unwrap();

        let new_id = amend(&repository, "a better first line").unwrap();

        let after = repository.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(after.id().to_string(), new_id, "HEAD moved to the amended commit");
        assert_ne!(after.id(), before.id(), "amending still writes a new commit object");
        assert_eq!(after.summary(), Some("a better first line"));
        assert_eq!(after.tree_id(), before.tree_id(), "nothing was staged, so the tree is untouched");
        assert_eq!(after.parent_count(), 0, "the root commit is still parentless");
        assert_eq!(fs::read_to_string(tmp.path().join("main.tex")).unwrap(), "the manuscript\n");
    }

    /// The card's second done-when: a staged change folds into `HEAD` rather than becoming its
    /// own commit.
    #[test]
    fn amending_with_something_staged_folds_it_into_head() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "a better manuscript\n").unwrap();
        stage(&repository, "main.tex").unwrap();

        amend(&repository, "first").unwrap();

        let rows = log(&repository, 0, 10).unwrap();
        assert_eq!(rows.len(), 1, "no second commit was created: {rows:?}");
        let tree = repository.head().unwrap().peel_to_tree().unwrap();
        let blob = tree.get_path(Path::new("main.tex")).unwrap().to_object(&repository).unwrap();
        assert_eq!(blob.as_blob().unwrap().content(), b"a better manuscript\n");
        assert!(changes(&repository).staged.is_empty(), "the amend consumed the staged change");
    }

    /// Amending the newest of several commits must not touch what it is built on.
    #[test]
    fn amending_keeps_the_same_parent() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "second version\n").unwrap();
        commit_all(&repository, "second");
        let first_parent = repository.head().unwrap().peel_to_commit().unwrap().parent_id(0).unwrap();

        amend(&repository, "a better second message").unwrap();

        let amended = repository.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(amended.parent_count(), 1);
        assert_eq!(amended.parent_id(0).unwrap(), first_parent, "amending rewrote what it is built on");
    }

    /// An empty message is refused exactly like a plain commit's, and refuses before writing
    /// anything: `HEAD` is still the original commit.
    #[test]
    fn amending_with_an_empty_message_is_refused_and_changes_nothing() {
        let (_tmp, repository) = repo();
        let before = repository.head().unwrap().peel_to_commit().unwrap().id();

        let error = amend(&repository, "   ").unwrap_err();

        assert!(matches!(error, GitError::EmptyMessage), "{error:?}");
        assert_eq!(repository.head().unwrap().peel_to_commit().unwrap().id(), before);
    }

    /// There is nothing for *Amend* to do before the first commit exists, and the sentence says
    /// so rather than `git2`'s own unborn-`HEAD` error.
    #[test]
    fn amending_a_repository_with_no_commits_yet_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();

        assert!(matches!(amend(&repository, "anything"), Err(GitError::NothingToAmend)));
    }

    // -----------------------------------------------------------------------------------------
    // S11.5a: clone.
    // -----------------------------------------------------------------------------------------

    /// A bare repository holding one pushed commit (`main.tex`), standing in for a project a
    /// coauthor already has on GitHub. Built by pushing a real working repository into it, the
    /// way the first machine's *Sync* would have.
    fn published_project() -> (tempfile::TempDir, tempfile::TempDir) {
        let (work, repository, bare, _branch) = repo_with_empty_remote();
        push(&repository, None).unwrap();
        (work, bare)
    }

    #[test]
    fn clone_brings_down_the_files_and_the_history_and_points_origin_back() {
        let (_work, bare) = published_project();
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("paper");

        let cloned = clone(bare.path().to_str().unwrap(), &destination, None).unwrap();

        assert_eq!(fs::read_to_string(destination.join("main.tex")).unwrap(), "the manuscript\n");
        assert_eq!(log(&cloned, 0, 10).unwrap().len(), 1);
        assert_eq!(origin_url(&cloned).as_deref(), bare.path().to_str());
        // Tracking is set up, so the Sync button's arrows have something to compare against.
        assert_eq!(branch_state(&cloned).unwrap().ahead_behind, Some((0, 0)));
    }

    #[test]
    fn clone_makes_the_folder_and_its_parents_when_they_are_not_there() {
        let (_work, bare) = published_project();
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("papers").join("2026").join("thesis");

        clone(bare.path().to_str().unwrap(), &destination, None).unwrap();

        assert!(destination.join("main.tex").exists());
    }

    #[test]
    fn clone_into_an_existing_empty_folder_is_fine() {
        let (_work, bare) = published_project();
        let destination = tempfile::tempdir().unwrap();

        clone(bare.path().to_str().unwrap(), destination.path(), None).unwrap();

        assert!(destination.path().join("main.tex").exists());
    }

    #[test]
    fn clone_refuses_a_folder_with_files_in_it_and_touches_nothing() {
        let (_work, bare) = published_project();
        let destination = tempfile::tempdir().unwrap();
        fs::write(destination.path().join("mine.txt"), "keep me").unwrap();

        let error = clone(bare.path().to_str().unwrap(), destination.path(), None).err().unwrap();

        assert!(matches!(error, GitError::DestinationNotEmpty(_)), "{error:?}");
        assert_eq!(fs::read_to_string(destination.path().join("mine.txt")).unwrap(), "keep me");
        assert!(!destination.path().join("main.tex").exists());
    }

    #[test]
    fn a_failed_clone_removes_the_folder_it_made() {
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("paper");
        let nowhere = parent.path().join("no-such-repository");

        let error = clone(nowhere.to_str().unwrap(), &destination, None).err().unwrap();

        assert!(matches!(error, GitError::Git(_)), "{error:?}");
        assert!(!destination.exists(), "a half-made folder was left behind");
    }

    #[test]
    fn a_failed_clone_empties_but_keeps_a_folder_it_was_given() {
        let parent = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let nowhere = parent.path().join("no-such-repository");

        clone(nowhere.to_str().unwrap(), destination.path(), None).err().unwrap();

        assert!(destination.path().is_dir());
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_empty_remote_clones_into_a_repository_with_no_commits() {
        let bare = bare_remote();
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("fresh");

        let cloned = clone(bare.path().to_str().unwrap(), &destination, None).unwrap();

        assert!(branch_state(&cloned).unwrap().unborn);
    }

    #[test]
    fn folder_names_follow_what_git_clone_picks() {
        assert_eq!(folder_name_for("https://github.com/ada/thesis.git").as_deref(), Some("thesis"));
        assert_eq!(folder_name_for("https://github.com/ada/thesis").as_deref(), Some("thesis"));
        assert_eq!(folder_name_for("https://github.com/ada/thesis/").as_deref(), Some("thesis"));
        assert_eq!(folder_name_for("git@github.com:ada/thesis.git").as_deref(), Some("thesis"));
        assert_eq!(folder_name_for(r"C:\repos\thesis.git").as_deref(), Some("thesis"));
        assert_eq!(folder_name_for("https://github.com/"), None);
        assert_eq!(folder_name_for(""), None);
        assert_eq!(folder_name_for("   "), None);
    }

    #[test]
    fn the_github_token_is_only_for_github() {
        assert!(is_github_https("https://github.com/ada/thesis.git"));
        assert!(is_github_https("https://GitHub.com/ada/thesis"));
        assert!(is_github_https("https://ada@github.com/ada/thesis.git"));
        assert!(is_github_https("https://github.com:443/ada/thesis.git"));

        assert!(!is_github_https("https://gitlab.com/ada/thesis.git"));
        assert!(!is_github_https("https://github.com.evil.example/ada/thesis.git"));
        assert!(!is_github_https("https://evilgithub.com/ada/thesis.git"));
        assert!(!is_github_https("https://github.com@evil.example/ada/thesis.git"));
        assert!(!is_github_https("http://github.com/ada/thesis.git"), "plain http would send it in the clear");
        assert!(!is_github_https("git@github.com:ada/thesis.git"));
        assert!(!is_github_https("/home/ada/thesis.git"));
    }

    // -----------------------------------------------------------------------------------------
    // S11.7: the two sides of a diff.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn a_changes_row_compares_the_index_with_the_working_tree() {
        let (tmp, repository) = repo(); // main.tex = "the manuscript\n", committed
        fs::write(tmp.path().join("main.tex"), "the manuscript, revised\n").unwrap();

        let sides = diff_sides(&repository, "main.tex", false).unwrap();

        assert_eq!(sides.before, "the manuscript\n");
        assert_eq!(sides.after, "the manuscript, revised\n");
    }

    #[test]
    fn once_staged_the_changes_side_is_empty_and_the_staged_side_has_the_edit() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "staged edit\n").unwrap();
        stage(&repository, "main.tex").unwrap();

        let unstaged = diff_sides(&repository, "main.tex", false).unwrap();
        let staged = diff_sides(&repository, "main.tex", true).unwrap();

        assert_eq!(unstaged.before, unstaged.after, "nothing left to stage");
        assert_eq!(staged.before, "the manuscript\n");
        assert_eq!(staged.after, "staged edit\n");
    }

    #[test]
    fn a_file_staged_and_edited_again_shows_each_row_its_own_pair() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "staged\n").unwrap();
        stage(&repository, "main.tex").unwrap();
        fs::write(tmp.path().join("main.tex"), "edited again\n").unwrap();

        assert_eq!(diff_sides(&repository, "main.tex", false).unwrap(), DiffSides { before: "staged\n".into(), after: "edited again\n".into() });
        assert_eq!(diff_sides(&repository, "main.tex", true).unwrap(), DiffSides { before: "the manuscript\n".into(), after: "staged\n".into() });
    }

    #[test]
    fn a_new_file_is_a_pure_addition_and_a_deleted_one_a_pure_removal() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("new.tex"), "brand new\n").unwrap();
        let untracked = diff_sides(&repository, "new.tex", false).unwrap();
        assert_eq!((untracked.before.as_str(), untracked.after.as_str()), ("", "brand new\n"));

        fs::remove_file(tmp.path().join("main.tex")).unwrap();
        let deleted = diff_sides(&repository, "main.tex", false).unwrap();
        assert_eq!((deleted.before.as_str(), deleted.after.as_str()), ("the manuscript\n", ""));
    }

    #[test]
    fn a_staged_file_on_an_unborn_branch_has_an_empty_before() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("first.tex"), "first words\n").unwrap();
        stage(&repository, "first.tex").unwrap();

        let sides = diff_sides(&repository, "first.tex", true).unwrap();

        assert_eq!((sides.before.as_str(), sides.after.as_str()), ("", "first words\n"));
    }

    #[test]
    fn diff_sides_reads_without_changing_anything_and_refuses_paths_that_leave_the_project() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "edited\n").unwrap();
        let before = changes(&repository);

        diff_sides(&repository, "main.tex", false).unwrap();
        diff_sides(&repository, "main.tex", true).unwrap();

        assert_eq!(changes(&repository), before, "looking at a diff must not stage anything");
        for bad in ["../elsewhere.tex", "/etc/passwd", "a/../../b.tex"] {
            assert!(matches!(diff_sides(&repository, bad, false), Err(GitError::OutsideProject(_))), "{bad}");
        }
    }

    #[test]
    fn a_file_that_is_not_utf8_is_refused_rather_than_shown_garbled() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), [0xff, 0xfe, 0x00]).unwrap();

        assert!(matches!(diff_sides(&repository, "main.tex", false), Err(GitError::NotText(_))));
    }
}
