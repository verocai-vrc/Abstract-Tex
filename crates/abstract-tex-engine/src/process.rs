//! Running one engine process: spawn it in a working folder, stream its stderr, wait for it,
//! or kill it the moment the build is cancelled. Shared by every engine (`tectonic.rs`,
//! `latexmk.rs`, S9.4), which differ only in the command line they build.
//!
//! It must never decide *what* to run — no engine flags, no knowledge of TeX — only how. That
//! includes the working folder and the environment: both arrive already decided (S9.12, where
//! `latexmk.rs` starts choosing between two working folders), and nothing here looks at them.

use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{EngineError, ProgressSink};

/// How long one build may run before it is stopped. Generous: a cold first build that downloads a
/// large package set takes a few minutes, and the point of the limit is a document that never
/// finishes, which used to hold the build lane until somebody cancelled it.
pub const BUILD_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// How much of an engine's output is kept: the last megabyte. A runaway `\loop` that prints used to
/// be collected whole, and the process grew until the build was cancelled.
pub(crate) const OUTPUT_KEPT_BYTES: usize = 1024 * 1024;

/// The longest single line kept; a longer one is split here, so a stream with no newline in it
/// cannot grow the read buffer without bound either.
const LINE_KEPT_BYTES: usize = 64 * 1024;

/// Run `binary` with `args` in `working_dir` and wait for it, or kill it if `cancel` fires
/// first or [`BUILD_TIME_LIMIT`] passes. Returns the exit status and what it printed to stderr
/// (the last [`OUTPUT_KEPT_BYTES`] of it). Its stdout is read only for progress (see below) and is
/// not returned.
///
/// `env` is added to this process's own environment rather than replacing it: a TeX engine needs
/// `PATH`, `HOME` and the distribution's own variables to work at all, so an engine only ever
/// names the handful it wants to set (S9.12's kpathsea search paths).
pub async fn run(
    binary: &Path,
    working_dir: &Path,
    args: &[String],
    env: &[(&str, OsString)],
    cancel: &CancellationToken,
    progress: Option<ProgressSink>,
) -> Result<(ExitStatus, String), EngineError> {
    run_with_limit(binary, working_dir, args, env, cancel, progress, BUILD_TIME_LIMIT).await
}

/// [`run`] with the time limit passed in, so a test does not have to wait fifteen minutes.
pub async fn run_with_limit(
    binary: &Path,
    working_dir: &Path,
    args: &[String],
    env: &[(&str, OsString)],
    cancel: &CancellationToken,
    progress: Option<ProgressSink>,
    limit: std::time::Duration,
) -> Result<(ExitStatus, String), EngineError> {
    debug!(?binary, ?args, ?working_dir, ?env, "spawning engine");
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(working_dir)
        .envs(env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // If our process dies, take the child with it. Never leave a stray engine running.
        .kill_on_drop(true);
    // Its own process group, so that the whole tree can be ended at once: `latexmk` starts pdflatex
    // and bibtex, and killing only `latexmk` left them running (review finding R4.7).
    #[cfg(unix)]
    cmd.process_group(0);
    hide_console_window(&mut cmd);

    let mut child = cmd.spawn().map_err(EngineError::Spawn)?;
    // Ends the whole tree if this future is dropped before the engine finishes (a task aborted by
    // a newer request), which `kill_on_drop` alone would do for the first process only.
    let mut tree = ProcessTree::of(&child);

    // Read stderr concurrently with waiting, otherwise a chatty engine fills the pipe and
    // blocks forever. `take()` moves the handle out of `child` so we own it separately.
    let stderr_pipe = child.stderr.take().expect("stderr was requested as piped");
    // stdout is piped too, and drained for the same reason: Tectonic's V1 interface prints its
    // `note: downloading …` lines there, and a pipe nobody reads would eventually block the
    // engine. Its text is only mined for progress; it is not added to the stderr that
    // `BuildOutcome::stderr` and the error fallback show. `UnboundedSender` is cheap to clone, so
    // both pumps can send to the same status line.
    let stdout_pipe = child.stdout.take().expect("stdout was requested as piped");
    let stdout_task = tokio::spawn(pump_stderr(stdout_pipe, progress.clone()));
    let stderr_task = tokio::spawn(pump_stderr(stderr_pipe, progress));

    // `select!` waits on several futures at once and runs the branch of whichever finishes
    // first. Here: the engine exits, or the orchestrator cancels us, or the time limit passes.
    // This is the whole reason for choosing a subprocess: cancellation is a kill.
    let status = tokio::select! {
        status = child.wait() => status?,
        _ = cancel.cancelled() => {
            debug!("build cancelled; killing the engine");
            tree.kill();
            let _ = child.kill().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(EngineError::Cancelled);
        }
        _ = tokio::time::sleep(limit) => {
            debug!(?limit, "build ran past its time limit; killing the engine");
            tree.kill();
            let _ = child.kill().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(EngineError::TimedOut(limit));
        }
    };
    tree.finished();
    let _ = stdout_task.await; // let the last of its lines reach the status bar before returning
    Ok((status, stderr_task.await.unwrap_or_default()))
}

/// The engine's process group, killed as a whole when asked to or when dropped unfinished.
struct ProcessTree {
    /// The group's id (the process id of the first process), or `None` once it is over or where
    /// there is no group to kill.
    group: Option<u32>,
}

impl ProcessTree {
    fn of(child: &tokio::process::Child) -> Self {
        Self { group: child.id() }
    }

    /// Kill every process in the group. Harmless if they are already gone.
    fn kill(&mut self) {
        if let Some(group) = self.group.take() {
            kill_group(group);
        }
    }

    /// The engine exited by itself: whatever it started is its own business again.
    fn finished(&mut self) {
        self.group = None;
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(unix)]
fn kill_group(group: u32) {
    // A negative pid names the process group. `unsafe` only because `kill` is a C function; it
    // takes two integers and touches no memory of ours.
    unsafe {
        libc::kill(-(group as i32), libc::SIGKILL);
    }
}

/// On Windows there are no process groups to signal; `taskkill /T` ends a process and its children.
#[cfg(windows)]
fn kill_group(group: u32) {
    let mut taskkill = std::process::Command::new("taskkill");
    taskkill.args(["/PID", &group.to_string(), "/T", "/F"]);
    {
        use std::os::windows::process::CommandExt;
        taskkill.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = taskkill.output();
}

/// What the status bar should say for one line of an engine's stderr, if anything. Tectonic
/// prints `note: downloading <file>` for every file it fetches from the network, which is the one
/// thing worth telling an author during a cold first build ("Compiling…" for a minute and a half
/// with nothing else reads as a hang). Every other line, notes about passes and warnings alike,
/// is not progress and says nothing here.
///
/// Returns an owned `String` because the line it was given is about to be dropped; `Option` is how
/// Rust says "maybe nothing", where other languages would use an empty string or a null.
pub fn progress_from_line(line: &str) -> Option<String> {
    let name = line.strip_prefix("note: downloading ")?.trim();
    if name.is_empty() {
        None
    } else {
        Some(format!("Downloading {name}"))
    }
}

/// Drain an engine's stderr a line at a time. A line that is progress (see [`progress_from_line`])
/// is forwarded to `progress` the moment it arrives — that is what turns a frozen "Compiling…"
/// into "Downloading amsmath.sty" during a cold package fetch — and every line, progress or not,
/// is also collected into the string returned for [`BuildOutcome::stderr`]. Split out as a free function taking any `AsyncRead` so a test can
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
    let mut kept = TailOfOutput::default();

    // `emit` closes over `progress` and `kept` so both branches below (a complete line,
    // and whatever is left when the pipe closes) share exactly one place that decodes and
    // records a line.
    let emit = |line_bytes: &[u8], progress: &Option<ProgressSink>, kept: &mut TailOfOutput| {
        let line = String::from_utf8_lossy(line_bytes).into_owned();
        if let (Some(sink), Some(message)) = (progress, progress_from_line(&line)) {
            // A send error only means the orchestrator dropped the receiver (the build was
            // superseded, the window closed). Nothing to do about it here.
            let _ = sink.send(message);
        }
        kept.push(line);
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
            emit(&line_bytes[..line_bytes.len() - 1], &progress, &mut kept);
        }
        // No newline in sight and already a long line: take what there is as a line of its own.
        while pending.len() >= LINE_KEPT_BYTES {
            let line_bytes: Vec<u8> = pending.drain(..LINE_KEPT_BYTES).collect();
            emit(&line_bytes, &progress, &mut kept);
        }
    }
    // A final line with no trailing `\n` (Tectonic always ends with one, but nothing enforces
    // that in general) still deserves to be shown and collected.
    if !pending.is_empty() {
        emit(&pending, &progress, &mut kept);
    }
    kept.finish()
}

/// The last [`OUTPUT_KEPT_BYTES`] of an engine's output, a line at a time. The *end* is kept
/// because that is where TeX says what went wrong; when earlier lines were dropped the text begins
/// with a line saying how many, so the cut is never silent.
#[derive(Default)]
struct TailOfOutput {
    lines: std::collections::VecDeque<String>,
    bytes: usize,
    dropped: usize,
}

impl TailOfOutput {
    fn push(&mut self, mut line: String) {
        line.push('\n');
        self.bytes += line.len();
        self.lines.push_back(line);
        while self.bytes > OUTPUT_KEPT_BYTES {
            match self.lines.pop_front() {
                Some(oldest) => {
                    self.bytes -= oldest.len();
                    self.dropped += 1;
                }
                None => break,
            }
        }
    }

    fn finish(self) -> String {
        let mut text = String::with_capacity(self.bytes + 64);
        if self.dropped > 0 {
            text.push_str(&format!("… {} earlier lines left out …\n", self.dropped));
        }
        for line in self.lines {
            text.push_str(&line);
        }
        text
    }
}

/// On Windows a console-less GUI app that spawns a console program gets a black window flashing
/// up on every compile. `CREATE_NO_WINDOW` prevents that. Elsewhere this is a no-op.
pub fn hide_console_window(cmd: &mut Command) {
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

    /// The lines are the ones Tectonic really prints on a cold cache (captured 10 Nov 2026 from
    /// the bundled binary), not invented ones: the first version of this test fed a line Tectonic
    /// never writes, which is how the status bar's "Downloading" suffix never worked.
    #[tokio::test]
    async fn pump_stderr_forwards_only_downloads_as_they_arrive_and_collects_the_whole_text() {
        let input: &[u8] = b"note: generating format \"latex\"\nnote: downloading latex.ltx\nnote: downloading cmr10.tfm\nwarning: failure fetching \"tuenc.def\" from network (1/3)\nnote: Running TeX ...\n";
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let collected = pump_stderr(input, Some(tx)).await;

        assert_eq!(rx.recv().await.unwrap(), "Downloading latex.ltx");
        assert_eq!(rx.recv().await.unwrap(), "Downloading cmr10.tfm");
        assert!(
            rx.try_recv().is_err(),
            "nothing else is worth a status bar's words"
        );
        assert_eq!(collected, String::from_utf8_lossy(input));
    }

    #[test]
    fn only_a_download_note_becomes_progress() {
        assert_eq!(
            progress_from_line("note: downloading amsmath.sty").as_deref(),
            Some("Downloading amsmath.sty")
        );
        assert_eq!(progress_from_line("note: Running TeX ..."), None);
        assert_eq!(progress_from_line("error: halted"), None);
        assert_eq!(
            progress_from_line("note: downloading "),
            None,
            "a note with no name says nothing"
        );
    }

    /// Tectonic's command-line (V1) interface, the one this app uses, writes its `note:` lines
    /// to *stdout*, not stderr; only the `-X` interface uses stderr. A fake engine that does what
    /// the real one does, run through the real `run`, so the pipe that was once thrown away
    /// (`Stdio::null()`) is actually exercised.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_download_note_on_stdout_reaches_progress_and_stdout_stays_out_of_stderr() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let directory = tempfile::tempdir().unwrap();
        let script = "echo 'note: downloading a.sty'; echo 'a real complaint' >&2";

        let (status, stderr) = run(
            Path::new("/bin/sh"),
            directory.path(),
            &["-c".to_string(), script.to_string()],
            &[],
            &CancellationToken::new(),
            Some(tx),
        )
        .await
        .unwrap();

        assert!(status.success());
        assert_eq!(rx.recv().await.unwrap(), "Downloading a.sty");
        assert_eq!(
            stderr, "a real complaint\n",
            "stdout's text is not part of the stderr string"
        );
    }

    #[tokio::test]
    async fn pump_stderr_without_a_sink_still_collects_the_text() {
        let input: &[u8] = b"a single line\n";
        assert_eq!(pump_stderr(input, None).await, "a single line\n");
    }

    // Review finding R4.7. `child.kill()` ends only the process that was started. `latexmk` starts
    // pdflatex and bibtex, which are not in its place: cancelled, they kept running and writing into
    // the build folder, and their open pipes kept the two output readers alive. The engine now runs
    // in a process group of its own and the whole group is killed.
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_ends_the_engines_children_as_well() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("grandchild.pid");
        let script = format!("sleep 60 & echo $! > '{}'; wait", pid_file.display());
        let cancel = CancellationToken::new();

        let running = {
            let (directory, cancel) = (directory.path().to_path_buf(), cancel.clone());
            tokio::spawn(async move {
                run(
                    Path::new("/bin/sh"),
                    &directory,
                    &["-c".to_string(), script],
                    &[],
                    &cancel,
                    None,
                )
                .await
            })
        };
        let grandchild: i32 = loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = text.trim().parse() {
                    break pid;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        cancel.cancel();
        assert!(matches!(running.await.unwrap(), Err(EngineError::Cancelled)));

        // The kernel needs a moment to reap it; poll rather than sleep a fixed time.
        let mut alive = true;
        for _ in 0..100 {
            alive = std::process::Command::new("kill")
                .args(["-0", &grandchild.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            if !alive {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(!alive, "the engine's child process {grandchild} is still running");
    }

    // The same finding: nothing limited how long a build could run, so a document that loops for ever
    // held the build lane until someone cancelled.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_build_that_runs_past_its_time_limit_is_stopped_and_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();

        let result = run_with_limit(
            Path::new("/bin/sh"),
            directory.path(),
            &["-c".to_string(), "sleep 60".to_string()],
            &[],
            &CancellationToken::new(),
            None,
            std::time::Duration::from_millis(300),
        )
        .await;

        assert!(matches!(result, Err(EngineError::TimedOut(_))), "{result:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let sentence = result.unwrap_err().to_string();
        assert!(sentence.contains("stopped"), "{sentence}");
    }

    // And the output: all of it was kept in memory, so a runaway `\loop` that prints grew the process
    // until the build was cancelled. The text kept is the *end* of the output, which is where the
    // error is.
    #[tokio::test]
    async fn an_engine_that_prints_without_end_keeps_only_the_last_of_it() {
        let mut lots = Vec::new();
        for n in 0..200_000 {
            lots.extend_from_slice(
                format!("line number {n} with some padding to make it longer\n").as_bytes(),
            );
        }
        let collected = pump_stderr(&lots[..], None).await;

        assert!(
            collected.len() <= OUTPUT_KEPT_BYTES + 200,
            "{} bytes kept",
            collected.len()
        );
        assert!(collected.ends_with("line number 199999 with some padding to make it longer\n"));
        assert!(
            collected.starts_with("… "),
            "it says that earlier lines were left out: {:?}",
            &collected[..60]
        );
    }

    #[tokio::test]
    async fn one_endless_line_does_not_grow_without_bound() {
        let endless = vec![b'x'; 3 * OUTPUT_KEPT_BYTES];
        let collected = pump_stderr(&endless[..], None).await;
        assert!(
            collected.len() <= OUTPUT_KEPT_BYTES + 200,
            "{} bytes kept",
            collected.len()
        );
        assert!(collected.contains('x'));
    }

    #[tokio::test]
    async fn short_output_is_kept_whole_and_unmarked() {
        let collected = pump_stderr(&b"one\ntwo\n"[..], None).await;
        assert_eq!(collected, "one\ntwo\n");
    }
}
