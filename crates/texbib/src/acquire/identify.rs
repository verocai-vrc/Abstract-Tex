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

/// A DOI is one word shaped `10.<registrant>/<suffix>`, wrapped or bare — the structural facts
/// [`crate::acquire::doi::normalize_doi`] does not itself check, since that function only strips a
/// wrapper and trusts doi.org to judge the rest. `identify` is the one place that does judge, so
/// ordinary text is never sent to doi.org as if it might be one (review finding R8.18: a paste that
/// merely began `10.` — "10. Conclusion", "10.5 mg was dissolved…" — used to be, and the whole of it
/// went out as part of the URL).
fn looks_like_doi(candidate: &str) -> Option<String> {
    let normalized = normalize_doi(candidate);
    is_doi_shape(&normalized).then_some(normalized)
}

/// `10.` + a registrant code + `/` + a suffix, with no whitespace anywhere: a DOI is never written
/// with a space in it, so a paste that has one is a DOI *followed by something*, and that something
/// would be sent to doi.org along with it. The registrant code is digits, with optional `.`-separated
/// sub-divisions (`10.1000.10/123456`), and its first group is at least four digits long — every
/// registrant code issued is, and it is what keeps a ratio such as `10.5/12` out. The suffix is
/// anything non-empty: old publisher DOIs carry parentheses, colons, brackets and semicolons.
fn is_doi_shape(doi: &str) -> bool {
    if doi.chars().any(char::is_whitespace) {
        return false;
    }
    // `let … else` binds on a match and runs its `else` block (which must leave the function) when
    // there is none — a flat way to say "give up unless the text has this shape", where a chain of
    // `if let` would nest one level deeper per step.
    let Some(after_prefix) = doi.strip_prefix("10.") else {
        return false;
    };
    let Some((registrant, suffix)) = after_prefix.split_once('/') else {
        return false;
    };
    if suffix.is_empty() {
        return false;
    }
    let mut groups = registrant.split('.');
    let first_group_is_long_enough = groups
        .next()
        .is_some_and(|group| group.len() >= 4 && group.bytes().all(|byte| byte.is_ascii_digit()));
    first_group_is_long_enough
        && groups.all(|group| !group.is_empty() && group.bytes().all(|byte| byte.is_ascii_digit()))
}

fn looks_like_arxiv_id(candidate: &str) -> Option<String> {
    let normalized = normalize_arxiv_id(candidate);
    is_arxiv_id_shape(&normalized).then_some(normalized)
}

/// `YYMM.NNNN[vN]` or `YYMM.NNNNN[vN]` (new-style, 2007 onward: four digits after the dot up to
/// December 2014, five from January 2015) or `archive/YYMMNNN` (old-style) — the two shapes
/// [`crate::acquire::arxiv::normalize_arxiv_id`]'s own doc comment names, the version suffix
/// included since `normalize_arxiv_id` deliberately keeps it rather than stripping it (only
/// `arxiv::fetch_arxiv_with`'s own `eprint`/`url` fields strip it, for the reason its comment
/// gives). Checked digit-by-digit rather than with a regex dependency, the same discipline the
/// rest of this crate holds to.
fn is_arxiv_id_shape(id: &str) -> bool {
    let without_version = strip_version_suffix(id);
    if let Some((yymm, rest)) = without_version.split_once('.') {
        return is_year_and_month(yymm)
            && (rest.len() == 4 || rest.len() == 5)
            && rest.bytes().all(|b| b.is_ascii_digit());
    }
    if let Some((archive, digits)) = without_version.split_once('/') {
        return !archive.is_empty()
            && archive
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'.' || b == b'-')
            && digits.len() == 7
            && digits.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

/// `YYMM`: four digits whose last two are a month, 01 to 12 (the first new-style id is `0704.0001`,
/// April 2007). Without the month and the four-or-five-digit number after the dot, any decimal
/// number such as `1250.5` or `3000.25` was "an arXiv id", and pasting it into a table swallowed
/// it and sent it to arxiv.org (review finding R8.22).
fn is_year_and_month(yymm: &str) -> bool {
    let digits = yymm.as_bytes();
    if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let month = (digits[2] - b'0') * 10 + (digits[3] - b'0');
    (1..=12).contains(&month)
}

/// A run of exactly 10 or 13 digits once hyphens/spaces are stripped, the last character of a
/// 10-digit ISBN allowed to be a literal `X` check digit (ISBN-10's own rule; ISBN-13 has none),
/// whose check digit adds up, and which — for 13 digits — begins `978` or `979`. A bare number is
/// not an ISBN just for having ten or thirteen digits: a phone number, an order number or a
/// millisecond timestamp (`1700000000000`) has them too, and the editor swallows what this accepts
/// (review finding R8.21). A mistyped ISBN is therefore pasted as the text it is, not looked up.
fn looks_like_isbn(candidate: &str) -> Option<String> {
    let normalized = normalize_isbn(candidate);
    // Bytes, not a `str` slice: `normalized[..9]` panics when byte 9 falls inside a multi-byte
    // letter, and `normalized` is whatever the author pasted (review finding R8.19: `Jean-André`
    // is ten bytes once the hyphen goes, and the `é` straddles byte 9). On bytes the same test is
    // simply false for any non-ASCII text.
    let bytes = normalized.as_bytes();
    let is_isbn_10 = bytes.len() == 10
        && bytes[..9].iter().all(u8::is_ascii_digit)
        && matches!(bytes[9], b'0'..=b'9' | b'X')
        && isbn_10_check_digit_adds_up(bytes);
    let is_isbn_13 = bytes.len() == 13
        && bytes.iter().all(u8::is_ascii_digit)
        && (bytes.starts_with(b"978") || bytes.starts_with(b"979"))
        && isbn_13_check_digit_adds_up(bytes);
    (is_isbn_10 || is_isbn_13).then_some(normalized)
}

/// ISBN-10's rule (ISO 2108): the digits weighted 10, 9, ... 1 — a final `X` counting as ten — add up
/// to a multiple of 11. `isbn` is already known to be nine digits and a digit or `X`.
fn isbn_10_check_digit_adds_up(isbn: &[u8]) -> bool {
    let weighted_sum: u32 = isbn
        .iter()
        .enumerate()
        .map(|(position, &byte)| {
            let value = if byte == b'X' { 10 } else { u32::from(byte - b'0') };
            value * (10 - position as u32)
        })
        .sum();
    weighted_sum.is_multiple_of(11)
}

/// ISBN-13's rule: the digits weighted 1, 3, 1, 3, ... add up to a multiple of 10. `isbn` is already
/// known to be thirteen digits.
fn isbn_13_check_digit_adds_up(isbn: &[u8]) -> bool {
    let weighted_sum: u32 = isbn
        .iter()
        .enumerate()
        .map(|(position, &byte)| {
            let weight = if position % 2 == 0 { 1 } else { 3 };
            u32::from(byte - b'0') * weight
        })
        .sum();
    weighted_sum.is_multiple_of(10)
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
        assert_eq!(
            identify("10.1371/journal.pcbi.1000387"),
            Some(Identified::Doi("10.1371/journal.pcbi.1000387".to_string()))
        );
    }

    #[test]
    fn a_new_style_arxiv_id_is_identified() {
        assert_eq!(
            identify("arXiv:1706.03762"),
            Some(Identified::Arxiv("1706.03762".to_string()))
        );
    }

    #[test]
    fn a_new_style_arxiv_id_with_version_is_identified() {
        assert_eq!(
            identify("1706.03762v7"),
            Some(Identified::Arxiv("1706.03762v7".to_string()))
        );
    }

    #[test]
    fn an_old_style_arxiv_id_is_identified() {
        assert_eq!(
            identify("hep-th/9901001"),
            Some(Identified::Arxiv("hep-th/9901001".to_string()))
        );
    }

    #[test]
    fn an_old_style_arxiv_id_with_a_version_suffix_is_identified() {
        assert_eq!(
            identify("hep-th/9901001v2"),
            Some(Identified::Arxiv("hep-th/9901001v2".to_string()))
        );
    }

    #[test]
    fn an_arxiv_abs_url_is_identified() {
        assert_eq!(
            identify("https://arxiv.org/abs/2101.00001"),
            Some(Identified::Arxiv("2101.00001".to_string()))
        );
    }

    #[test]
    fn a_13_digit_isbn_is_identified() {
        assert_eq!(
            identify("978-0-262-03384-8"),
            Some(Identified::Isbn("9780262033848".to_string()))
        );
    }

    #[test]
    fn a_10_digit_isbn_is_identified() {
        assert_eq!(
            identify("0-262-03384-4"),
            Some(Identified::Isbn("0262033844".to_string()))
        );
    }

    #[test]
    fn a_10_digit_isbn_with_x_check_digit_is_identified() {
        assert_eq!(
            identify("0-8044-2957-X"),
            Some(Identified::Isbn("080442957X".to_string()))
        );
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

    /// `tests/identify-cases.json` is read by this test and by `src/lib/editor/paste.test.ts`: the
    /// editor decides in TypeScript, before any IPC, whether to swallow a paste, and a paste it
    /// swallows that this function then refuses is lost. One table, two readers, so they cannot drift.
    #[test]
    fn the_shared_cases_are_answered_as_written() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/identify-cases.json")).expect("the table is JSON");
        let cases = table["cases"].as_array().expect("the table has a list of cases");
        assert!(cases.len() > 20, "the table was emptied");
        for case in cases {
            let paste = case["paste"].as_str().expect("every case has a paste");
            let normalized = || {
                case["identified"]["normalized"]
                    .as_str()
                    .expect("a normalized form")
                    .to_string()
            };
            let expected = match case["identified"]["kind"].as_str() {
                None => None,
                Some("doi") => Some(Identified::Doi(normalized())),
                Some("arxiv") => Some(Identified::Arxiv(normalized())),
                Some("isbn") => Some(Identified::Isbn(normalized())),
                Some(other) => panic!("unknown kind {other:?} in the table"),
            };
            assert_eq!(identify(paste), expected, "{paste:?}");
        }
    }

    // Review finding R8.18: `looks_like_doi` accepted anything that began `10.`, so a numbered heading
    // or a measurement was "a DOI", and the editor sent the whole paste to doi.org.
    #[test]
    fn text_that_merely_begins_with_ten_point_is_not_a_doi() {
        for prose in [
            "10. Conclusion",
            "10.5 mg of the compound was dissolved in 2 ml of ethanol and stirred.",
            "10.5% of the sample failed.",
            "10.000 samples were collected over two years",
            // A ratio has a slash, but a registrant code is never one digit long.
            "10.5/12",
        ] {
            assert_eq!(identify(prose), None, "{prose:?} is prose, not a DOI");
        }
    }

    #[test]
    fn a_doi_followed_by_more_text_is_not_a_doi() {
        // Whatever follows a DOI would be sent to doi.org as part of it.
        assert_eq!(identify("10.1000/xyz123 is the DOI of the paper I mean"), None);
        assert_eq!(
            identify("10.1000/xyz123\nAnd then a paragraph of the manuscript."),
            None
        );
        assert_eq!(identify("https://doi.org/10.1000/xyz123 and more"), None);
    }

    #[test]
    fn a_doi_with_the_odd_characters_real_ones_have_is_still_identified() {
        // Old Wiley DOIs carry parentheses, colons, angle brackets and semicolons.
        let wiley = "10.1002/(SICI)1097-4636(199706)35:3<345::AID-JBM8>3.0.CO;2-N";
        assert_eq!(identify(wiley), Some(Identified::Doi(wiley.to_string())));
        // A registrant code may carry a sub-division.
        assert_eq!(
            identify("10.1000.10/123456"),
            Some(Identified::Doi("10.1000.10/123456".to_string()))
        );
        // The `doi:` scheme, in any capitals, and surrounding whitespace.
        assert_eq!(
            identify("  DOI: 10.1371/journal.pcbi.1000387\n"),
            Some(Identified::Doi("10.1371/journal.pcbi.1000387".to_string()))
        );
    }

    #[test]
    fn a_doi_prefix_with_nothing_after_the_slash_is_not_a_doi() {
        assert_eq!(identify("10.1000/"), None);
        assert_eq!(identify("10.1000"), None);
        assert_eq!(identify("10./abc"), None);
    }

    #[test]
    fn a_registrant_code_is_digits_and_dots_and_nothing_else() {
        for not_a_doi in [
            "10.abcd/xyz",
            "10.1000x/xyz",
            "10.1000..5/xyz",
            "10.1000./xyz",
            "10.1000.a/xyz",
        ] {
            assert_eq!(identify(not_a_doi), None, "{not_a_doi:?}");
        }
    }

    // Review finding R8.19: `normalized[..9]` panics when byte 9 is inside a character.
    #[test]
    fn ten_bytes_of_letters_that_are_not_ascii_do_not_panic() {
        // 5 x 2 bytes, 1 + 3 x 3 bytes, and a hyphenated name that is 10 bytes once the hyphen goes.
        for pasted in ["ééééé", "a李明李", "Jean-André"] {
            assert_eq!(identify(pasted), None, "{pasted:?}");
        }
    }

    #[test]
    fn a_doi_is_preferred_when_a_string_could_also_look_like_something_else() {
        // No real collision exists between the three shapes, but this pins the order anyway so a
        // future change to one matcher's looseness is caught by a test, not just by luck.
        assert_eq!(
            identify("10.1109/tcbb.2019.000001"),
            Some(Identified::Doi("10.1109/tcbb.2019.000001".to_string()))
        );
    }
}
