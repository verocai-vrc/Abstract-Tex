//! Fetches a bibliography entry from a publisher's own identifier — no API key, no cloud service
//! of ours in the loop (DESIGN.md §5.4: "the entry appears, deduplicated"). Behind the `acquire`
//! Cargo feature, so the parser (this crate's real reason to exist, and the half that gets
//! published alone at S8.5) does not carry `reqwest` for a caller who never asks for it.
//!
//! [`doi`] is the first source (S7.4); arXiv and ISBN (S7.5) will sit beside it here, each
//! normalising whatever the author pasted and handing back a [`crate::Entry`] parsed from the
//! publisher's own BibTeX reply, so there is no second entry format to keep in sync with `parse`.

pub mod doi;
