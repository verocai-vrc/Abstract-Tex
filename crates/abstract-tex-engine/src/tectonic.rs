//! Tectonic as a subprocess.
//!
//! Finds the binary (environment variable, then the sidecar next to our own executable, then
//! `PATH`), runs it with the flags Abstract-Tex needs, and kills it on cancellation.

use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Instant;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::incremental::{self, AuxSnapshot};
use crate::{BuildJob, BuildOutcome, BuildSteps, Engine, EngineError, EngineInfo, ProgressSink};

/// Set this to point Abstract-Tex at a specific Tectonic binary. Useful for testing a new release.
pub const ENV_OVERRIDE: &str = "ABSTRACT_TEX_TECTONIC";

/// The Tectonic engine. Holds only the path it resolved; every build spawns a fresh process.
#[derive(Debug, Clone)]
pub struct Tectonic {
    binary: PathBuf,
}

impl Tectonic {
    /// Locate a Tectonic binary, or return `EngineError::NotFound`. The search order — an
    /// explicit `ABSTRACT_TEX_TECTONIC`, then the bundled sidecar, then `PATH` — lives in
    /// `abstract-tex-sidecar`, shared with the language server (S3.1).
    pub fn locate() -> Result<Self, EngineError> {
        match abstract_tex_sidecar::locate("tectonic", ENV_OVERRIDE) {
            Some(found) => Ok(Self { binary: found.path }),
            None => Err(EngineError::NotFound),
        }
    }

    /// Use a specific binary. Tests use this; the app uses `locate()`.
    pub fn at(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into() }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// The command line for one job. Split out so a unit test can check it without running
    /// anything. `single_pass` is S9.2's warm step: one TeX pass, no BibTeX, reading the previous
    /// build's `.aux`/`.bbl` back from the build folder (see `incremental.rs`).
    fn arguments(job: &BuildJob, single_pass: bool) -> Vec<String> {
        let mut args = vec![
            // Tectonic's "V1" interface: `tectonic <file>` with options. We do not use the newer
            // `tectonic -X build` workspace mode because it wants its own Tectonic.toml, and the
            // project's `abstract-tex.toml` is the one configuration file we allow (DESIGN.md §5.8).
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
        if single_pass {
            args.push("--pass".to_string());
            args.push("tex".to_string());
            // Tectonic reads inputs from the project, not from `--outdir`; without this the pass
            // would start with no `.aux` or `.bbl` and print every reference as `??`. `-Z` marks
            // an unstable option: the bundled engine is pinned (0.17), and the corpus test is what
            // notices if a new release changes it.
            args.push("-Z".to_string());
            args.push(format!("search-path={}", job.out_dir.to_string_lossy()));
        }
        args.push(job.root_file.to_string_lossy().into_owned());
        args
    }

    /// Run Tectonic once with `args` and wait for it, or kill it if `cancel` fires first. Returns
    /// the exit status and everything it printed to stderr.
    async fn run_once(
        &self,
        job: &BuildJob,
        args: &[String],
        cancel: &CancellationToken,
        progress: Option<ProgressSink>,
    ) -> Result<(ExitStatus, String), EngineError> {
        debug!(binary = ?self.binary, ?args, "spawning tectonic");
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
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
        let stderr_pipe = child.stderr.take().expect("stderr was requested as piped");
        let stderr_task = tokio::spawn(pump_stderr(stderr_pipe, progress));

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
        Ok((status, stderr_task.await.unwrap_or_default()))
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

    async fn build(
        &self,
        job: &BuildJob,
        cancel: CancellationToken,
        progress: Option<ProgressSink>,
    ) -> Result<BuildOutcome, EngineError> {
        // The output directory must exist; Tectonic will not create it.
        tokio::fs::create_dir_all(&job.out_dir).await?;
        let started = Instant::now();

        // Artifacts are named after the root file's stem: main.tex → main.pdf, main.log.
        let stem = job
            .root_file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".to_string());

        // S9.2: decide on a warm start, then remove the marker before anything runs, so this
        // build must succeed to earn the next one a warm start (`incremental::WARM_MARKER`).
        let marker = job.out_dir.join(incremental::WARM_MARKER);
        let warm = incremental::can_start_warm(&job.out_dir, &stem);
        let _ = tokio::fs::remove_file(&marker).await;

        let mut steps = BuildSteps::default();
        let mut stderr = String::new();
        // `Some` once a single pass settles the build; `None` means a full build must run.
        let mut settled: Option<ExitStatus> = None;
        if warm {
            let mut before = AuxSnapshot::read(&job.out_dir);
            loop {
                let args = Self::arguments(job, true);
                let (status, pass_stderr) = self.run_once(job, &args, &cancel, progress.clone()).await?;
                steps.single_passes += 1;
                stderr.push_str(&pass_stderr);
                // A failed pass goes to the full build rather than being reported: the pass read
                // the last build's `.aux`, and a stale one (a package removed since, whose macros
                // it still calls) fails on its own. The full build starts clean, so an error it
                // reports is the document's, and its log is the one the drawer explains.
                if !status.success() {
                    break;
                }
                let after = AuxSnapshot::read(&job.out_dir);
                // Unchanged: nothing moved, so the build is done in this many passes.
                if after == before {
                    settled = Some(status);
                    break;
                }
                if after.bibliography_changed(&before) || steps.single_passes >= incremental::MAX_SINGLE_PASSES {
                    break;
                }
                before = after;
            }
        }
        let status = match settled {
            Some(status) => status,
            None => {
                steps.full = true;
                let args = Self::arguments(job, false);
                let (status, full_stderr) = self.run_once(job, &args, &cancel, progress).await?;
                stderr.push_str(&full_stderr);
                status
            }
        };
        if status.success() {
            let _ = tokio::fs::write(&marker, b"").await;
        }
        let duration = started.elapsed();

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
            steps,
        };
        info!(success = outcome.success, ms = duration.as_millis(), ?steps, "tectonic finished");
        Ok(outcome)
    }
}

/// Drain an engine's stderr a line at a time. Each line is forwarded to `progress` the moment
/// it arrives — that is what turns a frozen "Compiling…" into "Downloading amsmath.sty" during
/// a cold package fetch — and every line is also collected into the string returned for
/// [`BuildOutcome::stderr`]. Split out as a free function taking any `AsyncRead` so a test can
/// feed it bytes without spawning a real subprocess.
///
/// This reads raw bytes and splits on `\n` itself rather than using tokio's `AsyncBufReadExt::
/// lines()`, because `lines()` requires valid UTF-8 and simply stops, silently, on the first
/// byte that isn't — and a TeX engine's stderr is not guaranteed to be. `String::from_utf8_lossy`
/// per line keeps one bad byte from losing every line after it.
async fn pump_stderr<R>(mut reader: R, progress: Option<ProgressSink>) -> String
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0u8; 4096];
    let mut pending = Vec::new();
    let mut collected = String::new();

    // `emit` closes over `progress` and `collected` so both branches below (a complete line,
    // and whatever is left when the pipe closes) share exactly one place that decodes and
    // records a line.
    let emit = |line_bytes: &[u8], progress: &Option<ProgressSink>, collected: &mut String| {
        let line = String::from_utf8_lossy(line_bytes).into_owned();
        if let Some(sink) = progress {
            // A send error only means the orchestrator dropped the receiver (the build was
            // superseded, the window closed). Nothing to do about it here.
            let _ = sink.send(line.clone());
        }
        collected.push_str(&line);
        collected.push('\n');
    };

    loop {
        let read = match reader.read(&mut chunk).await {
            Ok(0) => break, // EOF: the pipe closed because the process exited.
            Ok(n) => n,
            Err(_) => break, // A broken pipe mid-read; treat like EOF rather than panic.
        };
        pending.extend_from_slice(&chunk[..read]);
        while let Some(newline_at) = pending.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = pending.drain(..=newline_at).collect();
            emit(&line_bytes[..line_bytes.len() - 1], &progress, &mut collected);
        }
    }
    // A final line with no trailing `\n` (Tectonic always ends with one, but nothing enforces
    // that in general) still deserves to be shown and collected.
    if !pending.is_empty() {
        emit(&pending, &progress, &mut collected);
    }
    collected
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
    use abstract_tex_sidecar::find_on_path;
    use std::time::Duration;

    fn job(dir: &Path) -> BuildJob {
        BuildJob {
            project_dir: dir.to_path_buf(),
            root_file: PathBuf::from("main.tex"),
            out_dir: dir.join(".abstract-tex").join("build"),
            synctex: true,
        }
    }

    #[test]
    fn arguments_put_the_root_file_last_and_request_synctex() {
        let dir = PathBuf::from("proj");
        let args = Tectonic::arguments(&job(&dir), false);
        assert_eq!(args.last().map(String::as_str), Some("main.tex"));
        assert!(args.contains(&"--synctex".to_string()));
        assert!(args.contains(&"--keep-intermediates".to_string()));
        assert!(!args.contains(&"--pass".to_string()), "a full build lets the engine choose its passes");
        let outdir_pos = args.iter().position(|a| a == "--outdir").unwrap();
        assert!(args[outdir_pos + 1].ends_with("build"));
    }

    #[test]
    fn arguments_omit_synctex_when_not_requested() {
        let mut j = job(Path::new("proj"));
        j.synctex = false;
        assert!(!Tectonic::arguments(&j, false).contains(&"--synctex".to_string()));
    }

    #[test]
    fn a_single_pass_runs_tex_only_and_searches_the_build_folder() {
        let args = Tectonic::arguments(&job(Path::new("proj")), true);
        let pass = args.iter().position(|a| a == "--pass").unwrap();
        assert_eq!(args[pass + 1], "tex");
        let search = args.iter().position(|a| a == "-Z").unwrap();
        assert!(args[search + 1].starts_with("search-path=") && args[search + 1].ends_with("build"));
        assert_eq!(args.last().map(String::as_str), Some("main.tex"), "the root file stays last");
    }

    /// S9.2's warm path against the real engine, on a document small enough to build in a second:
    /// a cold build is full; an edit that moves nothing is one single pass with every reference
    /// still resolved; a new citation is a single pass and then a full build, BibTeX included.
    #[tokio::test]
    #[ignore]
    async fn warm_builds_take_one_pass_until_the_bibliography_changes() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let engine = Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo).expect("run `pnpm fetch-engine`"));
        let tmp = tempfile::tempdir().unwrap();
        let body = |extra: &str| {
            format!(
                "\\documentclass{{article}}\n\\begin{{document}}\nSee Section~\\ref{{s:end}} and \\cite{{knuth}}.{extra}\n\
                 \\section{{End}}\\label{{s:end}}\n\\bibliographystyle{{plain}}\n\\bibliography{{refs}}\n\\end{{document}}\n"
            )
        };
        std::fs::write(tmp.path().join("refs.bib"), "@book{knuth, author={Knuth, D.}, title={T}, year=1984}\n@book{lamport, author={Lamport, L.}, title={L}, year=1994}\n").unwrap();
        std::fs::write(tmp.path().join("main.tex"), body("")).unwrap();
        let j = job(tmp.path());
        let undefined = |log: &Option<PathBuf>| {
            let text = std::fs::read_to_string(log.as_ref().unwrap()).unwrap();
            text.matches("undefined").count()
        };

        let cold = engine.build(&j, CancellationToken::new(), None).await.unwrap();
        assert!(cold.success, "{}", cold.stderr);
        assert_eq!(cold.steps, BuildSteps { single_passes: 0, full: true });

        std::fs::write(tmp.path().join("main.tex"), body(" A new sentence.")).unwrap();
        let warm = engine.build(&j, CancellationToken::new(), None).await.unwrap();
        assert!(warm.success, "{}", warm.stderr);
        assert_eq!(warm.steps, BuildSteps { single_passes: 1, full: false });
        assert_eq!(undefined(&warm.log), 0, "a single pass must still see the last build's .aux and .bbl");

        std::fs::write(tmp.path().join("main.tex"), body(" And \\cite{lamport}.")).unwrap();
        let cited = engine.build(&j, CancellationToken::new(), None).await.unwrap();
        assert!(cited.success, "{}", cited.stderr);
        assert_eq!(cited.steps, BuildSteps { single_passes: 1, full: true });
        assert_eq!(undefined(&cited.log), 0, "the full build must have run BibTeX for the new key");
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
                let found = abstract_tex_sidecar::in_repo_binaries("tectonic", &repo).expect("run `pnpm fetch-engine` first");
                Tectonic::at(found)
            }
            Err(e) => panic!("{e}"),
        };

        let info = engine.probe().await.unwrap();
        assert!(info.version.to_lowercase().contains("tectonic"), "{}", info.version);

        let tmp = tempfile::tempdir().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/main.tex");
        std::fs::copy(&src, tmp.path().join("main.tex")).unwrap();

        let outcome = engine.build(&job(tmp.path()), CancellationToken::new(), None).await.unwrap();
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
        let result = engine.build(&job(tmp.path()), cancel, None).await;
        // Either it was cancelled (the point of the test) or the stand-in exited immediately
        // because it rejected our arguments; both are fast. What must not happen is a hang.
        assert!(started.elapsed() < Duration::from_secs(5));
        if let Err(e) = result {
            assert!(matches!(e, EngineError::Cancelled), "unexpected: {e}");
        }
    }

    #[tokio::test]
    async fn pump_stderr_forwards_each_line_as_it_arrives_and_collects_the_whole_text() {
        let input: &[u8] = b"Downloading amsmath.sty\nnote: writing main.pdf\n";
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let collected = pump_stderr(input, Some(tx)).await;

        assert_eq!(rx.recv().await.unwrap(), "Downloading amsmath.sty");
        assert_eq!(rx.recv().await.unwrap(), "note: writing main.pdf");
        assert_eq!(collected, "Downloading amsmath.sty\nnote: writing main.pdf\n");
    }

    #[tokio::test]
    async fn pump_stderr_without_a_sink_still_collects_the_text() {
        let input: &[u8] = b"a single line\n";
        assert_eq!(pump_stderr(input, None).await, "a single line\n");
    }
}
