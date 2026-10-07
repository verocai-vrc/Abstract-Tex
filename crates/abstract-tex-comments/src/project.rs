//! Opening the one repository a project's comments live in, from a plain project folder — the
//! same shape `abstract-tex-snapshot` already established for its own hidden ref. Duplicated
//! rather than shared: a few lines each, and the module doc on that crate's own
//! `open_or_create`/`open_existing` already explains why that crate accepts the duplication
//! rather than a dependency between two otherwise-independent crates.
//!
//! Every function here is the `&Path`-based sibling of one in [`crate::store`], so a caller that
//! only has a project folder — `src-tauri`, once something there calls this crate — never needs
//! to know what a `git2::Repository` is.

use std::path::Path;

use git2::Repository;

use crate::store::{self, CommentRecord, CommentsError, LoadedComment, MergeOutcome};

/// Where comments are kept for a project that is not itself a Git repository: a bare repository
/// of our own, under the folder DESIGN.md §5.8 already reserves for us.
///
/// A different physical repository from `abstract-tex-snapshot`'s own `.abstract-tex/snapshots.git`
/// — each hidden-ref crate owns its own, so one never has to know the other's ref names to avoid
/// colliding with them.
const OWN_REPOSITORY: &str = ".abstract-tex/comments.git";

/// The repository a comment would be *written* into: the author's own, if they have one
/// (discovered by walking up from `project_dir`, since a project is often a subfolder of a larger
/// repository — a thesis inside a monorepo, a paper inside a folder of papers), or ours.
fn open_or_create(project_dir: &Path) -> Result<Repository, CommentsError> {
    if let Ok(theirs) = Repository::discover(project_dir) {
        return Ok(theirs);
    }
    let ours = project_dir.join(OWN_REPOSITORY);
    match Repository::open_bare(&ours) {
        Ok(repository) => Ok(repository),
        Err(_) => Repository::init_bare(&ours).map_err(CommentsError::Write),
    }
}

/// The repository comments would be *read* from, without making one — asking "does this project
/// have any comments?" must never itself leave a repository behind where there was none, the same
/// reasoning `abstract-tex-snapshot::open_existing`'s own doc gives.
fn open_existing(project_dir: &Path) -> Option<Repository> {
    Repository::discover(project_dir)
        .or_else(|_| Repository::open_bare(project_dir.join(OWN_REPOSITORY)))
        .ok()
}

/// [`store::add`] against the repository `project_dir` names.
pub fn add(project_dir: &Path, record: &CommentRecord) -> Result<String, CommentsError> {
    store::add(&open_or_create(project_dir)?, record)
}

/// [`store::resolve`] against the repository `project_dir` names.
pub fn resolve(
    project_dir: &Path,
    comment_id: &str,
    author: &str,
    resolved: bool,
    created: i64,
) -> Result<String, CommentsError> {
    store::resolve(
        &open_or_create(project_dir)?,
        comment_id,
        author,
        resolved,
        created,
    )
}

/// [`store::load`] against the repository `project_dir` names — an empty list, not an error, for
/// a project that has no comments repository at all yet (nobody has commented, or fetched anyone
/// else's comment, so there is nothing to open and nothing to report).
pub fn load(
    project_dir: &Path,
    read_file: impl Fn(&str) -> Option<String>,
) -> Result<Vec<LoadedComment>, CommentsError> {
    match open_existing(project_dir) {
        Some(repository) => store::load(&repository, read_file),
        None => Ok(Vec::new()),
    }
}

/// Fetch, merge, and push — in that order, the same order `abstract-tex-git::sync` already uses
/// for the author's own branch — against the repository `project_dir` names. *Sync*'s own
/// explicit refspec (DESIGN.md §5.6): this never rides along with a branch push or fetch, and a
/// branch sync never sends this ref either.
pub fn sync(project_dir: &Path, remote_name: &str) -> Result<MergeOutcome, CommentsError> {
    let repository = open_or_create(project_dir)?;
    store::fetch(&repository, remote_name)?;
    let outcome = store::merge_from(&repository, remote_name)?;
    store::push(&repository, remote_name)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn record(file: &str, quote_in: &str, author: &str, body: &str) -> CommentRecord {
        let start = quote_in.find("result").unwrap();
        let end = start + "result".len();
        CommentRecord {
            file: file.to_string(),
            anchor: crate::Anchor::capture(quote_in, start, end).unwrap(),
            author: author.to_string(),
            body: body.to_string(),
            created: 0,
        }
    }

    #[test]
    fn a_project_with_no_comments_yet_loads_as_an_empty_list_and_creates_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load(tmp.path(), |_| None).unwrap();
        assert_eq!(loaded, Vec::new());
        assert!(
            !tmp.path().join(OWN_REPOSITORY).exists(),
            "a read must never create the repository it is asking about"
        );
    }

    #[test]
    fn adding_a_comment_to_a_project_with_no_git_repository_uses_our_own() {
        let tmp = tempfile::tempdir().unwrap();
        let text = "the result was significant";
        fs::write(tmp.path().join("main.tex"), text).unwrap();

        add(tmp.path(), &record("main.tex", text, "ada", "says who?")).unwrap();

        assert!(tmp.path().join(OWN_REPOSITORY).exists());
        let files: std::collections::HashMap<&str, &str> = [("main.tex", text)].into_iter().collect();
        let loaded = load(tmp.path(), |f| files.get(f).map(|s| s.to_string())).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].record.body, "says who?");
    }

    #[test]
    fn adding_a_comment_to_a_project_that_is_already_a_git_repository_uses_that_one() {
        let tmp = tempfile::tempdir().unwrap();
        Repository::init(tmp.path()).unwrap();
        let text = "the result stands";
        fs::write(tmp.path().join("main.tex"), text).unwrap();

        add(tmp.path(), &record("main.tex", text, "ada", "?")).unwrap();

        assert!(
            !tmp.path().join(OWN_REPOSITORY).exists(),
            "an author's own repository must not grow a second, unused one alongside it"
        );
        let repository = Repository::open(tmp.path()).unwrap();
        assert_eq!(store::load_all(&repository).unwrap().len(), 1);
    }

    #[test]
    fn adding_a_comment_from_a_subfolder_of_a_larger_repository_reaches_the_repositorys_root() {
        let tmp = tempfile::tempdir().unwrap();
        Repository::init(tmp.path()).unwrap();
        let paper = tmp.path().join("papers").join("draft");
        fs::create_dir_all(&paper).unwrap();
        let text = "the result improves on prior work";
        fs::write(paper.join("main.tex"), text).unwrap();

        add(&paper, &record("main.tex", text, "ada", "?")).unwrap();

        let repository = Repository::open(tmp.path()).unwrap();
        assert_eq!(
            store::load_all(&repository).unwrap().len(),
            1,
            "the comment landed on the repository found by walking up from the subfolder"
        );
    }

    #[test]
    fn resolving_through_the_project_wrapper_folds_into_a_load_the_same_way() {
        let tmp = tempfile::tempdir().unwrap();
        let text = "the result stands";
        fs::write(tmp.path().join("main.tex"), text).unwrap();
        let id = add(tmp.path(), &record("main.tex", text, "ada", "?")).unwrap();

        resolve(tmp.path(), &id, "bo", true, 1).unwrap();

        let files: std::collections::HashMap<&str, &str> = [("main.tex", text)].into_iter().collect();
        let loaded = load(tmp.path(), |f| files.get(f).map(|s| s.to_string())).unwrap();
        assert!(loaded[0].resolved);
    }

    #[test]
    fn syncing_two_projects_that_each_commented_offline_leaves_both_with_everything() {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = format!("file://{}", origin_dir.path().display());

        let ada_dir = tempfile::tempdir().unwrap();
        fs::write(ada_dir.path().join("main.tex"), "the result was clear").unwrap();
        {
            let repository = open_or_create(ada_dir.path()).unwrap();
            repository.remote("origin", &origin_url).unwrap();
        }
        add(
            ada_dir.path(),
            &record("main.tex", "the result was clear", "ada", "ada's note"),
        )
        .unwrap();

        let bo_dir = tempfile::tempdir().unwrap();
        fs::write(bo_dir.path().join("main.tex"), "the result was odd").unwrap();
        {
            let repository = open_or_create(bo_dir.path()).unwrap();
            repository.remote("origin", &origin_url).unwrap();
        }
        add(
            bo_dir.path(),
            &record("main.tex", "the result was odd", "bo", "bo's note"),
        )
        .unwrap();

        sync(ada_dir.path(), "origin").unwrap();
        sync(bo_dir.path(), "origin").unwrap();
        sync(ada_dir.path(), "origin").unwrap();

        let mut ada_bodies: Vec<String> = load(ada_dir.path(), |_| None)
            .unwrap()
            .into_iter()
            .map(|c| c.record.body)
            .collect();
        ada_bodies.sort();
        assert_eq!(ada_bodies, vec!["ada's note", "bo's note"]);

        let mut bo_bodies: Vec<String> = load(bo_dir.path(), |_| None)
            .unwrap()
            .into_iter()
            .map(|c| c.record.body)
            .collect();
        bo_bodies.sort();
        assert_eq!(bo_bodies, vec!["ada's note", "bo's note"]);
    }
}
