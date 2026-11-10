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
//!
//! [`zotero`] (S8.1) is a different shape from the other three: it answers "is Zotero, with
//! Better BibTeX, reachable on this machine" rather than fetching any one entry, and nothing
//! calls it during paste-to-cite — it is the detection step S8.2's collection linking builds on.

pub mod arxiv;
pub mod doi;
pub mod identify;
pub mod isbn;
pub mod zotero;

pub use identify::{identify, Identified};

/// The most a reply from a bibliography source may be before it is refused. A DOI record, an arXiv
/// entry or an Open Library page is a few kilobytes; `doi.org` redirects to a publisher's own server,
/// whose reply this crate does not control and used to read whole (review finding R3.16).
pub(crate) const REPLY_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

/// Read `reader` as text, giving up with a sentence if it holds more than `limit` bytes. `take`
/// stops the read one byte past the limit, which is how "too long" is told from "exactly the limit".
pub(crate) fn read_text_within(reader: impl std::io::Read, limit: u64) -> Result<String, String> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(
            "the reply was larger than any bibliography record should be, so it was not read".to_string(),
        );
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod reply_limit_tests {
    use super::*;

    #[test]
    fn a_reply_up_to_the_limit_is_read_whole_and_one_byte_over_is_refused() {
        assert_eq!(read_text_within(&b"12345"[..], 5).unwrap(), "12345");
        let error = read_text_within(&b"123456"[..], 5).unwrap_err();
        assert!(error.contains("larger than"), "{error}");
        assert_eq!(read_text_within(&b""[..], 5).unwrap(), "");
    }

    #[test]
    fn a_reply_that_is_not_utf8_is_still_text() {
        assert_eq!(
            read_text_within(&[0x66, 0xff, 0x6f][..], 10).unwrap(),
            "f\u{fffd}o"
        );
    }
}
