//! Running one engine process: spawn it in the project folder, stream its stderr, wait for it,
//! or kill it the moment the build is cancelled. Shared by every engine (`tectonic.rs`,
//! `latexmk.rs`, S9.4), which differ only in the command line they build.
//!
//! It must never decide *what* to run — no engine flags, no knowledge of TeX — only how.

use std::path::Path;
use std::process::{ExitStatus, Stdio};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{EngineError, ProgressSink};

/// Run `binary` with `args` in `project_dir` and wait for it, or kill it if `cancel` fires first.
/// Returns the exit status and everything it printed to stderr.
pub async fn run(
    binary: &Path,
    project_dir: &Path,
    args: &[String],
    cancel: &CancellationToken,
    progress: Option<ProgressSink>,
) -> Result<(ExitStatus, String), EngineError> {
    debug!(?binary, ?args, "spawning engine");
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        // If our process dies, take the child with it. Never leave a stray engine running.
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
            debug!("build cancelled; killing the engine");
            let _ = child.kill().await;
            return Err(EngineError::Cancelled);
        }
    };
    Ok((status, stderr_task.await.unwrap_or_default()))
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
