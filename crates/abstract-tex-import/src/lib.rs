//! Opening what people already have (S17.8). Someone picks a file; this crate says what it is, which
//! folder its document is in, and which folders are never one document.
//!
//! **What it must never do:** write anything, know about Tauri or a window, or decide what is said. It
//! answers with data; the words are the app's.

pub mod choose;
pub mod crowd;

pub use choose::{document_folder, has_tex, kind_of, FileKind};
pub use crowd::Crowd;
