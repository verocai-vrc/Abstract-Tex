//! A11: naming a version.
//!
//! "The version your supervisor last saw" is a commit, and a name is how an author finds it again.
//! This module creates one kind of tag, **lightweight and local**, on a commit the author picked in
//! the Graph. The Graph already shows tags as chips beside the commit (`tags_by_commit`), so
//! creating one is all that was missing.
//!
//! **What it must never do:**
//!
//! - Never move or replace an existing tag. A tag is a promise that a name means one version.
//! - Never need an identity. A lightweight tag has no tagger, so naming a version cannot fail with
//!   "Git does not know who you are", which an annotated tag would.
//! - Never rewrite the name typed. One Git cannot use is refused with the rule, as branches are.
//!
//! It does not push. `sync` carries branches, not tags, so a name made here exists on this machine
//! only until the author pushes it by other means; the view says so.

use crate::{GitError, Oid, Repository};

/// Tag `commit` (a full id, as the Graph's rows carry it) with `name`.
pub fn create_tag(repository: &Repository, commit: &str, name: &str) -> Result<(), GitError> {
    let name = name.trim();
    // Git's own rule for a reference name, applied to the full `refs/tags/<name>` it would have.
    // It also rejects spaces, `..`, `~`, a trailing `.lock` and the empty name.
    if name.is_empty() || !git2::Reference::is_valid_name(&format!("refs/tags/{name}")) {
        return Err(GitError::InvalidTagName(name.to_string()));
    }
    let commit_id = Oid::from_str(commit).map_err(|_| GitError::NoSuchCommit(commit.to_string()))?;
    let commit = repository
        .find_commit(commit_id)
        .map_err(|_| GitError::NoSuchCommit(commit.to_string()))?;
    if repository.find_reference(&format!("refs/tags/{name}")).is_ok() {
        return Err(GitError::TagExists(name.to_string()));
    }
    // `false`: never overwrite. The check above already refused an existing name, and this keeps
    // the call itself from ever being a way to move a tag.
    repository.tag_lightweight(name, commit.as_object(), false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{branch_state, log};
    use std::fs;

    fn commit_all(repository: &Repository, message: &str) -> String {
        let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();
        let mut index = repository.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let head = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = head.iter().collect();
        repository
            .commit(Some("HEAD"), &who, &who, message, &tree, &parents)
            .unwrap()
            .to_string()
    }

    /// Two commits; returns `(older id, newer id)`.
    fn repo() -> (tempfile::TempDir, Repository, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("main.tex"), "one\n").unwrap();
        let first = commit_all(&repository, "first");
        fs::write(tmp.path().join("main.tex"), "two\n").unwrap();
        let second = commit_all(&repository, "second");
        (tmp, repository, first, second)
    }

    #[test]
    fn a_named_version_shows_as_a_chip_on_its_commit_and_nowhere_else() {
        let (_tmp, repository, first, second) = repo();

        create_tag(&repository, &first, "sent-to-supervisor").unwrap();

        let rows = log(&repository, 0, 10).unwrap();
        let tags_of = |id: &str| rows.iter().find(|row| row.id == id).unwrap().tags.clone();
        assert!(
            tags_of(&first).contains(&"sent-to-supervisor".to_string()),
            "{:?}",
            tags_of(&first)
        );
        assert!(!tags_of(&second).contains(&"sent-to-supervisor".to_string()));
    }

    #[test]
    fn naming_a_version_moves_nothing() {
        let (_tmp, repository, first, _second) = repo();
        let before = branch_state(&repository).unwrap();

        create_tag(&repository, &first, "v1").unwrap();

        assert_eq!(branch_state(&repository).unwrap(), before);
    }

    #[test]
    fn a_name_git_cannot_use_is_refused_in_a_sentence() {
        let (_tmp, repository, first, _second) = repo();
        for bad in [
            "Sent to supervisor, 3 Oct",
            "",
            "  ",
            "a..b",
            "bad~name",
            "ends.lock",
        ] {
            let error = create_tag(&repository, &first, bad).unwrap_err();
            assert!(matches!(error, GitError::InvalidTagName(_)), "{bad:?}: {error:?}");
            assert!(error.to_string().contains("no spaces"), "{error}");
        }
    }

    #[test]
    fn a_name_already_used_is_refused_and_the_tag_stays_where_it_was() {
        let (_tmp, repository, first, second) = repo();
        create_tag(&repository, &first, "draft").unwrap();

        let error = create_tag(&repository, &second, "draft").unwrap_err();

        assert!(matches!(error, GitError::TagExists(_)), "{error:?}");
        let rows = log(&repository, 0, 10).unwrap();
        let on_first = rows.iter().find(|row| row.id == first).unwrap();
        assert!(on_first.tags.contains(&"draft".to_string()));
    }

    #[test]
    fn a_commit_that_is_not_in_the_project_is_refused_by_a_sentence() {
        let (_tmp, repository, _first, _second) = repo();
        let error = create_tag(&repository, &"0".repeat(40), "v1").unwrap_err();
        assert!(matches!(error, GitError::NoSuchCommit(_)), "{error:?}");
        let error = create_tag(&repository, "not-a-commit", "v1").unwrap_err();
        assert!(matches!(error, GitError::NoSuchCommit(_)), "{error:?}");
    }
}
