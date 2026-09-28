//! Engine abstraction: everything Abstract-Tex needs to know about a TeX engine, and nothing else.
//!
//! This crate owns:
//! - the [`Engine`] trait every engine implements (`probe`, `build`),
//! - the plain data types a build takes and produces ([`BuildJob`], [`BuildOutcome`]),
//! - the one concrete engine we ship today, [`tectonic::Tectonic`].
//!
//! It must never:
//! - know about Tauri, windows or events (that is the app crate's job),
//! - parse the `.log` file beyond finding it (that is the `texlog` crate's job),
//! - link the Tectonic crate. DESIGN.md §4.1: we spawn a subprocess so that cancelling a build is
//!   `kill`, instant and clean, and a crashed engine cannot take the app down.
//!
//! # A note on error handling, once, for the whole workspace
//!
//! Library crates like this one define their own error *type* with `thiserror` (see
//! [`EngineError`]). Callers can `match` on the variants and react differently to "no engine
//! installed" than to "the build was cancelled". The application crate, at the very edge where
//! errors only get shown to a person, uses `anyhow`, which erases the type and keeps a message.
//! Typed on the inside, flattened at the edge.

pub mod incremental;
pub mod tectonic;

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

/// One line an engine printed to stderr while it ran, sent as soon as it arrives rather than
/// buffered until the build finishes. Tectonic prints package downloads here on a cold cache,
/// and DESIGN.md §6 requires that a fetch "is stated plainly with progress — never a silent
/// hang". A channel sender, rather than a plain callback, because the orchestrator wants to
/// `.clone()` it into a spawned task while `build()` keeps its own copy.
pub type ProgressSink = UnboundedSender<String>;

/// What `probe()` learns about an installed engine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineInfo {
    /// Human name, e.g. `"Tectonic"`.
    pub name: String,
    /// Whatever `--version` printed, trimmed.
    pub version: String,
    /// Where the binary lives. Shown in the UI so a user can see which engine ran.
    pub path: PathBuf,
}

/// Everything a build needs. Plain data: the orchestrator constructs one per compile.
#[derive(Debug, Clone)]
pub struct BuildJob {
    /// The project folder. The engine runs with this as its working directory so relative
    /// `\input{sections/intro}` paths resolve the way the author expects.
    pub project_dir: PathBuf,
    /// The root `.tex` file, relative to `project_dir`.
    pub root_file: PathBuf,
    /// Where artifacts go: `.abstract-tex/build/` by convention (DESIGN.md §5.8). Never the source
    /// tree. Never cleaned between builds, because `.aux` files are the warm cache.
    pub out_dir: PathBuf,
    /// Emit a `.synctex.gz` so the PDF pane can map clicks back to source lines (sprint 3).
    pub synctex: bool,
}

/// What a finished build produced. `success == false` is a normal outcome, not an error:
/// a document with a typo still *ran*; it just did not produce a PDF the author wants.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildOutcome {
    pub success: bool,
    /// Present if the engine left a PDF behind, even on failure (an earlier page may be usable).
    pub pdf: Option<PathBuf>,
    /// The `.log` file. This is the input to the `texlog` crate, never something we show raw.
    pub log: Option<PathBuf>,
    pub synctex: Option<PathBuf>,
    /// Engine's stderr. Tectonic prints package downloads and a summary here.
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    /// How the engine got there (S9.2): what the speed work is measured by, and what a slow
    /// build's log line explains.
    pub steps: BuildSteps,
}

/// The work one build did. A cold build is `full` and nothing else; a warm build whose edit
/// moved nothing is one single pass; one that added a citation is a single pass and then full.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct BuildSteps {
    /// TeX passes run alone, reading the previous build's `.aux` and `.bbl` (`incremental.rs`).
    pub single_passes: u32,
    /// Whether the engine's own full build ran: every TeX pass it decides on, BibTeX included.
    pub full: bool,
}

/// Why a build could not run at all. Contrast with `BuildOutcome { success: false }`, which
/// means it ran and TeX rejected the document.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("no TeX engine binary was found")]
    NotFound,

    #[error("the engine could not be started: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("the build was cancelled")]
    Cancelled,

    #[error("i/o error while building: {0}")]
    Io(#[from] std::io::Error),
}

/// One interface for every engine: bundled Tectonic today, a system `latexmk` from v0.5.
///
/// `async fn` inside a trait needs a small explanation. Rust supports `async fn` in traits
/// natively, but not when the trait is used through a `Box<dyn Engine>` (a "trait object",
/// i.e. "some engine, decided at run time"). The orchestrator wants exactly that, so we use the
/// `async_trait` macro, which rewrites each method to return a boxed future behind the scenes.
/// Both the trait and every `impl` carry the attribute.
#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    /// Is this engine available on this machine, and which version?
    async fn probe(&self) -> Result<EngineInfo, EngineError>;

    /// Run one build. Returns when the engine exits or `cancel` fires, whichever is first.
    /// On cancellation the child process is killed and `EngineError::Cancelled` is returned.
    ///
    /// Each line the engine writes to stderr is sent on `progress` as it arrives, if a sink is
    /// given. The same text still lands in [`BuildOutcome::stderr`] for the "raw output" view.
    /// `None` is for callers that do not surface progress (tests, a one-off probe build).
    async fn build(
        &self,
        job: &BuildJob,
        cancel: CancellationToken,
        progress: Option<ProgressSink>,
    ) -> Result<BuildOutcome, EngineError>;
}

