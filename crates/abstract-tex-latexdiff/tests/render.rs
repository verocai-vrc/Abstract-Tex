//! `abstract-tex-latexdiff`, against the `fake-latexdiff-*` stand-ins `cargo test` builds
//! alongside these tests (S11.4b) — real subprocess behaviour, a real exit code and real
//! stderr, without this machine (or most CI runners) needing a real `latexdiff` install.
//! `CARGO_BIN_EXE_*` is only set for integration tests, which is why these live here.

use std::path::Path;

use abstract_tex_git::Repository;
use abstract_tex_latexdiff::render_with;

fn installed() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-latexdiff-installed"))
}

fn missing() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-latexdiff-missing"))
}

fn fails() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-latexdiff-fails"))
}

/// A repository with one commit holding `main.tex`, and a second commit that changes it — the
/// two revisions every test below compares.
fn repo_with_two_revisions() -> (tempfile::TempDir, Repository, git2::Oid, git2::Oid) {
    let tmp = tempfile::tempdir().unwrap();
    let repository = Repository::init(tmp.path()).unwrap();
    let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();

    std::fs::write(tmp.path().join("main.tex"), "the first version\n").unwrap();
    let old = {
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("main.tex")).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        repository
            .commit(Some("HEAD"), &who, &who, "first", &tree, &[])
            .unwrap()
    };

    std::fs::write(tmp.path().join("main.tex"), "the second version\n").unwrap();
    let new = {
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("main.tex")).unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repository.find_commit(old).unwrap();
        repository
            .commit(Some("HEAD"), &who, &who, "second", &tree, &[&parent])
            .unwrap()
    };

    (tmp, repository, old, new)
}

/// The card's own done-when: a machine with no `latexdiff` is refused by a sentence naming where
/// to get it, before either revision is even exported.
#[test]
fn rendering_is_refused_by_name_when_latexdiff_is_not_installed() {
    let (_tmp, repository, old, new) = repo_with_two_revisions();
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let error = render_with(
        missing(),
        &repository,
        old,
        new,
        "main.tex",
        old_dir.path(),
        new_dir.path(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("isn't installed"), "{error}");
    assert!(error.to_string().contains("ctan.org"), "{error}");
    // Refused before either export ran — nothing to clean up, nothing half-done.
    assert!(!old_dir.path().join("main.tex").exists());
    assert!(!new_dir.path().join("main.tex").exists());
}

/// Both revisions are exported, and the diffed content lands over the *new* revision's copy —
/// which is what lets everything else in that revision's tree (figures, a bibliography, a
/// document class) still be sitting right where the compiler would look for them.
#[test]
fn rendering_exports_both_revisions_and_writes_the_diff_over_the_new_one() {
    let (_tmp, repository, old, new) = repo_with_two_revisions();
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let diff_path = render_with(
        installed(),
        &repository,
        old,
        new,
        "main.tex",
        old_dir.path(),
        new_dir.path(),
    )
    .unwrap();

    assert_eq!(diff_path, new_dir.path().join("main.tex"));
    assert_eq!(
        std::fs::read_to_string(&diff_path).unwrap(),
        "% a latexdiff would have gone here\n"
    );
    // The old export is untouched — its own copy of the file is still the first version, not the
    // fake's fixed diff output, because only `new_dir`'s copy is ever overwritten.
    assert_eq!(
        std::fs::read_to_string(old_dir.path().join("main.tex")).unwrap(),
        "the first version\n"
    );
}

/// `latexdiff` installed but refusing the files it was given — its own stderr, not a generic
/// phrase, must reach the final error.
#[test]
fn latexdiffs_own_refusal_reaches_the_error_message() {
    let (_tmp, repository, old, new) = repo_with_two_revisions();
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let error = render_with(
        fails(),
        &repository,
        old,
        new,
        "main.tex",
        old_dir.path(),
        new_dir.path(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("unbalanced braces"), "{error}");
}
