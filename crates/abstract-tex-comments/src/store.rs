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
//! **Resolving a comment is an event, not a rewrite (S14.3a).** Two authors resolving the same
//! thread while offline must merge as two additions, never a conflict — the same requirement
//! [`add`] already meets for the comment itself — so a resolution is its own content-addressed
//! blob, [`ResolutionRecord`], naming the comment it is about rather than replacing any field on
//! it. [`load`] folds every comment's resolutions back together by [`ResolutionRecord::created`],
//! last write wins, so "resolved, then reopened, then resolved again" reads back as resolved
//! without anyone's event being discarded — see `resolved_state` for the tie-break when two
//! events claim the same timestamp.

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

/// The filename suffix a [`ResolutionRecord`] blob is inserted under, so [`load_all`] (comments
/// only) and [`load_resolutions`] (resolutions only) can tell the two kinds of entry apart on the
/// same tree without either having to parse the other's JSON to find out what it is. Checked
/// before the plainer `.json` a comment uses, since `"<hash>.resolve.json"` also ends in `.json`.
const RESOLUTION_SUFFIX: &str = ".resolve.json";

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

/// A resolve-or-reopen event about one comment, named by that comment's own id (the hex of its
/// blob, the same string [`add`] returns) rather than by rewriting the comment itself — see the
/// module doc for why an in-place rewrite would reintroduce the exact merge conflict [`add`]'s own
/// content addressing was built to avoid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionRecord {
    pub comment_id: String,
    pub author: String,
    /// `true` resolves the comment, `false` reopens it. Not an enum: the two are symmetric, and
    /// `resolved_state` only ever needs the latest value by [`ResolutionRecord::created`].
    pub resolved: bool,
    /// Seconds since the Unix epoch — the field `resolved_state` orders events by.
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
    let blob_id = write_blob_on_ref(repository, &json, |blob| format!("{blob}.json"), "Add a comment")?;
    Ok(blob_id.to_string())
}

/// Add one resolve-or-reopen event, the same way [`add`] adds a comment: a new blob, inserted
/// into the same tree [`add`] builds on, under `RESOLUTION_SUFFIX` rather than a bare `.json` so
/// [`load_all`] and [`load_resolutions`] can tell the two apart.
pub fn add_resolution(repository: &Repository, record: &ResolutionRecord) -> Result<String, CommentsError> {
    let json = serde_json::to_vec_pretty(record).map_err(CommentsError::Malformed)?;
    let message = if record.resolved {
        "Resolve a comment"
    } else {
        "Reopen a comment"
    };
    let blob_id = write_blob_on_ref(
        repository,
        &json,
        |blob| format!("{blob}{RESOLUTION_SUFFIX}"),
        message,
    )?;
    Ok(blob_id.to_string())
}

/// Shared write path for [`add`] and [`add_resolution`]: write `bytes` as a blob, insert it into
/// the tree on top of [`COMMENTS_REF`]'s current tip under whatever name `filename` derives from
/// the blob's own id, and commit that tree as the new tip.
fn write_blob_on_ref(
    repository: &Repository,
    bytes: &[u8],
    filename: impl FnOnce(Oid) -> String,
    message: &str,
) -> Result<Oid, CommentsError> {
    let blob = repository.blob(bytes).map_err(CommentsError::Write)?;
    let filename = filename(blob);

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
        .commit(None, &who, &who, message, &tree, &parents)
        .map_err(CommentsError::Write)?;
    repository
        .reference(COMMENTS_REF, commit_id, true, message)
        .map_err(CommentsError::Write)?;
    Ok(blob)
}

/// Every comment currently on [`COMMENTS_REF`], with no attempt to reanchor them — see [`load`]
/// for the version a caller with files on disk actually wants. Resolution events ([`add_resolution`])
/// live on the same tree but are skipped here; [`load_resolutions`] reads those.
pub fn load_all(repository: &Repository) -> Result<Vec<(String, CommentRecord)>, CommentsError> {
    read_tree_entries(repository, |name| {
        // A resolution's own filename also ends in `.json` — check the longer, more specific
        // suffix first so a resolution is never misread as a comment.
        if name.ends_with(RESOLUTION_SUFFIX) {
            None
        } else {
            name.strip_suffix(".json")
        }
    })
}

/// Every resolve-or-reopen event currently on [`COMMENTS_REF`] — see `resolved_state` for
/// folding them, per comment, into a single current answer.
pub fn load_resolutions(repository: &Repository) -> Result<Vec<(String, ResolutionRecord)>, CommentsError> {
    read_tree_entries(repository, |name| name.strip_suffix(RESOLUTION_SUFFIX))
}

/// Shared tree walk for [`load_all`] and [`load_resolutions`]: `id_from_filename` decides which
/// entries belong to this caller (by returning the id part of a matching name) and skips the
/// rest, so the two never need to know how the other's filenames are shaped beyond that.
fn read_tree_entries<T: serde::de::DeserializeOwned>(
    repository: &Repository,
    id_from_filename: impl Fn(&str) -> Option<&str>,
) -> Result<Vec<(String, T)>, CommentsError> {
    let Some(commit) = current_commit(repository)? else {
        return Ok(Vec::new());
    };
    let tree = commit.tree().map_err(CommentsError::Read)?;
    let mut out = Vec::new();
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        let Some(id) = id_from_filename(name) else {
            continue;
        };
        let object = entry.to_object(repository).map_err(CommentsError::Read)?;
        let Some(blob) = object.as_blob() else { continue };
        let record: T = serde_json::from_slice(blob.content()).map_err(CommentsError::Malformed)?;
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

/// A comment together with where (or whether) its anchor still holds, and whether the latest
/// resolution event about it (if any) marked it resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedComment {
    pub id: String,
    pub record: CommentRecord,
    pub status: AnchorStatus,
    pub resolved: bool,
}

/// Every comment on [`COMMENTS_REF`], reanchored against the files' current text, folded together
/// with whatever resolution events exist for it.
///
/// `read_file` takes the comment's project-relative path and returns that file's current
/// contents, or `None` if it has no such file — a plain closure rather than a project directory,
/// so this crate never touches a filesystem itself and a test never needs one on disk.
pub fn load(
    repository: &Repository,
    read_file: impl Fn(&str) -> Option<String>,
) -> Result<Vec<LoadedComment>, CommentsError> {
    let resolutions = load_resolutions(repository)?;
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
            let resolved = resolved_state(&id, &resolutions);
            Ok(LoadedComment {
                id,
                record,
                status,
                resolved,
            })
        })
        .collect()
}

/// Fold every resolution event about `comment_id` down to one answer: the `resolved` value of
/// whichever has the latest [`ResolutionRecord::created`]. A comment with no resolution events at
/// all is unresolved. Ties (two events claiming the same timestamp, which a clock skew between two
/// offline authors can produce) break on the event's own blob id — arbitrary, but, unlike "whichever
/// `load_resolutions` happened to list last," the same answer every time this is called on the same
/// ref, which is the only property a tie-break here needs.
fn resolved_state(comment_id: &str, resolutions: &[(String, ResolutionRecord)]) -> bool {
    resolutions
        .iter()
        .filter(|(_, record)| record.comment_id == comment_id)
        .max_by_key(|(blob_id, record)| (record.created, blob_id.clone()))
        .is_some_and(|(_, record)| record.resolved)
}

/// Record a resolve-or-reopen event for the comment `comment_id` names (its id, as returned by
/// [`add`]) and write it the same way [`add`] writes a comment. Does not check that `comment_id`
/// actually names a comment on this ref — a resolution for an id nobody has fetched yet is exactly
/// what two authors working offline produce, and `resolved_state` already treats an unmatched
/// resolution as simply nothing to fold in.
pub fn resolve(
    repository: &Repository,
    comment_id: &str,
    author: &str,
    resolved: bool,
    created: i64,
) -> Result<String, CommentsError> {
    add_resolution(
        repository,
        &ResolutionRecord {
            comment_id: comment_id.to_string(),
            author: author.to_string(),
            resolved,
            created,
        },
    )
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

    #[test]
    fn a_comment_with_no_resolution_event_loads_as_unresolved() {
        let (_tmp, repository) = repo();
        add(&repository, &record("main.tex", "the result stands", "ada", "?")).unwrap();
        let loaded = load(&repository, |_| None).unwrap();
        assert!(!loaded[0].resolved);
    }

    #[test]
    fn resolving_then_reopening_a_comment_tracks_the_latest_event() {
        let (_tmp, repository) = repo();
        let id = add(&repository, &record("main.tex", "the result stands", "ada", "?")).unwrap();

        resolve(&repository, &id, "bo", true, 10).unwrap();
        let loaded = load(&repository, |_| None).unwrap();
        assert!(loaded[0].resolved, "the resolve event at t=10 should have landed");

        resolve(&repository, &id, "ada", false, 20).unwrap();
        let loaded = load(&repository, |_| None).unwrap();
        assert!(
            !loaded[0].resolved,
            "the later reopen at t=20 should win over t=10"
        );
    }

    #[test]
    fn an_older_resolution_never_overrides_a_newer_one_regardless_of_write_order() {
        let (_tmp, repository) = repo();
        let id = add(&repository, &record("main.tex", "the result stands", "ada", "?")).unwrap();

        // Written in the opposite order from the previous test: the reopen (t=5) lands first,
        // the resolve (t=15) second. `resolved_state` must still order by `created`, not by
        // which event the tree happened to list, or which was written to the ref first.
        resolve(&repository, &id, "ada", false, 5).unwrap();
        resolve(&repository, &id, "bo", true, 15).unwrap();
        let loaded = load(&repository, |_| None).unwrap();
        assert!(loaded[0].resolved, "t=15's resolve is later than t=5's reopen");
    }

    #[test]
    fn two_authors_resolving_the_same_comment_offline_merge_as_two_additions_not_a_conflict() {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = format!("file://{}", origin_dir.path().display());

        let (_tmp_a, ada) = repo();
        ada.remote("origin", &origin_url).unwrap();
        let id = add(&ada, &record("main.tex", "the result stands", "ada", "?")).unwrap();
        push(&ada, "origin").unwrap();

        let (_tmp_b, bo) = repo();
        bo.remote("origin", &origin_url).unwrap();
        fetch(&bo, "origin").unwrap();
        assert_eq!(merge_from(&bo, "origin").unwrap(), MergeOutcome::FastForwarded);

        // Both authors resolve the same comment offline, each unaware of the other's event —
        // the resolution-side equivalent of the existing comment merge test above. Each event is
        // its own blob (different author, so different content), so the tree merge sees two
        // additions, exactly as it does for two different comments.
        resolve(&ada, &id, "ada", true, 100).unwrap();
        resolve(&bo, &id, "bo", true, 100).unwrap();

        push(&ada, "origin").unwrap();
        fetch(&bo, "origin").unwrap();
        let outcome = merge_from(&bo, "origin").unwrap();
        assert_eq!(outcome, MergeOutcome::Merged, "two additions, not a conflict");

        let resolutions = load_resolutions(&bo).unwrap();
        assert_eq!(resolutions.len(), 2, "both resolution events survived the merge");
        let loaded = load(&bo, |_| None).unwrap();
        assert!(loaded[0].resolved);
    }

    #[test]
    fn a_resolution_for_an_unknown_comment_id_is_harmless() {
        let (_tmp, repository) = repo();
        add(&repository, &record("main.tex", "the result stands", "ada", "?")).unwrap();
        resolve(
            &repository,
            "0000000000000000000000000000000000000000",
            "ada",
            true,
            1,
        )
        .unwrap();

        let loaded = load(&repository, |_| None).unwrap();
        assert_eq!(
            loaded.len(),
            1,
            "the unrelated resolution did not create a phantom comment"
        );
        assert!(!loaded[0].resolved, "it was never about this comment");
    }
}
