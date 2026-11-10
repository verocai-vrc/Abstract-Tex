//! B4: listing, creating and switching local branches.
//!
//! Until now nothing in the app could leave the branch a project was cloned or initialised on,
//! which "compare a branch against `main`" and any pull-request workflow need. This module is the
//! smallest honest version of that: **local branches only, create and switch, nothing else.** No
//! delete, no rename, no merge-a-branch (sync merges from the remote; DESIGN.md §6 keeps the Graph
//! single-lane on purpose).
//!
//! **What it must never do:**
//!
//! - Never switch with uncommitted work in tracked files. A switch rewrites files on disk under
//!   an editor that has them open, and "your edits came along" is a thing an author who has never
//!   used Git cannot reason about. It refuses, in a sentence, and touches nothing.
//! - Never overwrite a file Git has never seen. A safe checkout refuses that too, and the refusal
//!   gets its own sentence rather than libgit2's.
//! - Never rewrite what the author typed. A branch name Git cannot use is refused with the rule,
//!   not quietly turned into a different name.
//! - Never leave the repository half-switched: the files move first, and `HEAD` moves only once
//!   they have.

use git2::{BranchType, ObjectType};

use crate::{status, ChangeKind, GitError, Repository};

/// One local branch, for the picker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRow {
    pub name: String,
    /// Whether this is the branch `HEAD` is on.
    pub current: bool,
}

/// Every local branch, the current one first and the rest alphabetical. Empty on a repository
/// with no commits, where `HEAD` names a branch that does not exist yet.
pub fn branches(repository: &Repository) -> Result<Vec<BranchRow>, GitError> {
    let mut rows: Vec<BranchRow> = Vec::new();
    // `repository.branches` is an iterator of `Result`s, so each item is unwrapped with `?` and
    // the error type converts through `GitError`'s `From<git2::Error>`.
    for entry in repository.branches(Some(BranchType::Local))? {
        let (branch, _kind) = entry?;
        // A branch name that is not valid UTF-8 cannot be shown or typed back, so it is left out.
        if let Some(name) = branch.name()? {
            rows.push(BranchRow {
                name: name.to_string(),
                current: branch.is_head(),
            });
        }
    }
    // `false` sorts before `true`, so the negation puts the current branch first; the name then
    // orders the rest.
    rows.sort_by(|a, b| (!a.current, &a.name).cmp(&(!b.current, &b.name)));
    Ok(rows)
}

/// Start a branch at the current commit and switch to it, like `git switch -c`.
///
/// Uncommitted changes are fine here and travel with the author: the files on disk do not change,
/// only which branch the next commit lands on. That is the natural way to say "this should have
/// been on its own branch".
pub fn create_branch(repository: &Repository, name: &str) -> Result<(), GitError> {
    let name = name.trim();
    // `name_is_valid` is Git's own rule, so what this accepts is what `git branch` would accept.
    // The empty string is invalid under it too.
    if !git2::Branch::name_is_valid(name)? {
        return Err(GitError::InvalidBranchName(name.to_string()));
    }
    let head = repository.head().map_err(|_| GitError::NothingToBranchFrom)?;
    if repository.find_branch(name, BranchType::Local).is_ok() {
        return Err(GitError::BranchExists(name.to_string()));
    }
    let commit = head.peel_to_commit()?;
    // `false`: never replace an existing branch. The check above already refused one, and this
    // keeps the call itself from ever being a way to move a branch out from under its commits.
    let branch = repository.branch(name, &commit, false)?;
    point_head_at(repository, &branch)
}

/// Switch to an existing local branch, moving the files on disk to its version.
pub fn switch_branch(repository: &Repository, name: &str) -> Result<(), GitError> {
    let branch = repository
        .find_branch(name, BranchType::Local)
        .map_err(|_| GitError::NoSuchBranch(name.to_string()))?;
    if branch.is_head() {
        return Ok(());
    }
    refuse_if_uncommitted(repository)?;

    let target = branch.get().peel(ObjectType::Commit)?;
    // `safe`, not `force`: a safe checkout refuses to overwrite anything it was not told about,
    // which is the whole protection for the author's untracked files. `refuse_if_uncommitted` has
    // already dealt with tracked changes, so what can still trip this is an untracked file the
    // other branch has a tracked copy of.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repository
        .checkout_tree(&target, Some(&mut checkout))
        .map_err(|error| {
            if error.code() == git2::ErrorCode::Conflict {
                GitError::WouldOverwriteFiles
            } else {
                GitError::Git(error)
            }
        })?;
    // Only now, with the files already moved: a failure above leaves `HEAD` where it was.
    point_head_at(repository, &branch)
}

/// Tracked files with changes Git has not recorded, or a merge half done. Untracked files are not
/// counted: they are not rewritten by a switch unless the other branch has a file of the same name,
/// which the checkout itself catches.
fn refuse_if_uncommitted(repository: &Repository) -> Result<(), GitError> {
    let status = status(repository)?;
    let tracked_change = status
        .unstaged
        .iter()
        .any(|change| change.kind != ChangeKind::Untracked);
    if tracked_change || !status.staged.is_empty() || !status.conflicted.is_empty() {
        return Err(GitError::UncommittedChanges);
    }
    Ok(())
}

fn point_head_at(repository: &Repository, branch: &git2::Branch<'_>) -> Result<(), GitError> {
    // `get().name()` is the full ref (`refs/heads/second-draft`), which is what `set_head` wants.
    let reference = branch
        .get()
        .name()
        .ok_or_else(|| GitError::Git(git2::Error::from_str("the branch's name is not valid UTF-8")))?;
    repository.set_head(reference)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_state;
    use std::fs;
    use std::path::Path;

    fn commit_all(repository: &Repository, message: &str) {
        let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();
        let mut index = repository.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let head = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = head.iter().collect();
        repository
            .commit(Some("HEAD"), &who, &who, message, &tree, &parents)
            .unwrap();
    }

    /// A repository with one commit of `main.tex`.
    fn repo() -> (tempfile::TempDir, Repository) {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "the first version\n").unwrap();
        commit_all(&repository, "first");
        (tmp, repository)
    }

    fn current(repository: &Repository) -> String {
        branch_state(repository)
            .unwrap()
            .name
            .expect("a branch is checked out")
    }

    fn on_disk(tmp: &tempfile::TempDir, file: &str) -> String {
        fs::read_to_string(tmp.path().join(file)).unwrap()
    }

    #[test]
    fn the_current_branch_comes_first_and_the_rest_alphabetically() {
        let (_tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "zeta").unwrap();
        switch_branch(&repository, &original).unwrap();
        create_branch(&repository, "alpha").unwrap();
        switch_branch(&repository, &original).unwrap();

        let names: Vec<(String, bool)> = branches(&repository)
            .unwrap()
            .into_iter()
            .map(|row| (row.name, row.current))
            .collect();

        assert_eq!(
            names,
            vec![
                (original, true),
                ("alpha".to_string(), false),
                ("zeta".to_string(), false)
            ]
        );
    }

    #[test]
    fn a_repository_with_no_commits_has_no_branches_to_list() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        assert_eq!(branches(&repository).unwrap(), Vec::new());
    }

    #[test]
    fn a_new_branch_starts_at_the_current_commit_and_becomes_the_current_one() {
        let (_tmp, repository) = repo();
        let original = current(&repository);
        let head_before = branch_state(&repository).unwrap().head;

        create_branch(&repository, "second-draft").unwrap();

        assert_eq!(current(&repository), "second-draft");
        assert_eq!(branch_state(&repository).unwrap().head, head_before);
        assert!(
            branches(&repository)
                .unwrap()
                .iter()
                .any(|row| row.name == original),
            "the branch it started from is still there"
        );
    }

    #[test]
    fn uncommitted_work_travels_with_a_new_branch() {
        let (tmp, repository) = repo();
        fs::write(tmp.path().join("main.tex"), "half-written\n").unwrap();

        create_branch(&repository, "experiment").unwrap();

        assert_eq!(on_disk(&tmp, "main.tex"), "half-written\n");
    }

    #[test]
    fn a_name_git_cannot_use_is_refused_in_a_sentence_and_nothing_is_created() {
        let (_tmp, repository) = repo();
        let original = current(&repository);
        for bad in [
            "second draft",
            "",
            "   ",
            "a..b",
            "-leading-dash",
            "with:colon",
            "ends.lock",
        ] {
            let error = create_branch(&repository, bad).unwrap_err();
            assert!(
                matches!(error, GitError::InvalidBranchName(_)),
                "{bad:?}: {error:?}"
            );
            assert!(error.to_string().contains("no spaces"), "{error}");
        }
        assert_eq!(current(&repository), original);
        assert_eq!(branches(&repository).unwrap().len(), 1);
    }

    #[test]
    fn a_name_already_taken_is_refused() {
        let (_tmp, repository) = repo();
        let original = current(&repository);
        let error = create_branch(&repository, &original).unwrap_err();
        assert!(matches!(error, GitError::BranchExists(_)), "{error:?}");
    }

    #[test]
    fn a_branch_needs_a_commit_to_start_from() {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        let error = create_branch(&repository, "draft").unwrap_err();
        assert!(matches!(error, GitError::NothingToBranchFrom), "{error:?}");
    }

    #[test]
    fn switching_moves_the_files_on_disk_to_the_other_branchs_version_and_back() {
        let (tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "second-draft").unwrap();
        fs::write(tmp.path().join("main.tex"), "the second version\n").unwrap();
        fs::write(tmp.path().join("extra.tex"), "only on the branch\n").unwrap();
        commit_all(&repository, "second");

        switch_branch(&repository, &original).unwrap();
        assert_eq!(current(&repository), original);
        assert_eq!(on_disk(&tmp, "main.tex"), "the first version\n");
        assert!(!Path::new(&tmp.path().join("extra.tex")).exists());

        switch_branch(&repository, "second-draft").unwrap();
        assert_eq!(on_disk(&tmp, "main.tex"), "the second version\n");
        assert!(tmp.path().join("extra.tex").exists());
    }

    #[test]
    fn switching_to_the_branch_already_checked_out_is_not_an_error() {
        let (_tmp, repository) = repo();
        let original = current(&repository);
        switch_branch(&repository, &original).unwrap();
        assert_eq!(current(&repository), original);
    }

    #[test]
    fn switching_to_a_branch_that_does_not_exist_is_refused_by_name() {
        let (_tmp, repository) = repo();
        let error = switch_branch(&repository, "nowhere").unwrap_err();
        assert!(
            matches!(&error, GitError::NoSuchBranch(name) if name == "nowhere"),
            "{error:?}"
        );
    }

    /// The author's unsaved-to-Git work must survive: the refusal changes nothing, not the branch
    /// and not the file.
    #[test]
    fn a_switch_is_refused_and_touches_nothing_while_tracked_files_have_uncommitted_changes() {
        let (tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "second-draft").unwrap();
        switch_branch(&repository, &original).unwrap();
        fs::write(tmp.path().join("main.tex"), "edited, not committed\n").unwrap();

        let error = switch_branch(&repository, "second-draft").unwrap_err();

        assert!(matches!(error, GitError::UncommittedChanges), "{error:?}");
        assert_eq!(current(&repository), original);
        assert_eq!(on_disk(&tmp, "main.tex"), "edited, not committed\n");
    }

    #[test]
    fn staged_but_uncommitted_work_blocks_a_switch_too() {
        let (tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "second-draft").unwrap();
        switch_branch(&repository, &original).unwrap();
        fs::write(tmp.path().join("main.tex"), "staged\n").unwrap();
        crate::stage(&repository, "main.tex").unwrap();
        fs::write(tmp.path().join("main.tex"), "the first version\n").unwrap(); // wt == HEAD, index differs

        let error = switch_branch(&repository, "second-draft").unwrap_err();

        assert!(matches!(error, GitError::UncommittedChanges), "{error:?}");
    }

    #[test]
    fn a_file_git_has_never_seen_does_not_block_a_switch_and_stays_put() {
        let (tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "second-draft").unwrap();
        switch_branch(&repository, &original).unwrap();
        fs::write(tmp.path().join("notes.tex"), "my scratch notes\n").unwrap();

        switch_branch(&repository, "second-draft").unwrap();

        assert_eq!(current(&repository), "second-draft");
        assert_eq!(on_disk(&tmp, "notes.tex"), "my scratch notes\n");
    }

    /// The other branch has its own committed `extra.tex`; the author has an untracked file of the
    /// same name here. Overwriting it would destroy work Git has never seen.
    #[test]
    fn a_switch_that_would_overwrite_an_untracked_file_is_refused_and_the_file_survives() {
        let (tmp, repository) = repo();
        let original = current(&repository);
        create_branch(&repository, "second-draft").unwrap();
        fs::write(tmp.path().join("extra.tex"), "committed on the branch\n").unwrap();
        commit_all(&repository, "adds extra");
        switch_branch(&repository, &original).unwrap();
        fs::write(tmp.path().join("extra.tex"), "my own, never committed\n").unwrap();

        let error = switch_branch(&repository, "second-draft").unwrap_err();

        assert!(matches!(error, GitError::WouldOverwriteFiles), "{error:?}");
        assert_eq!(current(&repository), original);
        assert_eq!(on_disk(&tmp, "extra.tex"), "my own, never committed\n");
    }
}
