//! S10.1: a snapshot of the project's files on every successful compile, written as real Git
//! objects on a ref nothing else looks at.
//!
//! This crate owns one promise from DESIGN.md §5.7: *there is always a recoverable state, even
//! from an author who has never once pressed commit*. It keeps that promise with ordinary Git —
//! §4's storage table rejects "a snapshot format" by name, because a format locks the history
//! inside this app, and the whole value of a snapshot is that recovering it needs `git` and not
//! us.
//!
//! **What it must never do**, which matters more here than what it does:
//!
//! - Never touch the author's index, `HEAD`, working tree or any branch. A snapshot that changed
//!   what `git status` says would make this app an uninvited co-author of someone else's history.
//! - Never fail a build. The caller takes a snapshot *after* the compile has been reported; an
//!   error here is logged and swallowed (`compile.rs`), never raised at the author.
//! - Never write into the source tree except where Git already writes — a project that is already
//!   a repository gets a ref in its own `.git/`, and a project that is not gets a bare repository
//!   under `.abstract-tex/`, which DESIGN.md §5.8 reserves for us.
//!
//! Errors are a `thiserror` enum, the split `crates/abstract-tex-engine/src/lib.rs` explains
//! once: typed errors in a library, `anyhow` only at the app edge.

use std::path::{Path, PathBuf};

use git2::{Commit, Oid, Repository, Signature, TreeBuilder};
use tracing::debug;

/// Where snapshots are kept inside a repository the author already has.
///
/// A ref outside `refs/heads/` and `refs/tags/` is invisible to `git branch`, `git tag`, `git
/// log` and `git status`, and is not pushed by a default `git push` — which is exactly the
/// "hidden ref" DESIGN.md §5.7 asks for. It is still an ordinary ref, so `git log
/// refs/abstract-tex/snapshots` and `git show refs/abstract-tex/snapshots:main.tex` recover it
/// with no tooling from us at all.
pub const SNAPSHOT_REF: &str = "refs/abstract-tex/snapshots";

/// Where snapshots are kept for a project that is not a Git repository: a bare repository of our
/// own, under the folder DESIGN.md §5.8 already gives us.
const OWN_REPOSITORY: &str = ".abstract-tex/snapshots.git";

/// Folder names never walked into.
///
/// Deliberately the same list `IGNORED_DIRS` uses for the file tree in `src-tauri/src/project.rs`, so
/// that what a snapshot contains is what the author sees in the sidebar — the useful definition
/// of "the project". It cannot be *shared* with that list, because the app crate depends on this
/// one and not the other way round; if it grows a third copy, it should become a crate.
///
/// `.git` because the author's history is not part of their manuscript, and `.abstract-tex`
/// because it is ours: build artifacts, the warm marker, and — for a project with no Git of its
/// own — the snapshot repository itself, which must not contain a copy of every version of
/// itself. The rest are other tools' folders, which `.gitignore` would usually catch, except in
/// the case this loop exists for: a project with no Git and so no `.gitignore` at all.
const NEVER_WALKED: [&str; 6] = [
    ".git",
    ".abstract-tex",
    ".preamble",
    "node_modules",
    ".svn",
    ".hg",
];

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("the snapshot repository could not be opened or created: {0}")]
    Repository(#[source] git2::Error),

    #[error("the project's files could not be read: {0}")]
    Read(#[source] std::io::Error),

    #[error("the snapshot could not be written: {0}")]
    Write(#[source] git2::Error),

    #[error("a snapshot could not be read back: {0}")]
    ReadBack(#[source] git2::Error),

    /// S11.6: an id that is not on [`SNAPSHOT_REF`]. In a project that is also the author's own
    /// repository, "any commit" would be anything they ever committed, whose tree is rooted at the
    /// repository and not at this folder — so a recovery path must only ever name a snapshot.
    #[error("that is not one of this project's snapshots")]
    NotASnapshot,
}

/// What one call to [`snapshot`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snapshot {
    /// A new commit on [`SNAPSHOT_REF`], with the previous snapshot as its parent.
    Took(Oid),
    /// Nothing was written: this compile's files are byte-for-byte the previous snapshot's. A
    /// compile that changed nothing is not a version, and an empty commit every few seconds
    /// would bury the ones that mean something.
    Unchanged,
}

/// Snapshot every file of `project_dir` onto [`SNAPSHOT_REF`].
///
/// Called after a successful compile and after the app has already told the frontend the build
/// finished, so nothing here is on the latency path §2 rule 2 protects.
pub fn snapshot(project_dir: &Path) -> Result<Snapshot, SnapshotError> {
    let repository = open_or_create(project_dir)?;
    let tree = build_tree(&repository, project_dir)?;

    // `find_reference` rather than `head`: the author's `HEAD` is theirs, and the parent of a
    // snapshot is always the snapshot before it, whatever branch they happen to be on.
    let parent = repository
        .find_reference(SNAPSHOT_REF)
        .ok()
        .and_then(|reference| reference.peel_to_commit().ok());

    if parent.as_ref().is_some_and(|commit| commit.tree_id() == tree) {
        debug!("nothing changed since the last snapshot");
        return Ok(Snapshot::Unchanged);
    }

    let commit = write_commit(&repository, tree, parent.as_ref())?;
    debug!(%commit, "snapshot taken");
    Ok(Snapshot::Took(commit))
}

/// The repository a snapshot goes into: the author's, if they have one, or ours.
///
/// `discover` and not `open`, because a project folder is often a subfolder of the repository —
/// a thesis inside a monorepo, a paper inside a folder of papers — and its history belongs
/// wherever the author put it. `discover` walks up until it finds a `.git`, which is what every
/// Git command does.
fn open_or_create(project_dir: &Path) -> Result<Repository, SnapshotError> {
    if let Ok(theirs) = Repository::discover(project_dir) {
        return Ok(theirs);
    }
    // No repository anywhere above this folder: keep our own, bare, out of the source tree. Bare
    // because it has no working tree of its own — the project folder *is* the working tree, and
    // we only ever write objects, never check anything out.
    let ours = project_dir.join(OWN_REPOSITORY);
    match Repository::open_bare(&ours) {
        Ok(repository) => Ok(repository),
        Err(_) => Repository::init_bare(&ours).map_err(SnapshotError::Repository),
    }
}

/// The repository a snapshot would be *read* from, without making one.
///
/// Separate from [`open_or_create`] because a read must not have a side effect: asking "is there
/// anything to recover?" about a project that has never been compiled would otherwise leave a
/// repository behind, and `.abstract-tex/snapshots.git` appearing without a snapshot in it is a
/// small lie about what the app has kept.
fn open_existing(project_dir: &Path) -> Option<Repository> {
    Repository::discover(project_dir)
        .or_else(|_| Repository::open_bare(project_dir.join(OWN_REPOSITORY)))
        .ok()
}

/// Every file under `project_dir`, as a Git tree, written straight into the object database.
///
/// Straight in, and never through `repository.index()`: the index *is* what `git status` reads,
/// so staging the project's files to build a tree from them would silently stage the author's
/// work-in-progress as a side effect of pressing compile. [`TreeBuilder`] writes the same objects
/// with nothing observable in between.
fn build_tree(repository: &Repository, project_dir: &Path) -> Result<Oid, SnapshotError> {
    tree_for_folder(repository, project_dir)
}

/// One folder, recursively.
fn tree_for_folder(repository: &Repository, folder: &Path) -> Result<Oid, SnapshotError> {
    let mut builder = repository.treebuilder(None).map_err(SnapshotError::Write)?;

    // Sorted, because `read_dir` order is the filesystem's and a tree that depends on it would
    // hash differently on two machines with the same files — which would turn every first
    // snapshot after a clone into a spurious "everything changed".
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(SnapshotError::Read)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();

    for path in entries {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue; // a name this platform cannot spell in UTF-8 is not something Git can store
        };
        if path.is_dir() {
            if NEVER_WALKED.contains(&name) || is_ignored(repository, &path) {
                continue;
            }
            let sub = tree_for_folder(repository, &path)?;
            // An empty folder has no Git representation at all, and adding one produces a tree
            // that `git` itself would never write. Skipping keeps our trees ordinary.
            if !is_empty_tree(repository, sub) {
                insert(&mut builder, name, sub, git2::FileMode::Tree)?;
            }
        } else if path.is_file() && !is_ignored(repository, &path) {
            let blob = repository.blob_path(&path).map_err(SnapshotError::Write)?;
            let mode = if is_executable(&path) {
                git2::FileMode::BlobExecutable
            } else {
                git2::FileMode::Blob
            };
            insert(&mut builder, name, blob, mode)?;
        }
        // Anything else — a symlink, a socket, a device — is skipped rather than followed: a
        // symlink out of the project could pull an unbounded amount of somebody else's disk into
        // the manuscript's history.
    }
    builder.write().map_err(SnapshotError::Write)
}

fn insert(
    builder: &mut TreeBuilder<'_>,
    name: &str,
    id: Oid,
    mode: git2::FileMode,
) -> Result<(), SnapshotError> {
    builder
        .insert(name, id, mode.into())
        .map(|_| ())
        .map_err(SnapshotError::Write)
}

fn is_empty_tree(repository: &Repository, id: Oid) -> bool {
    repository
        .find_tree(id)
        .map(|tree| tree.is_empty())
        .unwrap_or(true)
}

/// Does the author's own `.gitignore` already say this file is not part of the project?
///
/// Asked so that build junk, `node_modules` and an editor's scratch files stay out of a history
/// nobody asked for. `is_path_ignored` answers from the repository's ignore rules and never
/// reads the index, so it is safe to call here. A bare repository of our own has no ignore rules
/// and no working directory to resolve them against, so it answers `false` for everything, which
/// is the right default: [`NEVER_WALKED`] has already removed what we know does not belong.
fn is_ignored(repository: &Repository, path: &Path) -> bool {
    if repository.workdir().is_none() {
        return false;
    }
    repository.is_path_ignored(path).unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Windows has no executable bit, and Git records `100644` for every file checked out there.
#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

/// Write the commit and move the hidden ref to it — in that order, and touching nothing else.
///
/// `commit` is called with `None` for the ref to update, which is what keeps `HEAD` and the
/// current branch out of this: it writes the object and returns its id, and then the ref is set
/// on its own. Doing it the other way round (`Some("HEAD")`, the usual spelling) would move the
/// author's branch to a commit they never made.
fn write_commit(
    repository: &Repository,
    tree: Oid,
    parent: Option<&Commit<'_>>,
) -> Result<Oid, SnapshotError> {
    let tree = repository.find_tree(tree).map_err(SnapshotError::Write)?;
    let who = author(repository);
    let parents: Vec<&Commit<'_>> = parent.into_iter().collect();

    let id = repository
        .commit(None, &who, &who, "Snapshot on compile", &tree, &parents)
        .map_err(SnapshotError::Write)?;

    repository
        .reference(SNAPSHOT_REF, id, true, "snapshot on compile")
        .map_err(SnapshotError::Write)?;
    Ok(id)
}

/// Who a snapshot is by. The author's own `user.name`/`user.email` when Git has them, so a
/// recovered snapshot is attributed the way the rest of their history is; a neutral stand-in
/// otherwise, because a snapshot must never be the thing that makes Git ask them to configure
/// an identity (DESIGN.md §2 rule 4: zero setup).
fn author(repository: &Repository) -> Signature<'static> {
    repository
        .signature()
        .or_else(|_| Signature::now("Abstract-Tex", "snapshot@abstract-tex.invalid"))
        .expect("a signature with a valid name and e-mail is always constructible")
}

/// Read one file out of the most recent snapshot, by its project-relative path. This is the
/// recovery path in one function — it exists so the tests can prove a snapshot is readable, and
/// so the app has something to offer before S10.3's view exists.
pub fn read_from_latest(project_dir: &Path, file: &Path) -> Result<Option<Vec<u8>>, SnapshotError> {
    let Some(repository) = open_existing(project_dir) else {
        return Ok(None);
    };
    let Ok(reference) = repository.find_reference(SNAPSHOT_REF) else {
        return Ok(None);
    };
    let commit = reference.peel_to_commit().map_err(SnapshotError::ReadBack)?;
    let Ok(entry) = commit.tree().map_err(SnapshotError::ReadBack)?.get_path(file) else {
        return Ok(None);
    };
    let object = entry.to_object(&repository).map_err(SnapshotError::ReadBack)?;
    Ok(object.as_blob().map(|blob| blob.content().to_vec()))
}

// ---------------------------------------------------------------------------------------------
// S11.6: reading snapshots back, for the Snapshots list.
// ---------------------------------------------------------------------------------------------

/// One snapshot, as the Snapshots list shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRow {
    pub id: String,
    /// The first seven characters, as `git log --oneline` shows them.
    pub short_id: String,
    /// Seconds since the Unix epoch, UTC; the frontend phrases it, as it does for the Graph.
    pub time: i64,
    /// Words of prose in the `.tex` files of that version, so the list reads as a progress log:
    /// the author recognises "the one with 4,200 words" long before a time.
    pub words: usize,
}

/// The snapshots, newest first, at most `limit` of them. Empty for a project that has never been
/// compiled — and, like [`read_from_latest`], leaves nothing behind for having asked.
pub fn list(project_dir: &Path, limit: usize) -> Result<Vec<SnapshotRow>, SnapshotError> {
    let Some(repository) = open_existing(project_dir) else {
        return Ok(Vec::new());
    };
    if repository.find_reference(SNAPSHOT_REF).is_err() {
        return Ok(Vec::new());
    }
    let mut walk = repository.revwalk().map_err(SnapshotError::ReadBack)?;
    walk.push_ref(SNAPSHOT_REF).map_err(SnapshotError::ReadBack)?;
    // Newest first by ancestry, not by clock: two compiles in one second have the same timestamp.
    walk.set_sorting(git2::Sort::TOPOLOGICAL)
        .map_err(SnapshotError::ReadBack)?;

    let mut rows = Vec::new();
    for id in walk.take(limit) {
        let commit = repository
            .find_commit(id.map_err(SnapshotError::ReadBack)?)
            .map_err(SnapshotError::ReadBack)?;
        rows.push(SnapshotRow {
            id: commit.id().to_string(),
            short_id: commit.id().to_string().chars().take(7).collect(),
            time: commit.time().seconds(),
            words: prose_words(&repository, &commit)?,
        });
    }
    Ok(rows)
}

/// Every file of one snapshot, project-relative with forward slashes, sorted.
pub fn files(project_dir: &Path, id: &str) -> Result<Vec<String>, SnapshotError> {
    let repository = open_existing(project_dir).ok_or(SnapshotError::NotASnapshot)?;
    let commit = snapshot_commit(&repository, id)?;
    let mut paths = Vec::new();
    commit
        .tree()
        .and_then(|tree| {
            tree.walk(git2::TreeWalkMode::PreOrder, |folder, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    if let Some(name) = entry.name() {
                        paths.push(format!("{folder}{name}"));
                    }
                }
                git2::TreeWalkResult::Ok
            })
        })
        .map_err(SnapshotError::ReadBack)?;
    paths.sort();
    Ok(paths)
}

/// One file as it was in one snapshot, or `None` if that snapshot did not have it.
pub fn read(project_dir: &Path, id: &str, file: &str) -> Result<Option<Vec<u8>>, SnapshotError> {
    let repository = open_existing(project_dir).ok_or(SnapshotError::NotASnapshot)?;
    let commit = snapshot_commit(&repository, id)?;
    let Ok(entry) = commit
        .tree()
        .map_err(SnapshotError::ReadBack)?
        .get_path(Path::new(file))
    else {
        return Ok(None);
    };
    let object = entry.to_object(&repository).map_err(SnapshotError::ReadBack)?;
    Ok(object.as_blob().map(|blob| blob.content().to_vec()))
}

/// The commit `id` names, but only if it is on [`SNAPSHOT_REF`] — see [`SnapshotError::NotASnapshot`].
fn snapshot_commit<'repo>(repository: &'repo Repository, id: &str) -> Result<Commit<'repo>, SnapshotError> {
    let wanted = Oid::from_str(id).map_err(|_| SnapshotError::NotASnapshot)?;
    let mut walk = repository.revwalk().map_err(SnapshotError::ReadBack)?;
    walk.push_ref(SNAPSHOT_REF)
        .map_err(|_| SnapshotError::NotASnapshot)?;
    if !walk.flatten().any(|found| found == wanted) {
        return Err(SnapshotError::NotASnapshot);
    }
    repository.find_commit(wanted).map_err(SnapshotError::ReadBack)
}

/// Words of prose across every `.tex` file in a snapshot's tree.
fn prose_words(repository: &Repository, commit: &Commit<'_>) -> Result<usize, SnapshotError> {
    let mut words = 0;
    commit
        .tree()
        .and_then(|tree| {
            tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob)
                    && entry.name().is_some_and(|name| name.ends_with(".tex"))
                {
                    if let Ok(blob) = repository.find_blob(entry.id()) {
                        // Not UTF-8 is not prose; a lossy read would count the replacement
                        // characters' neighbours as words.
                        if let Ok(text) = std::str::from_utf8(blob.content()) {
                            words += texwords::count_prose(text);
                        }
                    }
                }
                git2::TreeWalkResult::Ok
            })
        })
        .map_err(SnapshotError::ReadBack)?;
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A project folder with `main.tex` in it, and nothing else.
    fn project(contents: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.tex"), contents).unwrap();
        tmp
    }

    /// The snapshot ref's history, newest first.
    fn history(project_dir: &Path) -> Vec<Oid> {
        let repository = open_or_create(project_dir).unwrap();
        let mut walk = repository.revwalk().unwrap();
        walk.push_ref(SNAPSHOT_REF).unwrap();
        walk.map(|id| id.unwrap()).collect()
    }

    /// The card's first done-when: an author who never pressed commit still gets a history, and
    /// plain Git reads it.
    #[test]
    fn three_compiles_of_a_project_with_no_git_leave_a_three_commit_history() {
        let tmp = project("first");
        assert!(matches!(snapshot(tmp.path()).unwrap(), Snapshot::Took(_)));
        fs::write(tmp.path().join("main.tex"), "second").unwrap();
        assert!(matches!(snapshot(tmp.path()).unwrap(), Snapshot::Took(_)));
        fs::write(tmp.path().join("main.tex"), "third").unwrap();
        assert!(matches!(snapshot(tmp.path()).unwrap(), Snapshot::Took(_)));

        assert_eq!(
            history(tmp.path()).len(),
            3,
            "each compile is one commit, chained to the last"
        );
        // And the manuscript is in there, not just the commits.
        let recovered = read_from_latest(tmp.path(), Path::new("main.tex")).unwrap();
        assert_eq!(recovered.as_deref(), Some(b"third".as_slice()));
        assert!(
            tmp.path().join(OWN_REPOSITORY).is_dir(),
            "our repository lives under .abstract-tex/"
        );
    }

    /// The card's third done-when. A compile that changed nothing is not a version, and an empty
    /// commit every few seconds would bury the ones that mean something.
    #[test]
    fn a_compile_that_changed_nothing_adds_no_commit() {
        let tmp = project("unchanged");
        snapshot(tmp.path()).unwrap();
        assert_eq!(snapshot(tmp.path()).unwrap(), Snapshot::Unchanged);
        assert_eq!(history(tmp.path()).len(), 1);
    }

    /// The card's second done-when, and the one this crate's module doc calls more important than
    /// what it does: the author's own Git is left exactly as it was.
    #[test]
    fn a_snapshot_never_touches_the_authors_index_head_or_branches() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        let who = Signature::now("Ada", "ada@example.invalid").unwrap();

        // A first commit, so there is a `HEAD` and a branch to disturb.
        fs::write(tmp.path().join("main.tex"), "committed").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("main.tex")).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        repository
            .commit(Some("HEAD"), &who, &who, "first", &tree, &[])
            .unwrap();

        // Then the state an author is actually in when they press compile: something staged,
        // something else edited and not staged.
        fs::write(tmp.path().join("staged.tex"), "staged").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("staged.tex")).unwrap();
        index.write().unwrap();
        fs::write(tmp.path().join("main.tex"), "edited, not staged").unwrap();

        let status_before = statuses(&repository);
        let head_before = repository.head().unwrap().target();
        let branches_before = branch_names(&repository);

        assert!(matches!(snapshot(tmp.path()).unwrap(), Snapshot::Took(_)));

        assert_eq!(
            statuses(&repository),
            status_before,
            "the snapshot changed what `git status` says"
        );
        assert_eq!(
            repository.head().unwrap().target(),
            head_before,
            "the snapshot moved HEAD"
        );
        assert_eq!(
            branch_names(&repository),
            branches_before,
            "the snapshot added or moved a branch"
        );
        // And it went into the author's own repository, not a second one beside it.
        assert!(!tmp.path().join(OWN_REPOSITORY).exists());
        assert_eq!(history(tmp.path()).len(), 1);
    }

    /// The other half of "hidden": a ref outside `refs/heads` is not a branch, so nothing that
    /// lists branches — `git branch`, the Source Control view coming in S10.3 — will show it.
    #[test]
    fn the_snapshot_ref_is_not_a_branch() {
        let tmp = project("hidden");
        snapshot(tmp.path()).unwrap();
        let repository = open_or_create(tmp.path()).unwrap();
        assert!(branch_names(&repository).is_empty());
        assert!(
            repository.find_reference(SNAPSHOT_REF).is_ok(),
            "but the ref is there to recover from"
        );
    }

    /// Our own folder never enters a snapshot, and neither does anything the author's `.gitignore`
    /// already says is not part of the project.
    #[test]
    fn build_junk_and_ignored_files_stay_out() {
        let tmp = tempfile::tempdir().unwrap();
        Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "manuscript").unwrap();
        fs::write(tmp.path().join(".gitignore"), "*.draft\n").unwrap();
        fs::write(tmp.path().join("notes.draft"), "ignored").unwrap();
        fs::create_dir_all(tmp.path().join(".abstract-tex/build")).unwrap();
        fs::write(tmp.path().join(".abstract-tex/build/main.pdf"), "junk").unwrap();

        snapshot(tmp.path()).unwrap();

        assert!(read_from_latest(tmp.path(), Path::new("main.tex"))
            .unwrap()
            .is_some());
        assert!(
            read_from_latest(tmp.path(), Path::new("notes.draft"))
                .unwrap()
                .is_none(),
            "an ignored file"
        );
        assert!(
            read_from_latest(tmp.path(), Path::new(".abstract-tex/build/main.pdf"))
                .unwrap()
                .is_none(),
            "build junk"
        );
        // `.gitignore` itself is the author's file and belongs in the snapshot.
        assert!(read_from_latest(tmp.path(), Path::new(".gitignore"))
            .unwrap()
            .is_some());
    }

    /// Two projects with the same files must hash to the same tree whatever order the filesystem
    /// hands them back in — otherwise the first snapshot after a clone would report that
    /// everything had changed.
    #[test]
    fn the_tree_does_not_depend_on_the_order_the_filesystem_lists_files_in() {
        let one = tempfile::tempdir().unwrap();
        for name in ["a.tex", "b.tex", "c.tex"] {
            fs::write(one.path().join(name), name).unwrap();
        }
        let other = tempfile::tempdir().unwrap();
        for name in ["c.tex", "b.tex", "a.tex"] {
            fs::write(other.path().join(name), name).unwrap();
        }

        let tree_of = |dir: &Path| {
            let repository = open_or_create(dir).unwrap();
            build_tree(&repository, dir).unwrap()
        };
        assert_eq!(tree_of(one.path()), tree_of(other.path()));
    }

    /// Asking what has been kept must not create the place it would have been kept in.
    #[test]
    fn reading_from_a_project_that_was_never_compiled_leaves_nothing_behind() {
        let tmp = project("never compiled");
        assert_eq!(read_from_latest(tmp.path(), Path::new("main.tex")).unwrap(), None);
        assert!(
            !tmp.path().join(".abstract-tex").exists(),
            "a read created a repository"
        );
    }

    /// The folders the file tree hides are the folders a snapshot skips, even with no `.gitignore`
    /// to say so — which is the case this whole loop exists for.
    #[test]
    fn a_project_with_no_git_at_all_still_skips_the_folders_that_are_not_the_manuscript() {
        let tmp = project("manuscript");
        for folder in NEVER_WALKED {
            std::fs::create_dir_all(tmp.path().join(folder)).unwrap();
            std::fs::write(tmp.path().join(folder).join("junk.txt"), "not the manuscript").unwrap();
        }
        snapshot(tmp.path()).unwrap();

        let repository = open_or_create(tmp.path()).unwrap();
        let tree = repository
            .find_reference(SNAPSHOT_REF)
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .tree()
            .unwrap();
        assert_eq!(
            tree.len(),
            1,
            "only main.tex: {:?}",
            tree.iter()
                .map(|e| e.name().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    /// A folder with nothing in it is not something Git can represent, and inventing a
    /// representation for it would make our trees unlike every other tree in the repository.
    #[test]
    fn an_empty_folder_is_not_in_the_tree() {
        let tmp = project("manuscript");
        fs::create_dir(tmp.path().join("figures")).unwrap();
        snapshot(tmp.path()).unwrap();

        let repository = open_or_create(tmp.path()).unwrap();
        let tree = repository
            .find_reference(SNAPSHOT_REF)
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .tree()
            .unwrap();
        assert_eq!(tree.len(), 1, "only main.tex");
    }

    fn statuses(repository: &Repository) -> Vec<(String, git2::Status)> {
        repository
            .statuses(None)
            .unwrap()
            .iter()
            .map(|entry| (entry.path().unwrap_or_default().to_string(), entry.status()))
            .collect()
    }

    fn branch_names(repository: &Repository) -> Vec<String> {
        repository
            .branches(None)
            .unwrap()
            .filter_map(|branch| branch.ok())
            .filter_map(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // S11.6: reading snapshots back.
    // -----------------------------------------------------------------------------------------

    /// Three snapshots of `main.tex`, newest last, and their ids oldest first.
    fn three_versions() -> (tempfile::TempDir, Vec<String>) {
        let tmp = project("one two three");
        let mut ids = Vec::new();
        for text in [
            "one two three",
            "one two three four five",
            "one two three four five six seven",
        ] {
            fs::write(tmp.path().join("main.tex"), text).unwrap();
            let Snapshot::Took(id) = snapshot(tmp.path()).unwrap() else {
                panic!("expected a snapshot")
            };
            ids.push(id.to_string());
        }
        (tmp, ids)
    }

    #[test]
    fn the_list_is_newest_first_with_each_versions_word_count() {
        let (tmp, ids) = three_versions();

        let rows = list(tmp.path(), 10).unwrap();

        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            [ids[2].as_str(), ids[1].as_str(), ids[0].as_str()]
        );
        assert_eq!(rows.iter().map(|r| r.words).collect::<Vec<_>>(), [7, 5, 3]);
        assert_eq!(rows[0].short_id, ids[2][..7]);
        assert!(rows[0].time > 0);
    }

    #[test]
    fn the_list_honours_its_limit() {
        let (tmp, ids) = three_versions();
        let rows = list(tmp.path(), 2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, ids[2]);
    }

    #[test]
    fn a_project_never_compiled_has_no_snapshots_and_asking_leaves_nothing_behind() {
        let tmp = project("never built");
        assert!(list(tmp.path(), 10).unwrap().is_empty());
        assert!(!tmp.path().join(OWN_REPOSITORY).exists());
    }

    #[test]
    fn an_older_version_of_a_file_reads_back_as_it_was() {
        let (tmp, ids) = three_versions();
        assert_eq!(
            read(tmp.path(), &ids[0], "main.tex").unwrap().unwrap(),
            b"one two three"
        );
        assert_eq!(
            read(tmp.path(), &ids[1], "main.tex").unwrap().unwrap(),
            b"one two three four five"
        );
        assert_eq!(read(tmp.path(), &ids[0], "no-such.tex").unwrap(), None);
    }

    #[test]
    fn files_lists_every_path_of_a_snapshot_with_folders_spelled_out() {
        let tmp = project("x");
        fs::create_dir(tmp.path().join("chapters")).unwrap();
        fs::write(tmp.path().join("chapters").join("intro.tex"), "intro").unwrap();
        let Snapshot::Took(id) = snapshot(tmp.path()).unwrap() else {
            panic!()
        };

        assert_eq!(
            files(tmp.path(), &id.to_string()).unwrap(),
            ["chapters/intro.tex", "main.tex"]
        );
    }

    #[test]
    fn only_a_commit_on_the_snapshot_ref_can_be_read() {
        // The author's own repository: their commit is not a snapshot, and its tree is rooted at
        // the repository, not necessarily at this folder.
        let tmp = project("committed by hand");
        let repository = Repository::init(tmp.path()).unwrap();
        let who = Signature::now("Ada", "ada@example.invalid").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("main.tex")).unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let theirs = repository
            .commit(Some("HEAD"), &who, &who, "mine", &tree, &[])
            .unwrap();
        snapshot(tmp.path()).unwrap();

        assert!(matches!(
            read(tmp.path(), &theirs.to_string(), "main.tex"),
            Err(SnapshotError::NotASnapshot)
        ));
        assert!(matches!(
            files(tmp.path(), &theirs.to_string()),
            Err(SnapshotError::NotASnapshot)
        ));
        assert!(matches!(
            read(tmp.path(), "not an id", "main.tex"),
            Err(SnapshotError::NotASnapshot)
        ));
        // …and listing shows only snapshots.
        assert_eq!(list(tmp.path(), 10).unwrap().len(), 1);
    }
}
