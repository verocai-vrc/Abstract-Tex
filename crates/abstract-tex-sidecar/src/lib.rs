//! Finding a helper program we bundle (DESIGN.md §4.1, "zero setup to first PDF").
//!
//! Owns exactly one rule, shared by the engine (Tectonic, S1.2) and the language server
//! (TexLab, S3.1): an explicit environment variable wins, then the sidecar Tauri placed beside
//! our own executable, then whatever is on `PATH`. It must never spawn anything — it answers
//! "where is it?", and the caller decides what to do with the answer.

use std::path::{Path, PathBuf};

use tracing::{info, warn};

/// Where a program was found. Shown in the UI and in logs so a user can tell which one ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An environment variable named it explicitly.
    EnvOverride,
    /// The copy Tauri bundles next to our executable.
    Sidecar,
    /// The user's own installation, on `PATH`.
    OnPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub source: Source,
}

/// Locate `name` (e.g. `"tectonic"`), honouring `env_override` (e.g. `"ABSTRACT_TEX_TECTONIC"`).
///
/// Order matters and is deliberate:
/// 1. The environment variable — an explicit choice always wins, and it is how a developer
///    tests a new release without touching the bundle.
/// 2. The sidecar — the "zero setup" path for everyone else.
/// 3. `PATH` — a user who already has the program installed.
pub fn locate(name: &str, env_override: &str) -> Option<Found> {
    if let Some(explicit) = std::env::var_os(env_override) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            info!(?path, "using {name} from {env_override}");
            return Some(Found {
                path,
                source: Source::EnvOverride,
            });
        }
        warn!(?path, "{env_override} is set but is not a file; ignoring");
    }

    if let Some(sidecar) = sidecar_path(name) {
        if sidecar.is_file() {
            info!(?sidecar, "using bundled {name} sidecar");
            return Some(Found {
                path: sidecar,
                source: Source::Sidecar,
            });
        }
    }

    if let Some(on_path) = find_on_path(&exe_name(name)) {
        info!(?on_path, "using {name} from PATH");
        return Some(Found {
            path: on_path,
            source: Source::OnPath,
        });
    }

    None
}

/// Tauri places `externalBin` sidecars beside the main executable, with the target triple
/// stripped from the name: `binaries/tectonic-x86_64-pc-windows-msvc.exe` in the repo becomes
/// `tectonic.exe` next to `abstract-tex.exe`. This is true in `tauri dev` too, because
/// `tauri-build` copies sidecars into `target/debug/`.
pub fn sidecar_path(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    Some(dir.join(exe_name(name)))
}

/// Searches `PATH` for an executable. Small enough to write ourselves rather than pull a crate.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `"tectonic"` on Unix, `"tectonic.exe"` on Windows.
pub fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// The repo's `src-tauri/binaries/<name>-<triple>[.exe]`, for tests. Tests run from
/// `target/debug/deps`, where no sidecar sits beside them, so the real-program tests
/// (`cargo test -- --ignored`) look here after `locate` comes up empty. Never used by the app.
pub fn in_repo_binaries(name: &str, repo_root: &Path) -> Option<PathBuf> {
    let prefix = format!("{name}-");
    std::fs::read_dir(repo_root.join("src-tauri").join("binaries"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_name_adds_the_suffix_only_on_windows() {
        assert_eq!(exe_name("x"), if cfg!(windows) { "x.exe" } else { "x" });
    }

    #[test]
    fn env_override_wins_when_it_names_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-program");
        std::fs::write(&fake, b"").unwrap();
        // Each test picks its own variable name so they cannot race on a shared one.
        std::env::set_var("ABSTRACT_TEX_SIDECAR_TEST_A", &fake);
        let found = locate("nothing-called-this-exists", "ABSTRACT_TEX_SIDECAR_TEST_A");
        std::env::remove_var("ABSTRACT_TEX_SIDECAR_TEST_A");
        assert_eq!(
            found,
            Some(Found {
                path: fake,
                source: Source::EnvOverride
            })
        );
    }

    #[test]
    fn env_override_pointing_nowhere_is_ignored_not_fatal() {
        std::env::set_var("ABSTRACT_TEX_SIDECAR_TEST_B", "/definitely/not/here");
        let found = locate("nothing-called-this-exists", "ABSTRACT_TEX_SIDECAR_TEST_B");
        std::env::remove_var("ABSTRACT_TEX_SIDECAR_TEST_B");
        assert_eq!(found, None);
    }

    #[test]
    fn in_repo_binaries_matches_by_prefix() {
        let repo = tempfile::tempdir().unwrap();
        let bin = repo.path().join("src-tauri").join("binaries");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("texlab-x86_64-unknown-linux-gnu"), b"").unwrap();
        std::fs::write(bin.join("tectonic-x86_64-unknown-linux-gnu"), b"").unwrap();
        let found = in_repo_binaries("texlab", repo.path()).unwrap();
        assert!(found.ends_with("texlab-x86_64-unknown-linux-gnu"));
        assert_eq!(in_repo_binaries("biber", repo.path()), None);
    }
}
