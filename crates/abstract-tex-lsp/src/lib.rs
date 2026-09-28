//! The language server as a subprocess (DESIGN.md §4.1, "split the LSP client across the
//! boundary"): Rust owns TexLab's process lifecycle and its stdio pipe; TypeScript owns turning
//! the protocol into CodeMirror behaviour; a bridge over Tauri events joins them (S3.2).
//!
//! **Sprint 3 status (S3.2).** Three layers, bottom up: `transport` cuts frames out of a byte
//! stream, `server` owns the process and moves whole frames, and `bridge` speaks JSON-RPC over
//! it — request ids and response correlation, notifications, and restarting a server that dies.
//! What a method *means* is still not this crate's business: the frontend turns protocol into
//! CodeMirror behaviour (DESIGN.md §4.1).
//!
//! It must never touch the editor or the filesystem beyond spawning the process, and must
//! never depend on Tauri, so `cargo test -p abstract-tex-lsp` runs with no window open.

pub mod bridge;
pub mod server;
pub mod transport;

pub use bridge::{Bridge, CallError, Incoming, RpcError};
pub use server::{Running, TexLab, ENV_OVERRIDE};
pub use transport::{encode, FrameReader};

/// Why the language server could not be used. `thiserror` derives `Display` and `Error` from
/// the `#[error]` lines, the same way `abstract-tex-engine` does for its errors.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("no TexLab binary found (set ABSTRACT_TEX_TEXLAB, or run `pnpm fetch-lsp`)")]
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
