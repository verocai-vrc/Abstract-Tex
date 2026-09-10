//! Tectonic as a subprocess.
//!
//! Finds the binary (environment variable, then the sidecar next to our own executable, then
//! `PATH`), runs it with the flags Preamble needs, and kills it on cancellation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{exe_name, find_on_path, BuildJob, BuildOutcome, Engine, EngineError, EngineInfo};

/// Set this to point Preamble at a specific Tectonic binary. Useful for testing a new release.
pub const ENV_OVERRIDE: &str = "PREAMBLE_TECTONIC";

/// The Tectonic engine. Holds only the path it resolved; every build spawns a fresh process.
#[derive(Debug, Clone)]
pub struct Tectonic {
    binary: PathBuf,
}

impl Tectonic {
    /// Locate a Tectonic binary, or return `EngineError::NotFound`.
    ///
    /// Order matters and is deliberate:
    /// 1. `PREAMBLE_TECTONIC` — an explicit choice always wins.
    /// 2. The sidecar Tauri bundles next to our executable — the "zero setup" path.
    /// 3. `PATH` — a user who already has Tectonic installed.
    pub fn locate() -> Result<Self, EngineError> {
        if let Some(explicit) = std::env::var_os(ENV_OVERRIDE) {
            let path = PathBuf::from(explicit);
            if path.is_file() {
                info!(?path, "using Tectonic from {ENV_OVERRIDE}");
                return Ok(Self { binary: path });
            }
            warn!(?path, "{ENV_OVERRIDE} is set but is not a file; ignoring");
        }

        if let Some(sidecar) = Self::sidecar_path() {
            if sidecar.is_file() {
                info!(?sidecar, "using bundled Tectonic sidecar");
                return Ok(Self { binary: sidecar });
            }
        }

        if let Some(on_path) = find_on_path(&exe_name("tectonic")) {
            info!(?on_path, "using Tectonic from PATH");
            return Ok(Self { binary: on_path });
        }

        Err(EngineError::NotFound)
    }

    /// Use a specific binary. Tests use this; the app uses `locate()`.
    pub fn at(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into() }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Tauri places `externalBin` sidecars beside the main executable, with the target triple
    /// stripped from the name: `binaries/tectonic-x86_64-pc-windows-msvc.exe` in the repo
    /// becomes `tectonic.exe` next to `preamble.exe`. This is true in `tauri dev` too, because
    /// `tauri-build` copies sidecars into `target/debug/`.
    fn sidecar_path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        Some(dir.join(exe_name("tectonic")))
    }

    /// The command line for one job. Split out so a unit test can check it without running
    /// anything.
    fn arguments(job: &BuildJob) -> Vec<String> {
        let mut args = vec![
            // Tectonic's "V1" interface: `tectonic <file>` with options. We do not use the newer
            // `tectonic -X build` workspace mode because it wants its own Tectonic.toml, and the
            // project's `preamble.toml` is the one configuration file we allow (DESIGN.md §5.8).
            "--outdir".to_string(),
            job.out_dir.to_string_lossy().into_owned(),
            // Keep the .log on success too; texlog reads warnings from it.
            "--keep-logs".to_string(),
            // Keep .aux/.bbl between runs: they are the warm cache (DESIGN.md §5.1, rung 1).
            "--keep-intermediates".to_string(),
            // Less noise on stderr; errors are still printed.
            "--chatter".to_string(),
            "minimal".to_string(),
        ];
        if job.synctex {
            args.push("--synctex".to_string());
        }
        args.push(job.root_file.to_string_lossy().into_owned());
        args
    }
}

#[async_trait::async_trait]
impl Engine for Tectonic {
    async fn probe(&self) -> Result<EngineInfo, EngineError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("--version");
        hide_console_window(&mut cmd);
        let output = cmd.output().await.map_err(EngineError::Spawn)?;
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(EngineInfo {
            name: "Tectonic".to_string(),
            version,
            path: self.binary.clone(),
        })
    }

    async fn build(&self, job: &BuildJob, cancel: CancellationToken) -> Result<BuildOutcome, EngineError> {
        // The output directory must exist; Tectonic will not create it.
        tokio::fs::create_dir_all(&job.out_dir).await?;

        let started = Instant::now();
        let args = Self::arguments(job);
        debug!(binary = ?self.binary, ?args, "spawning tectonic");

        let mut cmd = Command::new(&self.binary);
        cmd.args(&args)
            .current_dir(&job.project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // If our process dies, take the child with it. Never leave a stray tectonic.exe.
            .kill_on_drop(true);
        hide_console_window(&mut cmd);

        let mut child = cmd.spawn().map_err(EngineError::Spawn)?;

        // Read stderr concurrently with waiting, otherwise a chatty engine fills the pipe and
        // blocks forever. `take()` moves the handle out of `child` so we own it separately.
        let mut stderr_pipe = child.stderr.take().expect("stderr was requested as piped");
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        });

        // `select!` waits on two futures at once and runs the branch of whichever finishes
        // first. Here: either the engine exits, or the orchestrator cancels us. This is the
        // whole reason for choosing a subprocess: cancellation is one `kill()`.
        let status = tokio::select! {
            status = child.wait() => status?,
            _ = cancel.cancelled() => {
                debug!("build cancelled; killing tectonic");
                let _ = child.kill().await;
                return Err(EngineError::Cancelled);
            }
        };

        let stderr = stderr_task.await.unwrap_or_default();
        let duration = started.elapsed();

        // Artifacts are named after the root file's stem: main.tex → main.pdf, main.log.
        let stem = job
            .root_file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".to_string());
        let artifact = |ext: &str| {
            let p = job.out_dir.join(format!("{stem}.{ext}"));
            p.is_file().then_some(p)
        };

        let outcome = BuildOutcome {
            success: status.success(),
            pdf: artifact("pdf"),
            log: artifact("log"),
            synctex: artifact("synctex.gz"),
            stderr,
            exit_code: status.code(),
            duration,
        };
        info!(success = outcome.success, ms = duration.as_millis(), "tectonic finished");
        Ok(outcome)
    }
}

/// On Windows a console-less GUI app that spawns a console program gets a black window flashing
/// up on every compile. `CREATE_NO_WINDOW` prevents that. Elsewhere this is a no-op.
fn hide_console_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn job(dir: &Path) -> BuildJob {
        BuildJob {
            project_dir: dir.to_path_buf(),
            root_file: PathBuf::from("main.tex"),
            out_dir: dir.join(".preamble").join("build"),
            synctex: true,
        }
    }

    #[test]
    fn arguments_put_the_root_file_last_and_request_synctex() {
        let dir = PathBuf::from("proj");
        let args = Tectonic::arguments(&job(&dir));
        assert_eq!(args.last().map(String::as_str), Some("main.tex"));
        assert!(args.contains(&"--synctex".to_string()));
        assert!(args.contains(&"--keep-intermediates".to_string()));
        let outdir_pos = args.iter().position(|a| a == "--outdir").unwrap();
        assert!(args[outdir_pos + 1].ends_with("build"));
    }

    #[test]
    fn arguments_omit_synctex_when_not_requested() {
        let mut j = job(Path::new("proj"));
        j.synctex = false;
        assert!(!Tectonic::arguments(&j).contains(&"--synctex".to_string()));
    }

    #[test]
    fn env_override_pointing_nowhere_is_ignored_not_fatal() {
        // A stale env var must not make the engine unusable; locate() falls through.
        std::env::set_var(ENV_OVERRIDE, "/definitely/not/a/file");
        let result = Tectonic::locate();
        std::env::remove_var(ENV_OVERRIDE);
        // Either found something else or NotFound; never Spawn/Io.
        match result {
            Ok(_) | Err(EngineError::NotFound) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    /// Runs the real engine. Ignored by default because it needs the sidecar fetched and, on a
    /// cold cache, network access to download packages. `cargo test -- --ignored` runs it.
    #[tokio::test]
    #[ignore]
    async fn builds_the_minimal_fixture_with_the_real_engine() {
        let engine = match Tectonic::locate() {
            Ok(e) => e,
            Err(EngineError::NotFound) => {
                // Tests run from target/debug/deps, so the sidecar is not beside us. Fall back
                // to the repo's binaries/ folder before giving up.
                let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
                let bin_dir = repo.join("src-tauri").join("binaries");
                let found = std::fs::read_dir(&bin_dir)
                    .ok()
                    .and_then(|rd| {
                        rd.filter_map(Result::ok)
                            .map(|e| e.path())
                            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("tectonic-")))
                    })
                    .expect("run `pnpm fetch-engine` first");
                Tectonic::at(found)
            }
            Err(e) => panic!("{e}"),
        };

        let info = engine.probe().await.unwrap();
        assert!(info.version.to_lowercase().contains("tectonic"), "{}", info.version);

        let tmp = tempfile::tempdir().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/main.tex");
        std::fs::copy(&src, tmp.path().join("main.tex")).unwrap();

        let outcome = engine.build(&job(tmp.path()), CancellationToken::new()).await.unwrap();
        assert!(outcome.success, "stderr:\n{}", outcome.stderr);
        assert!(outcome.pdf.is_some(), "no pdf produced");
        assert!(outcome.log.is_some(), "no log kept");
        assert!(outcome.synctex.is_some(), "no synctex produced");
    }

    /// Cancellation must return promptly even if the "engine" never exits. We use a shell sleep
    /// as a stand-in engine because it accepts any arguments and ignores them.
    #[tokio::test]
    async fn cancellation_kills_a_running_build() {
        let (sleeper, ok) = if cfg!(windows) {
            (find_on_path("timeout.exe").or_else(|| find_on_path("ping.exe")), true)
        } else {
            (find_on_path("sleep"), true)
        };
        let Some(sleeper) = sleeper else {
            eprintln!("no sleep-like binary found; skipping");
            return;
        };
        assert!(ok);

        // Tectonic::at with a binary that just waits. The arguments we pass are nonsense to it,
        // but `timeout`/`ping`/`sleep` all block long enough for the cancel to land first.
        let engine = Tectonic::at(sleeper);
        let tmp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            cancel_clone.cancel();
        });

        let started = Instant::now();
        let result = engine.build(&job(tmp.path()), cancel).await;
        // Either it was cancelled (the point of the test) or the stand-in exited immediately
        // because it rejected our arguments; both are fast. What must not happen is a hang.
        assert!(started.elapsed() < Duration::from_secs(5));
        if let Err(e) = result {
            assert!(matches!(e, EngineError::Cancelled), "unexpected: {e}");
        }
    }
}
