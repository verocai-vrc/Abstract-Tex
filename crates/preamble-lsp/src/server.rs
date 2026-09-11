//! TexLab as a subprocess: find it, start it, hold its pipes, stop it.
//!
//! Owns the process and nothing above it. `Running::send` and `Running::recv` move whole
//! frames; what the frames mean is S3.2's business. Dropping a `Running` kills the process —
//! a language server must never outlive the window that started it.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, info};

use crate::transport::{encode, FrameReader};
use crate::LspError;

/// Set this to point Preamble at a specific TexLab binary. Useful for testing a new release.
pub const ENV_OVERRIDE: &str = "PREAMBLE_TEXLAB";

/// The language server. Holds only the path it resolved; `spawn` starts a fresh process.
#[derive(Debug, Clone)]
pub struct TexLab {
    binary: PathBuf,
    /// Extra command-line arguments. Empty for the real TexLab, which needs none for stdio;
    /// tests use it to drive a stand-in process.
    args: Vec<String>,
}

impl TexLab {
    /// Locate a TexLab binary, or return `LspError::NotFound`. The search order — an explicit
    /// `PREAMBLE_TEXLAB`, then the bundled sidecar, then `PATH` — lives in `preamble-sidecar`,
    /// shared with the engine.
    pub fn locate() -> Result<Self, LspError> {
        match preamble_sidecar::locate("texlab", ENV_OVERRIDE) {
            Some(found) => Ok(Self::at(found.path)),
            None => Err(LspError::NotFound),
        }
    }

    /// Use a specific binary. Tests use this; the app uses `locate()`.
    pub fn at(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into(), args: Vec::new() }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Start the server with its stdin and stdout piped to us. `root` is the working directory;
    /// TexLab resolves relative paths against it. Stderr is inherited so anything the server
    /// prints about itself lands in our own log rather than vanishing.
    pub async fn spawn(&self, root: &Path) -> Result<Running, LspError> {
        let mut child = Command::new(&self.binary)
            .args(&self.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            // If the `Child` is dropped — the app closing, a panic — the process goes with it.
            .kill_on_drop(true)
            .spawn()
            .map_err(LspError::Spawn)?;
        // `take()` moves the pipe handles out of the `Child` so we can own them separately;
        // they are `Some` because we asked for `Stdio::piped()` above.
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        info!(binary = %self.binary.display(), pid = child.id(), "language server started");
        Ok(Running { child, stdin, frames: FrameReader::new(stdout) })
    }
}

/// A live language server process. One of these per open project.
pub struct Running {
    child: Child,
    stdin: ChildStdin,
    frames: FrameReader<ChildStdout>,
}

impl Running {
    /// The operating system's id for the process, while it is alive.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Write one message. `body` is the JSON; the header is added here.
    pub async fn send(&mut self, body: &[u8]) -> Result<(), LspError> {
        self.stdin.write_all(&encode(body)).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Read one message. `Err(LspError::Exited)` once the server has closed its output.
    pub async fn recv(&mut self) -> Result<Vec<u8>, LspError> {
        match self.frames.next().await? {
            Some(body) => Ok(body),
            None => Err(LspError::Exited),
        }
    }

    /// Has the process ended? Non-blocking; `Some(code)` if it has.
    pub fn exit_code(&mut self) -> Option<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            _ => None,
        }
    }

    /// Stop the process now. The polite `shutdown` request and `exit` notification are the
    /// bridge's job (S3.2); this is the hammer for when those did not work or there is no time.
    pub async fn kill(&mut self) -> Result<(), LspError> {
        debug!(pid = self.child.id(), "killing language server");
        self.child.kill().await?;
        Ok(())
    }
}
