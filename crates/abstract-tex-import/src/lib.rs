//! Opening what people already have (S17.8). Someone picks a file; this crate says what it is, which
//! folder its document is in, and which folders are never one document. A `.zip` it unpacks (S17.9), and a
//! document that is among other things it copies out (S17.10).
//!
//! **What it must never do:** write anywhere but in the folder it was told to make things in, overwrite
//! anything, know about Tauri or a window, or decide what is said. It answers with data; the words are
//! the app's.

pub mod choose;
pub mod crowd;
pub mod named_files;
pub mod staged;
pub mod unpack;

pub use choose::{document_folder, has_tex, kind_of, FileKind};
pub use crowd::Crowd;
pub use named_files::{copy_document, files_named_by, CopyError};
pub use staged::{free_name, Staged};
pub use unpack::{safe_path, unpack, Limits, Platform, UnpackError};
