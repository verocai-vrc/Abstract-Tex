//! Which Git repository a document's folder is in. Asked by every crate that opens one.
//!
//! Not `Repository::discover`, which also takes a folder that *is* the inside of a repository: one with
//! `HEAD`, `config`, `objects/` and `refs/` directly in it. A zip can hold such a folder under any name,
//! and its `config` can say `worktree = ../../..`. Then the person's home folder is "the working tree", and
//! every verb (list, discard, commit) is about their own files. Git itself calls this an embedded bare
//! repository, and has a setting against it.
//!
//! **What it must never do:** answer with a repository whose working tree is not the folder in which its
//! `.git` stands, or with one that has no `.git` at all.

use std::path::{Path, PathBuf};

use git2::{ErrorCode, Repository, RepositoryOpenFlags};

/// The repository `project_dir` is in, or `None`: the nearest folder upwards with a `.git`, as Git looks
/// for one, because a thesis is often a subfolder of a repository. That `.git` is a folder, or a file that
/// says where the folder is (a linked worktree, a submodule, `git init --separate-git-dir`).
pub fn repository_of(project_dir: &Path) -> Option<Repository> {
    for folder in really_at(project_dir)?.ancestors() {
        match look_in(folder) {
            Look::Found(repository) => return Some(repository),
            Look::Refused => return None,
            Look::Nothing => {}
        }
    }
    None
}

/// The repository whose working tree is exactly `folder`: not one that it is only somewhere inside.
pub fn repository_at(folder: &Path) -> Option<Repository> {
    match look_in(&really_at(folder)?) {
        Look::Found(repository) => Some(repository),
        Look::Refused | Look::Nothing => None,
    }
}

/// `folder` as the folders above it are to be looked through: with no link in it, where Git resolves them.
/// On Windows it does not, and `canonicalize` gives a `\\?\` path, which libgit2 cannot be trusted with.
fn really_at(folder: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        std::path::absolute(folder).ok()
    } else {
        folder.canonicalize().ok()
    }
}

enum Look {
    Found(Repository),
    /// There is a `.git` that this app will not work with. Looking further up would be guessing.
    Refused,
    Nothing,
}

fn look_in(folder: &Path) -> Look {
    let dot_git = folder.join(".git");
    if !dot_git.exists() {
        return Look::Nothing;
    }
    // `NO_SEARCH`: nothing above `folder`. `&[] as &[&Path]`: the folders to stop at, of which there are
    // none. The type must be said, because an empty list has nothing to guess it from.
    let opened = Repository::open_ext(folder, RepositoryOpenFlags::NO_SEARCH, &[] as &[&Path]);
    let repository = match opened {
        Ok(repository) => repository,
        // A folder that merely has the name: Git looks further up as well.
        Err(error) if error.code() == ErrorCode::NotFound && dot_git.is_dir() => return Look::Nothing,
        Err(_) => return Look::Refused,
    };
    // libgit2 tries the folder itself after a `.git` of no use, which is what must not be taken.
    if same_folder(repository.path(), folder) {
        return Look::Nothing;
    }
    if repository.workdir().is_some_and(|tree| same_folder(tree, folder)) {
        Look::Found(repository)
    } else {
        Look::Refused
    }
}

/// `canonicalize` on both, so that two spellings of one folder are one folder.
fn same_folder(one: &Path, other: &Path) -> bool {
    matches!((one.canonicalize(), other.canonicalize()), (Ok(one), Ok(other)) if one == other)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::RepositoryInitOptions;

    use super::*;

    /// Where the repository found for `folder` says its working tree is.
    fn tree_for(folder: &Path) -> Option<std::path::PathBuf> {
        repository_of(folder).map(|found| found.workdir().unwrap().canonicalize().unwrap())
    }

    fn commit_nothing(repository: &Repository) {
        let who = git2::Signature::now("Ada", "ada@example.com").unwrap();
        let tree = repository
            .find_tree(repository.treebuilder(None).unwrap().write().unwrap())
            .unwrap();
        repository
            .commit(Some("HEAD"), &who, &who, "first", &tree, &[])
            .unwrap();
    }

    /// `at` made into the inside of a repository whose working tree is `worktree`.
    fn plant(at: &Path, worktree: &str) {
        Repository::init_bare(at).unwrap();
        fs::write(
            at.join("config"),
            format!("[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = {worktree}\n"),
        )
        .unwrap();
    }

    #[test]
    fn the_inside_of_a_repository_that_a_document_brought_is_not_one() {
        let home = tempfile::tempdir().unwrap();
        let document = home.path().join("Documents/Abstract-Tex/thesis");
        fs::create_dir_all(document.join("chapters")).unwrap();
        plant(&document, "../../..");
        assert!(repository_of(&document).is_none());
        assert!(repository_of(&document.join("chapters")).is_none());
        // With a working tree of its own, or none: still not what a document's history is kept in.
        plant(&document, ".");
        assert!(repository_of(&document).is_none());
        // Nor behind a `.git` of no use, for which libgit2 goes on to the folder itself.
        fs::create_dir(document.join(".git")).unwrap();
        assert!(repository_of(&document).is_none());
        fs::write(document.join("config"), "[core]\n\tbare = true\n").unwrap();
        assert!(repository_of(&document).is_none());
        assert!(repository_at(&document).is_none());
    }

    #[test]
    fn it_does_not_hide_the_repository_the_document_is_really_in() {
        let outer = tempfile::tempdir().unwrap();
        Repository::init(outer.path()).unwrap();
        let document = outer.path().join("thesis");
        fs::create_dir_all(&document).unwrap();
        plant(&document, "../..");
        assert_eq!(tree_for(&document), Some(outer.path().canonicalize().unwrap()));
    }

    #[test]
    fn a_dot_git_that_claims_another_folder_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let document = home.path().join("thesis");
        fs::create_dir_all(&document).unwrap();
        plant(&document.join(".git"), "../..");
        assert!(repository_of(&document).is_none());

        // And one that is a file pointing at a repository inside the document.
        let other = home.path().join("other");
        fs::create_dir_all(&other).unwrap();
        plant(&other.join("figures"), "../..");
        fs::write(other.join(".git"), "gitdir: figures\n").unwrap();
        assert!(repository_of(&other).is_none());
    }

    #[test]
    fn a_document_in_a_subfolder_is_in_the_repository_above_it() {
        let outer = tempfile::tempdir().unwrap();
        Repository::init(outer.path()).unwrap();
        let document = outer.path().join("papers/one");
        fs::create_dir_all(&document).unwrap();
        // A `.git` with nothing in it is not a repository, and Git looks further up.
        fs::create_dir(document.join(".git")).unwrap();
        let root = outer.path().canonicalize().unwrap();
        assert_eq!(tree_for(&document), Some(root.clone()));
        assert_eq!(tree_for(outer.path()), Some(root));
        assert!(repository_at(outer.path()).is_some());
        assert!(repository_at(&document).is_none());
        assert!(repository_of(&outer.path().join("not there")).is_none());
        assert!(repository_of(tempfile::tempdir().unwrap().path()).is_none());
    }

    #[test]
    fn a_linked_worktree_is_found() {
        let outer = tempfile::tempdir().unwrap();
        let main = Repository::init(outer.path().join("main")).unwrap();
        commit_nothing(&main);
        let linked = outer.path().join("linked");
        main.worktree("linked", &linked, None).unwrap();
        fs::create_dir(linked.join("chapters")).unwrap();
        for folder in [linked.clone(), linked.join("chapters")] {
            assert_eq!(tree_for(&folder), Some(linked.canonicalize().unwrap()));
        }
    }

    // A submodule's `.git` is a file that points into the outer repository, whose `modules/` say
    // `core.worktree`; `git init --separate-git-dir` makes the same shape elsewhere.
    #[test]
    fn a_submodule_and_a_separate_git_dir_are_found() {
        for git_dir in ["super/.git/modules/paper", "elsewhere/paper.git"] {
            let outer = tempfile::tempdir().unwrap();
            Repository::init(outer.path().join("super")).unwrap();
            let paper = outer.path().join("super/paper");
            fs::create_dir_all(&paper).unwrap();
            Repository::init_opts(
                outer.path().join(git_dir),
                RepositoryInitOptions::new()
                    .no_dotgit_dir(true)
                    .mkpath(true)
                    .workdir_path(&paper),
            )
            .unwrap();
            assert!(paper.join(".git").is_file(), "the premise: a file");
            assert_eq!(tree_for(&paper), Some(paper.canonicalize().unwrap()), "{git_dir}");
            // Git itself writes no `core.worktree` for `--separate-git-dir`; libgit2 did, just above.
            let mut config = repository_of(&paper).unwrap().config().unwrap();
            config.remove("core.worktree").unwrap();
            assert_eq!(tree_for(&paper), Some(paper.canonicalize().unwrap()), "{git_dir}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_document_reached_through_a_link_is_in_the_repository_it_is_really_in() {
        let outer = tempfile::tempdir().unwrap();
        Repository::init(outer.path().join("repository")).unwrap();
        fs::create_dir(outer.path().join("repository/paper")).unwrap();
        let link = outer.path().join("link");
        std::os::unix::fs::symlink(outer.path().join("repository/paper"), &link).unwrap();
        assert_eq!(
            tree_for(&link),
            Some(outer.path().join("repository").canonicalize().unwrap())
        );
    }

    // As `git-new-workdir` and Google's `repo` leave one: the link is the document's own `.git`.
    #[cfg(unix)]
    #[test]
    fn a_dot_git_that_is_a_link_is_found() {
        let outer = tempfile::tempdir().unwrap();
        let paper = outer.path().join("paper");
        Repository::init(&paper).unwrap();
        fs::rename(paper.join(".git"), outer.path().join("kept")).unwrap();
        std::os::unix::fs::symlink(outer.path().join("kept"), paper.join(".git")).unwrap();
        assert_eq!(tree_for(&paper), Some(paper.canonicalize().unwrap()));
    }
}
