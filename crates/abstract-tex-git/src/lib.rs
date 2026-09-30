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

    /// S10.5a: `git init` inside a folder that is already in a repository. Names what was found,
    /// because the answer depends on which repository it is — often one the author forgot they
    /// had, occasionally a monorepo where the paper is meant to live.
    #[error("This folder is already inside a Git repository ({0}).")]
    AlreadyARepository(String),
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
}
