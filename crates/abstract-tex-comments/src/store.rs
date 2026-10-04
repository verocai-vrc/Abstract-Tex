//! Where comments live: a hidden ref, the same precedent as `abstract-tex-snapshot`'s
//! `refs/abstract-tex/snapshots`, but — unlike a snapshot — one this crate pushes and fetches on
//! purpose, because a comment made offline has to reach the other author somehow.
//!
//! **The design risk this spike exists to test:** two authors who each comment while the other is
//! offline build two different histories on `refs/abstract-tex/comments` with no common ancestor.
//! An ordinary Git branch merge handles that by diffing each side against where they split and
//! resolving unchanged-by-the-other-side regions automatically — but *comments* have no shared
//! starting point to diff against. [`merge_from`] answers this by storing every comment as its
//! own blob, named by the blob's own hash (content addressing, the same trick Git itself already
//! uses for objects), inserted into a flat tree. Two authors adding different comments add
//! different filenames, so the tree merge sees two pure additions and never a conflict — Git's
//! ordinary 3-way merge already knows how to do that; nothing here special-cases it.
//!
//! **What this does not solve, on purpose — a spike finds the edges, not just the happy path:**
//! two authors editing the *same* comment (resolving it, say) would be two different blobs at the
//! same path only if the path were derived from something other than content, which it isn't
//! here — so an edit is a *new* comment that supersedes an old one, never an in-place rewrite.
//! [`CommentRecord`] has no `resolved` field yet for exactly this reason: that needs an
//! append-only "event" of its own (a resolution record pointing at the comment it resolves) so
//! two authors resolving the same thread while offline still merge as two additions rather than a
//! conflict, and that is a follow-on design question for S14.3, not an answer this spike gives.

use std::collections::BTreeMap;

use git2::{Commit, Oid, Repository, Signature, Tree};
use serde::{Deserialize, Serialize};

use crate::anchor::{self, Anchor, OrphanReason, Reanchored};

/// A ref outside `refs/heads/` and `refs/tags/`: invisible to `git branch`/`git log`, not sent by
/// a plain `git push`, exactly as `abstract-tex-snapshot::SNAPSHOT_REF`'s doc comment explains.
/// The difference from that ref is the next line of this crate's module doc: this one *is* meant
/// to be pushed and fetched, just always by this exact name, never by whatever branch the author
/// happens to be on.
pub const COMMENTS_REF: &str = "refs/abstract-tex/comments";

/// One comment, as it is written into a blob. `file` is project-relative with forward slashes, so
/// a comment made on Windows reanchors correctly when read on Linux or inside a Git ref that
/// knows nothing about either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRecord {
    pub file: String,
    pub anchor: Anchor,
    pub author: String,
    pub body: String,
    /// Seconds since the Unix epoch. Not a merge key — content addressing already gives every
    /// comment a stable identity — only something to sort by when showing a thread in order.
    pub created: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum CommentsError {
    #[error("the remote could not be reached: {0}")]
    Remote(#[source] git2::Error),
    #[error("a comment could not be written: {0}")]
    Write(#[source] git2::Error),
    #[error("a comment could not be read back: {0}")]
    Read(#[source] git2::Error),
    #[error("a stored comment was not valid JSON: {0}")]
    Malformed(#[source] serde_json::Error),
}

/// Add one comment, as a new commit on [`COMMENTS_REF`] with the previous tip as its parent (or
/// no parent, the first time). Returns the comment's id: the hex of the blob it was written as,
/// which is also the filename it was inserted under — see the module doc on why content
/// addressing is the point, not an implementation detail.
pub fn add(repository: &Repository, record: &CommentRecord) -> Result<String, CommentsError> {
    let json = serde_json::to_vec_pretty(record).map_err(CommentsError::Malformed)?;
    let blob = repository.blob(&json).map_err(CommentsError::Write)?;
    let filename = format!("{blob}.json");

    let parent = current_commit(repository)?;
    let base_tree = parent
        .as_ref()
        .map(Commit::tree)
        .transpose()
        .map_err(CommentsError::Read)?;
    let mut builder = repository
        .treebuilder(base_tree.as_ref())
        .map_err(CommentsError::Write)?;
    builder
        .insert(&filename, blob, git2::FileMode::Blob.into())
        .map_err(CommentsError::Write)?;
    let tree_id = builder.write().map_err(CommentsError::Write)?;
    let tree = repository.find_tree(tree_id).map_err(CommentsError::Write)?;

    let who = author(repository);
    let parents: Vec<&Commit<'_>> = parent.iter().collect();
    let commit_id = repository
        .commit(None, &who, &who, "Add a comment", &tree, &parents)
        .map_err(CommentsError::Write)?;
    repository
        .reference(COMMENTS_REF, commit_id, true, "add comment")
        .map_err(CommentsError::Write)?;
    Ok(blob.to_string())
}

/// Every comment currently on [`COMMENTS_REF`], with no attempt to reanchor them — see [`load`]
/// for the version a caller with files on disk actually wants.
pub fn load_all(repository: &Repository) -> Result<Vec<(String, CommentRecord)>, CommentsError> {
    let Some(commit) = current_commit(repository)? else {
        return Ok(Vec::new());
    };
    let tree = commit.tree().map_err(CommentsError::Read)?;
    let mut out = Vec::new();
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let object = entry.to_object(repository).map_err(CommentsError::Read)?;
        let Some(blob) = object.as_blob() else { continue };
        let record: CommentRecord =
            serde_json::from_slice(blob.content()).map_err(CommentsError::Malformed)?;
        out.push((id.to_string(), record));
    }
    Ok(out)
}

/// What became of one comment once its anchor was looked for in the file it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorStatus {
    Anchored {
        start: usize,
        end: usize,
    },
    Orphaned(OrphanReason),
    /// The comment's own file is not among the files `read_file` could read — deleted, renamed,
    /// or never there in whatever view called this (a comment is never dropped for this; it is
    /// still returned, so a list of orphans can say which file it was on).
    FileMissing,
}

/// A comment together with where (or whether) its anchor still holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedComment {
    pub id: String,
    pub record: CommentRecord,
    pub status: AnchorStatus,
}

/// Every comment on [`COMMENTS_REF`], reanchored against the files' current text.
///
/// `read_file` takes the comment's project-relative path and returns that file's current
/// contents, or `None` if it has no such file — a plain closure rather than a project directory,
/// so this crate never touches a filesystem itself and a test never needs one on disk.
pub fn load(
    repository: &Repository,
    read_file: impl Fn(&str) -> Option<String>,
) -> Result<Vec<LoadedComment>, CommentsError> {
    load_all(repository)?
        .into_iter()
        .map(|(id, record)| {
            let status = match read_file(&record.file) {
                None => AnchorStatus::FileMissing,
                Some(text) => match anchor::reanchor(&text, &record.anchor) {
                    Reanchored::Found { start, end } => AnchorStatus::Anchored { start, end },
                    Reanchored::Orphaned(reason) => AnchorStatus::Orphaned(reason),
                },
            };
            Ok(LoadedComment { id, record, status })
        })
        .collect()
}

/// Push [`COMMENTS_REF`] to `remote_name`, by exactly that name on both ends — the explicit
/// refspec the design interview's answer calls for, so *Sync* never sends this ref as a side
/// effect of pushing the author's branch, and never sends the branch as a side effect of this.
pub fn push(repository: &Repository, remote_name: &str) -> Result<(), CommentsError> {
    let mut remote = repository
        .find_remote(remote_name)
        .map_err(CommentsError::Remote)?;
    let refspec = format!("{COMMENTS_REF}:{COMMENTS_REF}");
    remote
        .push(&[refspec.as_str()], None)
        .map_err(CommentsError::Remote)
}

/// Where a fetched comments ref lands before [`merge_from`] folds it in — our own remote-tracking
/// namespace, never [`COMMENTS_REF`] itself, so a fetch can never discard a local comment the
/// merge step was supposed to combine with.
fn tracking_ref(remote_name: &str) -> String {
    format!("refs/remotes/{remote_name}/abstract-tex/comments")
}

/// Fetch `remote_name`'s comments ref into our tracking namespace. Call [`merge_from`] afterwards
/// to actually combine it with what is on [`COMMENTS_REF`] here.
pub fn fetch(repository: &Repository, remote_name: &str) -> Result<(), CommentsError> {
    let mut remote = repository
        .find_remote(remote_name)
        .map_err(CommentsError::Remote)?;
    let refspec = format!("{COMMENTS_REF}:{}", tracking_ref(remote_name));
    remote
        .fetch(&[refspec.as_str()], None, None)
        .map_err(CommentsError::Remote)
}

/// What combining a fetched comments ref with the local one did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Nothing to combine: either nothing has been fetched, or the two refs already agree.
    NothingToMerge,
    /// The local ref had no comments of its own yet, or the fetched history is a straight
    /// continuation of it — no merge commit was needed.
    FastForwarded,
    /// Both sides had comments the other did not; a merge commit joins them. In every case this
    /// spike's tests cover, this is reached with zero conflicts — see the module doc.
    Merged,
    /// The merge could not resolve on its own. Listed by path so a caller can say which comments
    /// are in question; this spike never produces one (again, see the module doc), but a future
    /// "resolved" event could, and the caller must have somewhere to put the answer rather than
    /// a panic.
    Conflicted(Vec<String>),
}

/// Combine `remote_name`'s most recently [`fetch`]ed comments with [`COMMENTS_REF`] here.
pub fn merge_from(repository: &Repository, remote_name: &str) -> Result<MergeOutcome, CommentsError> {
    let Ok(tracking) = repository.find_reference(&tracking_ref(remote_name)) else {
        return Ok(MergeOutcome::NothingToMerge);
    };
    let theirs = tracking.peel_to_commit().map_err(CommentsError::Read)?;

    let Some(ours) = current_commit(repository)? else {
        set_ref(repository, theirs.id())?;
        return Ok(MergeOutcome::FastForwarded);
    };
    if ours.id() == theirs.id() {
        return Ok(MergeOutcome::NothingToMerge);
    }
    if is_descendant(repository, theirs.id(), ours.id())? {
        set_ref(repository, theirs.id())?;
        return Ok(MergeOutcome::FastForwarded);
    }
    if is_descendant(repository, ours.id(), theirs.id())? {
        return Ok(MergeOutcome::NothingToMerge); // we are already ahead of what they sent
    }

    merge_divergent(repository, &ours, &theirs)
}

/// `descendant` is a child of `ancestor``, one or more generations down.
fn is_descendant(repository: &Repository, descendant: Oid, ancestor: Oid) -> Result<bool, CommentsError> {
    repository
        .graph_descendant_of(descendant, ancestor)
        .map_err(CommentsError::Read)
}

/// The two histories share no fast-forward relationship: both added comments the other does not
/// have. Diffed against their common ancestor if they have one, or an empty tree if they do not
/// (the case of two authors who each commented before either had ever synced) — either way the
/// result is the same kind of 3-way merge Git already does for source files, landing here on
/// content-addressed, never-rewritten blobs where two additions can never collide at one path.
fn merge_divergent(
    repository: &Repository,
    ours: &Commit<'_>,
    theirs: &Commit<'_>,
) -> Result<MergeOutcome, CommentsError> {
    let ancestor = match repository.merge_base(ours.id(), theirs.id()) {
        Ok(base) => repository
            .find_commit(base)
            .map_err(CommentsError::Read)?
            .tree()
            .map_err(CommentsError::Read)?,
        Err(_) => empty_tree(repository)?,
    };
    let our_tree = ours.tree().map_err(CommentsError::Read)?;
    let their_tree = theirs.tree().map_err(CommentsError::Read)?;

    let mut index = repository
        .merge_trees(&ancestor, &our_tree, &their_tree, None)
        .map_err(CommentsError::Write)?;

    if index.has_conflicts() {
        let paths = conflicted_paths(&index)?;
        return Ok(MergeOutcome::Conflicted(paths));
    }

    let tree_id = index.write_tree_to(repository).map_err(CommentsError::Write)?;
    let tree = repository.find_tree(tree_id).map_err(CommentsError::Read)?;
    let who = author(repository);
    let commit_id = repository
        .commit(None, &who, &who, "Merge comments", &tree, &[ours, theirs])
        .map_err(CommentsError::Write)?;
    set_ref(repository, commit_id)?;
    Ok(MergeOutcome::Merged)
}

fn conflicted_paths(index: &git2::Index) -> Result<Vec<String>, CommentsError> {
    let mut paths = BTreeMap::new();
    for conflict in index.conflicts().map_err(CommentsError::Read)?.flatten() {
        for entry in [conflict.ancestor, conflict.our, conflict.their]
            .into_iter()
            .flatten()
        {
            if let Ok(path) = String::from_utf8(entry.path) {
                paths.insert(path, ());
            }
        }
    }
    Ok(paths.into_keys().collect())
}

fn empty_tree(repository: &Repository) -> Result<Tree<'_>, CommentsError> {
    let oid = repository
        .treebuilder(None)
        .map_err(CommentsError::Write)?
        .write()
        .map_err(CommentsError::Write)?;
    repository.find_tree(oid).map_err(CommentsError::Read)
}

fn set_ref(repository: &Repository, target: Oid) -> Result<(), CommentsError> {
    repository
        .reference(COMMENTS_REF, target, true, "merge comments")
        .map(|_| ())
        .map_err(CommentsError::Write)
}

fn current_commit(repository: &Repository) -> Result<Option<Commit<'_>>, CommentsError> {
    match repository.find_reference(COMMENTS_REF) {
        Ok(reference) => Ok(Some(reference.peel_to_commit().map_err(CommentsError::Read)?)),
        Err(_) => Ok(None),
    }
}

/// Same reasoning as `abstract-tex-snapshot::author`: the author's own identity when Git has one
/// configured, a neutral stand-in otherwise, because writing a comment must never be the thing
/// that makes Git ask them to configure an identity (DESIGN.md §2 rule 4).
fn author(repository: &Repository) -> Signature<'static> {
    repository
        .signature()
        .or_else(|_| Signature::now("Abstract-Tex", "comments@abstract-tex.invalid"))
        .expect("a signature with a valid name and e-mail is always constructible")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn repo() -> (tempfile::TempDir, Repository) {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        (tmp, repository)
    }

    fn record(file: &str, quote_in: &str, author_name: &str, body: &str) -> CommentRecord {
        let start = quote_in.find("result").unwrap();
        let end = start + "result".len();
        CommentRecord {
            file: file.to_string(),
            anchor: Anchor::capture(quote_in, start, end).unwrap(),
            author: author_name.to_string(),
            body: body.to_string(),
            created: 0,
        }
    }

    #[test]
    fn a_comment_round_trips_through_the_ref_with_its_anchor_intact() {
        let (_tmp, repository) = repo();
        let text = "the result was significant";
        let id = add(&repository, &record("main.tex", text, "ada", "says who?")).unwrap();

        let files: HashMap<&str, &str> = [("main.tex", text)].into_iter().collect();
        let loaded = load(&repository, |f| files.get(f).map(|s| s.to_string())).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].record.body, "says who?");
        assert_eq!(loaded[0].status, AnchorStatus::Anchored { start: 4, end: 10 });
    }

    #[test]
    fn a_comment_on_a_rewritten_sentence_comes_back_orphaned_not_dropped() {
        let (_tmp, repository) = repo();
        let original = "the result was significant";
        add(&repository, &record("main.tex", original, "ada", "says who?")).unwrap();

        let rewritten: HashMap<&str, &str> = [("main.tex", "a completely different sentence")]
            .into_iter()
            .collect();
        let loaded = load(&repository, |f| rewritten.get(f).map(|s| s.to_string())).unwrap();

        assert_eq!(loaded.len(), 1, "the comment is still listed");
        assert_eq!(
            loaded[0].status,
            AnchorStatus::Orphaned(OrphanReason::TextNotFound)
        );
    }

    #[test]
    fn a_comment_on_a_file_that_no_longer_exists_is_reported_file_missing() {
        let (_tmp, repository) = repo();
        add(
            &repository,
            &record("chapters/one.tex", "the result stands", "ada", "?"),
        )
        .unwrap();
        let loaded = load(&repository, |_| None).unwrap();
        assert_eq!(loaded[0].status, AnchorStatus::FileMissing);
    }

    #[test]
    fn two_comments_from_the_same_author_chain_as_two_commits() {
        let (_tmp, repository) = repo();
        add(&repository, &record("main.tex", "the result one", "ada", "first")).unwrap();
        add(
            &repository,
            &record("main.tex", "the result two", "ada", "second"),
        )
        .unwrap();

        let commit = current_commit(&repository).unwrap().unwrap();
        assert!(
            commit.parent(0).is_ok(),
            "the second commit has the first as parent"
        );
        assert_eq!(load_all(&repository).unwrap().len(), 2);
    }

    /// The spike's central question: two authors, each offline, each add a comment, each push.
    /// Does the second push's fetch-and-merge combine both without a hand-resolved conflict?
    #[test]
    fn two_authors_commenting_offline_at_once_merge_cleanly_over_a_real_remote() {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = format!("file://{}", origin_dir.path().display());

        let (_tmp_a, ada) = repo();
        ada.remote("origin", &origin_url).unwrap();
        let (_tmp_b, bo) = repo();
        bo.remote("origin", &origin_url).unwrap();

        // Both start offline and comment before either has ever synced — so their two histories
        // share no common ancestor at all, the harder of the two cases merge_divergent handles.
        add(
            &ada,
            &record("main.tex", "the result was clear", "ada", "ada's note"),
        )
        .unwrap();
        add(&bo, &record("main.tex", "the result was odd", "bo", "bo's note")).unwrap();

        // Ada syncs first: nothing to merge, a plain push.
        push(&ada, "origin").unwrap();

        // Bo syncs next: fetch, merge, then push the merge back.
        fetch(&bo, "origin").unwrap();
        let outcome = merge_from(&bo, "origin").unwrap();
        assert_eq!(
            outcome,
            MergeOutcome::Merged,
            "two unrelated histories, both additions"
        );
        assert_eq!(
            load_all(&bo).unwrap().len(),
            2,
            "bo now has both comments locally"
        );
        push(&bo, "origin").unwrap();

        // Ada syncs again and picks up bo's merge.
        fetch(&ada, "origin").unwrap();
        let outcome = merge_from(&ada, "origin").unwrap();
        assert_eq!(
            outcome,
            MergeOutcome::FastForwarded,
            "bo's push was a continuation of ada's"
        );
        let mut all = load_all(&ada).unwrap();
        all.sort_by(|a, b| a.1.body.cmp(&b.1.body));
        let bodies: Vec<&str> = all.iter().map(|(_, r)| r.body.as_str()).collect();
        assert_eq!(bodies, vec!["ada's note", "bo's note"]);
    }

    #[test]
    fn fetching_a_remote_with_nothing_on_the_ref_yet_is_not_an_error() {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = format!("file://{}", origin_dir.path().display());
        let (_tmp, ada) = repo();
        ada.remote("origin", &origin_url).unwrap();

        // The remote has no comments ref at all yet: libgit2 simply has nothing matching the
        // refspec to bring across, and leaves our tracking ref unset rather than erroring — the
        // same quiet shape `abstract-tex-git::fetch` already relies on for a brand new remote.
        fetch(&ada, "origin").unwrap();
        assert_eq!(merge_from(&ada, "origin").unwrap(), MergeOutcome::NothingToMerge);
    }
}
