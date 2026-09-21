//! Fetches a bibliography entry from a publisher's own identifier — no API key, no cloud service
//! of ours in the loop (DESIGN.md §5.4: "the entry appears, deduplicated"). Behind the `acquire`
//! Cargo feature, so the parser (this crate's real reason to exist, and the half that gets
//! published alone at S8.5) does not carry `reqwest` for a caller who never asks for it.
//!
//! [`doi`] is the first source (S7.4); [`arxiv`] and [`isbn`] (S7.5) sit beside it, each
//! normalising whatever the author pasted and handing back a [`crate::Entry`]. `doi` gets its
//! entry the same way `parse` would — real BibTeX text, from the publisher, through
//! [`crate::parse()`] — so it inherits real byte spans; `arxiv` and `isbn` build their entry by
//! hand from a JSON/Atom reply that is not BibTeX at all, so their entries carry `0..0` spans
//! everywhere (see `arxiv`'s `zero_span` for why) until something writes them out as text for
//! the first time (S7.6). [`identify()`] is what a paste-to-cite caller (S7.6) asks first, to
//! learn which of the three — or none — a pasted string names.

pub mod arxiv;
pub mod doi;
pub mod identify;
pub mod isbn;

pub use identify::{identify, Identified};
