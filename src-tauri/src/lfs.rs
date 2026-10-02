//! Git LFS, for the Source Control view's large-file banner (S11.3c, DESIGN.md §5.7: "Binary
//! assets over a threshold prompt for Git LFS").
//!
//! Git LFS is never bundled the way Tectonic and TexLab are (`abstract-tex-sidecar`): this app
//! works with no Git LFS at all, and an author who never writes a large figure should never pay
//! for carrying it. This module only asks whatever `git-lfs` a search of `PATH` finds to do the
//! one thing `abstract_tex_git` cannot — register the `lfs` filter and rewrite `.gitattributes`
//! correctly, patterns and precedence included. Everything after that is `abstract_tex_git::stage`,
//! already tested, called again rather than reimplemented here.
//!
//! **What it must never do:**
//!
//! - Never report success when `git-lfs` is not on this machine. A missing binary is `anyhow`'s
//!   own sentence, naming where to get it — never a silent no-op that leaves a large file
//!   committed as plain Git content with nothing tracking it.
//! - Never assume yesterday's answer. Whether `git-lfs` is installed is checked fresh on every
//!   call, the same reason S9.8's shell-escape consent is asked per machine and not cached.

use std::path::Path;

use anyhow::{bail, Context, Result};
use tokio::process::Command;

/// Track every path in `paths` with Git LFS, then stage `.gitattributes` and each path so the
/// filter actually takes effect on the next commit.
///
/// Literal paths, never a glob built from one — `*.png` would also claim every future figure the
/// banner has not shown the author yet, and the whole point of a prompt is that it only ever
/// speaks for files it has actually found. `git lfs install --local` runs first and is safe to
/// repeat: it only (re)writes this repository's own filter configuration, which a machine that
/// has tracked nothing here before may not have yet even with `git-lfs` itself installed.
pub async fn track(project_dir: &Path, paths: &[String]) -> Result<()> {
    track_with(Path::new("git"), project_dir, paths).await
}

/// Same, naming the `git` binary to run. `pub` only for `tests/lfs.rs`, which points this at one
/// of the `fake-git-lfs-*` binaries (`src/bin/`) rather than the real `git` — the same reason
/// `abstract-tex-lsp`'s `LspSession::at` is public: it proves this module's own logic against
/// real subprocess behaviour, a real exit code and real stderr, without depending on whether
/// *this* machine happens to have Git LFS installed. (GitHub-hosted CI runners ship it by
/// default; this development machine does not — either fact would make a test against the real
/// `git` flaky in one place or the other.)
pub async fn track_with(git: &Path, project_dir: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        bail!("No files to track.");
    }
    if !is_installed(git).await {
        bail!("Git LFS isn't installed on this machine. Install it from https://git-lfs.com, then try again.");
    }

    run(git, project_dir, &["lfs", "install", "--local"], "setting up Git LFS for this repository").await?;
    let mut track_args: Vec<&str> = vec!["lfs", "track"];
    track_args.extend(paths.iter().map(String::as_str));
    run(git, project_dir, &track_args, "tracking the file with Git LFS").await?;

    let repository = abstract_tex_git::open(project_dir)?;
    abstract_tex_git::stage(&repository, ".gitattributes")?;
    for path in paths {
        abstract_tex_git::stage(&repository, path)?;
    }
    Ok(())
}

/// Whether `git lfs` answers at all. `tokio::process::Command`, not `std::process::Command`:
/// this runs straight inside an async Tauri command with no `spawn_blocking` of its own, the way
/// `abstract-tex-lsp` and `abstract-tex-engine` already spawn their own subprocesses — a process
/// launch and wait is exactly what that type exists not to block a worker thread for.
async fn is_installed(git: &Path) -> bool {
    Command::new(git).args(["lfs", "version"]).output().await.is_ok_and(|output| output.status.success())
}

/// Run `git <args>` in `project_dir`, failing with one complete sentence that names both what was
/// being attempted and git's own stderr — not two halves split across a wrapped error's context
/// and its source, where `anyhow::Error`'s `Display` (what `to_message` actually sends the
/// frontend) only ever shows the outer one.
async fn run(git: &Path, project_dir: &Path, args: &[&str], action: &str) -> Result<()> {
    let output =
        Command::new(git).args(args).current_dir(project_dir).output().await.with_context(|| format!("running git to {action}"))?;
    if !output.status.success() {
        bail!("Git LFS failed while {action}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}
