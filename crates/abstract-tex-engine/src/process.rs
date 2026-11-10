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

/// Run `binary` with `args` in `working_dir` and wait for it, or kill it if `cancel` fires
/// first. Returns the exit status and everything it printed to stderr. Its stdout is read only
/// for progress (see below) and is not returned.
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
    hide_console_window(&mut cmd);

    let mut child = cmd.spawn().map_err(EngineError::Spawn)?;

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

    // `select!` waits on two futures at once and runs the branch of whichever finishes
    // first. Here: either the engine exits, or the orchestrator cancels us. This is the
    // whole reason for choosing a subprocess: cancellation is one `kill()`.
    let status = tokio::select! {
        status = child.wait() => status?,
        _ = cancel.cancelled() => {
            debug!("build cancelled; killing the engine");
            let _ = child.kill().await;
            return Err(EngineError::Cancelled);
        }
    };
    let _ = stdout_task.await; // let the last of its lines reach the status bar before returning
    Ok((status, stderr_task.await.unwrap_or_default()))
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
    let mut collected = String::new();

    // `emit` closes over `progress` and `collected` so both branches below (a complete line,
    // and whatever is left when the pipe closes) share exactly one place that decodes and
    // records a line.
    let emit = |line_bytes: &[u8], progress: &Option<ProgressSink>, collected: &mut String| {
        let line = String::from_utf8_lossy(line_bytes).into_owned();
        if let (Some(sink), Some(message)) = (progress, progress_from_line(&line)) {
            // A send error only means the orchestrator dropped the receiver (the build was
            // superseded, the window closed). Nothing to do about it here.
            let _ = sink.send(message);
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
}
