//! Parses a BibTeX or BibLaTeX `.bib` file into the items it is made of — entries, `@string`
//! macros, `@preamble`, `@comment`, and the free text between them — with a byte span on every
//! one, so a caller can change one field and leave every other byte of the file alone.
//!
//! Start at [`parse()`]. It never fails: a malformed item becomes a [`ParseError`] item that says
//! what went wrong and where, parsing resumes at the next `@`, and every other item is still
//! there. The `.bib` file is the author's, often shared with Zotero or JabRef, so this crate
//! keeps what it finds rather than normalising it: type and field names are stored as written
//! and compared case-insensitively; values keep their written form ([`Value`]: braced, quoted,
//! a number, a macro name, or several joined with `#`) and are resolved on request through
//! [`Bibliography::resolve`].
//!
//! **This crate must never read a file, make a network request, or know about the editor.**
//! Text in, data out, the same rule `texlog` follows, and for the same reason: it is published
//! on its own under MIT (S8.5), and it must be usable from a script or a CI check as easily as
//! from the app. The one planned exception is the `acquire` feature (S7.4–S7.5, DOI/arXiv/ISBN
//! lookups), which will sit behind a Cargo feature so the parser itself stays dependency-free.
//!
//! BibLaTeX is the same syntax as BibTeX with more entry types (`@online`, `@set`, `@xdata`)
//! and field names (`date`, `journaltitle`), so it costs nothing here. What BibLaTeX *means*
//! by `crossref`, `xdata` or `ids` — inheritance between entries — is not this crate's business
//! yet; the entries and fields come through as written.

#![warn(missing_docs)]

pub mod parse;
pub mod value;

pub use parse::{parse, Bibliography, Comment, Entry, Field, Item, ParseError, Preamble, Span, StringDef};
pub use value::{Value, ValuePart};
