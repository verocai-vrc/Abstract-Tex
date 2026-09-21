//! Decides which of the three sources ([`crate::acquire::doi`], [`crate::acquire::arxiv`],
//! [`crate::acquire::isbn`]) a pasted string belongs to, or none — the question S7.6's
//! paste-to-cite asks before it calls any of them. Pure text matching, no network, so it runs
//! with the `acquire` feature off just as easily as on; only the three normalizers it calls
//! into live behind that feature, and only because they sit in this same module tree.

use super::arxiv::{normalize_arxiv_id, strip_version_suffix};
use super::doi::normalize_doi;
use super::isbn::normalize_isbn;

/// Which source a pasted string names, carrying the string already stripped of whatever wrapper
/// (a URL, a `doi:`/`arXiv:` scheme, dashes in an ISBN) its own source recognises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identified {
    /// A DOI, normalized the way [`crate::acquire::doi::normalize_doi`] does.
    Doi(String),
    /// An arXiv identifier, normalized the way
    /// [`crate::acquire::arxiv::normalize_arxiv_id`] does.
    Arxiv(String),
    /// A 10- or 13-digit ISBN, digits and a possible trailing `X` only.
    Isbn(String),
}

/// What `pasted` is, by trying each normalizer's own recognised shapes in turn. A DOI's `10.`
/// prefix and an arXiv id's `YYMM.NNNNN` shape cannot collide, and neither looks like a run of
/// 10 or 13 digits, so the order here only matters for which sentence a caller sees first when
/// nothing matches — it does not create ambiguity between the three.
///
/// A plain sentence, a bare title, or an empty paste all return `None`: this function only ever
/// says yes to something shaped like one of the three identifiers, the same "do not guess"
/// discipline [`crate::acquire::doi::normalize_doi`] already documents for its own prefixes.
pub fn identify(pasted: &str) -> Option<Identified> {
    let trimmed = pasted.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(doi) = looks_like_doi(trimmed) {
        return Some(Identified::Doi(doi));
    }
    if let Some(id) = looks_like_arxiv_id(trimmed) {
        return Some(Identified::Arxiv(id));
    }
    if let Some(isbn) = looks_like_isbn(trimmed) {
        return Some(Identified::Isbn(isbn));
    }
    None
}

/// A DOI always contains a `10.` registrant prefix somewhere in it, wrapped or bare — the one
/// structural fact [`crate::acquire::doi::normalize_doi`] does not itself check, since that
/// function only strips a wrapper and trusts doi.org to judge the rest. `identify` is the one
/// place that does judge, so a plain sentence is never sent to doi.org as if it might be one.
fn looks_like_doi(candidate: &str) -> Option<String> {
    let normalized = normalize_doi(candidate);
    normalized.starts_with("10.").then_some(normalized)
}

fn looks_like_arxiv_id(candidate: &str) -> Option<String> {
    let normalized = normalize_arxiv_id(candidate);
    is_arxiv_id_shape(&normalized).then_some(normalized)
}

/// `YYMM.NNNNN[vN]` (new-style, 2007 onward) or `archive/YYMMNNN` (old-style) — the two shapes
/// [`crate::acquire::arxiv::normalize_arxiv_id`]'s own doc comment names, the version suffix
/// included since `normalize_arxiv_id` deliberately keeps it rather than stripping it (only
/// `arxiv::fetch_arxiv_with`'s own `eprint`/`url` fields strip it, for the reason its comment
/// gives). Checked digit-by-digit rather than with a regex dependency, the same discipline the
/// rest of this crate holds to.
fn is_arxiv_id_shape(id: &str) -> bool {
    let without_version = strip_version_suffix(id);
    if let Some((yymm, rest)) = without_version.split_once('.') {
        return yymm.len() == 4
            && yymm.bytes().all(|b| b.is_ascii_digit())
            && !rest.is_empty()
            && rest.len() <= 5
            && rest.bytes().all(|b| b.is_ascii_digit());
    }
    if let Some((archive, digits)) = without_version.split_once('/') {
        return !archive.is_empty()
            && archive.bytes().all(|b| b.is_ascii_alphabetic() || b == b'.' || b == b'-')
            && digits.len() == 7
            && digits.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

/// A run of exactly 10 or 13 digits once hyphens/spaces are stripped, the last character of a
/// 10-digit ISBN allowed to be a literal `X` check digit (ISBN-10's own rule; ISBN-13 has none).
fn looks_like_isbn(candidate: &str) -> Option<String> {
    let normalized = normalize_isbn(candidate);
    let is_isbn_10 = normalized.len() == 10
        && normalized[..9].bytes().all(|b| b.is_ascii_digit())
        && matches!(normalized.as_bytes()[9], b'0'..=b'9' | b'X');
    let is_isbn_13 = normalized.len() == 13 && normalized.bytes().all(|b| b.is_ascii_digit());
    (is_isbn_10 || is_isbn_13).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doi_url_is_identified_and_normalized() {
        assert_eq!(
            identify("https://doi.org/10.1109/tcbb.2019.000001"),
            Some(Identified::Doi("10.1109/tcbb.2019.000001".to_string()))
        );
    }

    #[test]
    fn a_bare_doi_is_identified() {
        assert_eq!(identify("10.1371/journal.pcbi.1000387"), Some(Identified::Doi("10.1371/journal.pcbi.1000387".to_string())));
    }

    #[test]
    fn a_new_style_arxiv_id_is_identified() {
        assert_eq!(identify("arXiv:1706.03762"), Some(Identified::Arxiv("1706.03762".to_string())));
    }

    #[test]
    fn a_new_style_arxiv_id_with_version_is_identified() {
        assert_eq!(identify("1706.03762v7"), Some(Identified::Arxiv("1706.03762v7".to_string())));
    }

    #[test]
    fn an_old_style_arxiv_id_is_identified() {
        assert_eq!(identify("hep-th/9901001"), Some(Identified::Arxiv("hep-th/9901001".to_string())));
    }

    #[test]
    fn an_old_style_arxiv_id_with_a_version_suffix_is_identified() {
        assert_eq!(identify("hep-th/9901001v2"), Some(Identified::Arxiv("hep-th/9901001v2".to_string())));
    }

    #[test]
    fn an_arxiv_abs_url_is_identified() {
        assert_eq!(identify("https://arxiv.org/abs/2101.00001"), Some(Identified::Arxiv("2101.00001".to_string())));
    }

    #[test]
    fn a_13_digit_isbn_is_identified() {
        assert_eq!(identify("978-0-262-03384-8"), Some(Identified::Isbn("9780262033848".to_string())));
    }

    #[test]
    fn a_10_digit_isbn_is_identified() {
        assert_eq!(identify("0-262-03384-4"), Some(Identified::Isbn("0262033844".to_string())));
    }

    #[test]
    fn a_10_digit_isbn_with_x_check_digit_is_identified() {
        assert_eq!(identify("0-306-40615-X"), Some(Identified::Isbn("030640615X".to_string())));
    }

    #[test]
    fn a_plain_sentence_identifies_as_nothing() {
        assert_eq!(identify("A Probabilistic Model of DNA Folding"), None);
    }

    #[test]
    fn an_empty_or_whitespace_paste_identifies_as_nothing() {
        assert_eq!(identify(""), None);
        assert_eq!(identify("   "), None);
    }

    #[test]
    fn a_run_of_the_wrong_digit_count_identifies_as_nothing() {
        assert_eq!(identify("12345"), None);
        assert_eq!(identify("123456789012345"), None);
    }

    #[test]
    fn a_doi_is_preferred_when_a_string_could_also_look_like_something_else() {
        // No real collision exists between the three shapes, but this pins the order anyway so a
        // future change to one matcher's looseness is caught by a test, not just by luck.
        assert_eq!(identify("10.1109/tcbb.2019.000001"), Some(Identified::Doi("10.1109/tcbb.2019.000001".to_string())));
    }
}
