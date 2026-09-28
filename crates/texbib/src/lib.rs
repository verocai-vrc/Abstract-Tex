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
//! Around the parser, and built only on its output:
//!
//! - [`render_entry`] and [`append_entry`] write an entry back as text, one field per line, and
//!   append it after a file's last item without touching any other byte;
//! - [`unique_key`] makes an `authorYEARword` citation key that does not collide with a list of
//!   existing ones;
//! - [`health::check`] finds missing required fields and page ranges written with a hyphen
//!   instead of an en-dash.
//!
//! **The parser must never read a file, make a network request, or know about an editor.** Text
//! in, data out, the same rule the sibling crate `texlog` follows, so it is as usable from a
//! script or a CI check as from a GUI. The one exception is opt-in: the `acquire` Cargo feature
//! adds an `acquire` module that fetches an entry from a DOI (doi.org content negotiation), an
//! arXiv id (the arXiv API) or an ISBN (OpenLibrary), says which of the three a pasted string is,
//! and talks to a local Zotero through Better BibTeX's JSON-RPC endpoint. It is named in a plain
//! code span, not linked, because the module does not exist in a build with the feature off and a
//! broken intra-doc link would fail `cargo doc` for exactly that build.
//!
//! BibLaTeX is the same syntax as BibTeX with more entry types (`@online`, `@set`, `@xdata`)
//! and field names (`date`, `journaltitle`), so it costs nothing here. What BibLaTeX *means*
//! by `crossref`, `xdata` or `ids` — inheritance between entries — is not this crate's business
//! yet; the entries and fields come through as written.

#![warn(missing_docs)]

#[cfg(feature = "acquire")]
pub mod acquire;
pub mod health;
pub mod keys;
pub mod parse;
pub mod render;
pub mod value;

pub use keys::unique_key;
pub use parse::{parse, Bibliography, Comment, Entry, Field, Item, ParseError, Preamble, Span, StringDef};
pub use render::{append_entry, render_entry};
pub use value::{Value, ValuePart};
