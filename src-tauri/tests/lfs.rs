//! `src-tauri/src/lfs.rs`, against the `fake-git-lfs-*` stand-ins `cargo test` builds alongside
//! these tests (S11.3c) — real subprocess behaviour, a real exit code and real stderr, without
//! depending on whether this build machine happens to have Git LFS installed. `CARGO_BIN_EXE_*`
//! is only set for integration tests, which is why these live here and not inside `src/lfs.rs`.

use std::path::Path;

use abstract_tex_lib::lfs::track_with;

fn installed() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-git-lfs-installed"))
}

fn missing() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-git-lfs-missing"))
}

fn track_fails() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-git-lfs-track-fails"))
}

/// No paths is a caller's bug, not a reason to shell out at all — checked before `git` is even
/// asked whether it exists, so this is the one case true on every machine alike.
#[tokio::test]
async fn tracking_nothing_is_refused_before_any_subprocess_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let error = track_with(missing(), tmp.path(), &[]).await.unwrap_err();
    assert_eq!(error.to_string(), "No files to track.");
}

/// The card's own done-when: a machine with no Git LFS is refused by a sentence naming where to
/// get it, not a bare subprocess exit code.
#[tokio::test]
async fn tracking_is_refused_by_name_when_git_lfs_is_not_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let error = track_with(missing(), tmp.path(), &["figure.png".to_string()]).await.unwrap_err();
    assert!(error.to_string().contains("isn't installed"), "{error}");
    assert!(error.to_string().contains("git-lfs.com"), "{error}");
}

/// A real `git-lfs` writes `.gitattributes` and leaves the file itself untouched; this proves the
/// half this module owns — staging both afterwards so the filter takes effect on the next commit,
/// via `abstract_tex_git::stage`, not a second, divergent implementation of it.
#[tokio::test]
async fn tracking_stages_gitattributes_and_the_file_once_git_lfs_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    abstract_tex_git::Repository::init(tmp.path()).unwrap();
    std::fs::write(tmp.path().join("figure.png"), b"not actually a figure").unwrap();

    track_with(installed(), tmp.path(), &["figure.png".to_string()]).await.unwrap();

    let repository = abstract_tex_git::open(tmp.path()).unwrap();
    let status = abstract_tex_git::status(&repository).unwrap();
    let staged: Vec<&str> = status.staged.iter().map(|change| change.path.as_str()).collect();
    assert!(staged.contains(&"figure.png"), "{staged:?}");
    assert!(staged.contains(&".gitattributes"), "{staged:?}");
}

/// Git LFS installed but refusing the file for its own reason — the `run` helper's stderr must
/// reach the author, not just a "command failed" with no content.
#[tokio::test]
async fn git_lfss_own_refusal_reaches_the_error_message() {
    let tmp = tempfile::tempdir().unwrap();
    let error = track_with(track_fails(), tmp.path(), &["figure.png".to_string()]).await.unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
}
