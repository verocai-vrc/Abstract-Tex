//! The language server as a subprocess (DESIGN.md §4.1, "split the LSP client across the
//! boundary"): Rust owns TexLab's process lifecycle and its stdio pipe; TypeScript owns turning
//! the protocol into CodeMirror behaviour; a bridge over Tauri events joins them (S3.2).
//!
//! **Sprint 3 status (S3.1): process and framing only.** This crate can find TexLab, start it,
//! send it a message and read the messages it sends back, one whole frame at a time. It does
//! not yet know what a request or a response *is* — request ids, correlation, notifications
//! and restart-on-crash are S3.2, in `bridge` when it exists — and it never looks inside a
//! frame's JSON.
//!
//! It must never touch the editor or the filesystem beyond spawning the process, and must
//! never depend on Tauri, so `cargo test -p preamble-lsp` runs with no window open.

pub mod server;
pub mod transport;

pub use server::{Running, TexLab, ENV_OVERRIDE};
pub use transport::{encode, FrameReader};

/// Why the language server could not be used. `thiserror` derives `Display` and `Error` from
/// the `#[error]` lines, the same way `preamble-engine` does for its errors.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("no TexLab binary found (set PREAMBLE_TEXLAB, or run `pnpm fetch-lsp`)")]
    NotFound,
    #[error("could not start TexLab: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("the language server closed its output")]
    Exited,
    #[error("malformed frame from the language server: {0}")]
    Protocol(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
