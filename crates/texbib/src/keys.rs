//! Generates a citation key for an [`Entry`] that has none yet, unique against a set of keys
//! already in use — the real caller `arxiv::generated_key`'s doc comment deferred this to:
//! paste-to-cite (S7.6) appends a hand-built entry to the project's `.bib` and needs a key that
//! does not collide with anything already in it.
//!
//! Must never read a file or make a network request (see `lib.rs`); `existing` is passed in by
//! the caller, who already has the project's index in memory.

use crate::Entry;

/// `surnameYEARfirstword`, ASCII-folded and lower-cased, disambiguated with a trailing letter
/// (`a`, `b`, `c`, ...) if that key is already in `existing`. The base shape matches
/// `arxiv::generated_key` — this function does not replace that one (arXiv and ISBN entries are
/// not indexed anywhere yet when their key is generated, so they have nothing to collide against)
/// — it is what a caller reaches for once it *does* have a real index to check.
///
/// `existing` is compared case-insensitively, matching BibTeX's own key comparison ([`Entry::key`]
/// doc comment) — a generated key that differed from an existing one only in case would still be
/// visually indistinguishable in a citation list.
pub fn unique_key(entry: &Entry, existing: &[String]) -> String {
    let base = base_key(entry);
    if !collides(&base, existing) {
        return base;
    }
    for suffix in b'a'..=b'z' {
        let candidate = format!("{base}{}", suffix as char);
        if !collides(&candidate, existing) {
            return candidate;
        }
    }
    // 26 collisions on the same surname/year/word is not a real bibliography; falling back to a
    // running number keeps this total rather than panicking on an adversarial `existing` list.
    let mut n = 1u32;
    loop {
        let candidate = format!("{base}{n}");
        if !collides(&candidate, existing) {
            return candidate;
        }
        n += 1;
    }
}

fn collides(candidate: &str, existing: &[String]) -> bool {
    existing.iter().any(|key| key.eq_ignore_ascii_case(candidate))
}

/// `surnameYEARfirstword` from whatever `entry` already carries — its own key if `identify`'s
/// caller already set one (an entry parsed by `doi::fetch_doi` inherits a real key from the
/// publisher's own BibTeX and should keep it, not be renamed), else built the same way
/// `arxiv::generated_key` does, from `author`/`editor`, `year` and `title`.
fn base_key(entry: &Entry) -> String {
    if !entry.key.is_empty() {
        return entry.key.clone();
    }
    let author = entry.field("author").or_else(|| entry.field("editor"));
    let surname = author
        .map(|field| surname_of(&plain_text(&field.value)))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let year = entry.field("year").map(|field| plain_text(&field.value)).unwrap_or_default();
    let title = entry.field("title").map(|field| plain_text(&field.value)).unwrap_or_default();
    let first_word = title.split_whitespace().find(|word| word.chars().any(char::is_alphanumeric)).unwrap_or("");
    format!("{surname}{year}{}", ascii_fold_lower(first_word))
}

/// A hand-built field's value is always a single [`crate::ValuePart::Braced`] part (see
/// `arxiv.rs`'s `field` and `isbn.rs`'s own equivalent) — no `@string` to resolve, no `#`
/// concatenation — so reading it back out is just unwrapping, not a `Bibliography::resolve` call
/// that would need a whole file to resolve against.
fn plain_text(value: &crate::Value) -> String {
    value
        .parts
        .iter()
        .map(|part| match part {
            crate::ValuePart::Braced(text) | crate::ValuePart::Quoted(text) => text.as_str(),
            crate::ValuePart::Number(digits) => digits.as_str(),
            crate::ValuePart::Macro(_) => "",
        })
        .collect()
}

/// The surname out of a BibTeX-style author field's *first* name: `"Last, First and ..."` takes
/// everything before the first comma or `and`; a plain `"First Last"` takes the last word;
/// `"{Corporate Name}"` is kept whole. A looser cousin of `src/lib/editor/cite.ts`'s `splitName` —
/// this only needs the first author's surname for a key, not every author for a label.
fn surname_of(author_field: &str) -> String {
    let first = author_field.split(" and ").next().unwrap_or(author_field).trim();
    if let Some(inner) = first.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return ascii_fold_lower(inner);
    }
    if let Some((last, _rest)) = first.split_once(',') {
        return ascii_fold_lower(last.trim());
    }
    let words: Vec<&str> = first.split_whitespace().collect();
    ascii_fold_lower(words.last().unwrap_or(&first))
}

fn ascii_fold_lower(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Field, Span, Value, ValuePart};

    fn zero_span() -> Span {
        Span { start: 0, end: 0 }
    }

    fn braced_field(name: &str, text: &str) -> Field {
        Field { span: zero_span(), name: name.to_string(), value_span: zero_span(), value: Value { parts: vec![ValuePart::Braced(text.to_string())] } }
    }

    fn entry_with_fields(key: &str, fields: Vec<Field>) -> Entry {
        Entry { span: zero_span(), entry_type: "misc".to_string(), key: key.to_string(), key_span: zero_span(), fields }
    }

    #[test]
    fn an_entry_that_already_has_a_key_keeps_it() {
        let entry = entry_with_fields("smith2019original", vec![]);
        assert_eq!(unique_key(&entry, &[]), "smith2019original");
    }

    #[test]
    fn a_keyless_entry_gets_the_surname_year_firstword_shape() {
        let entry = entry_with_fields("", vec![braced_field("author", "Jane Smith"), braced_field("year", "2019"), braced_field("title", "A Great Paper")]);
        assert_eq!(unique_key(&entry, &[]), "smith2019a");
    }

    #[test]
    fn a_comma_separated_author_field_still_yields_the_surname() {
        let entry = entry_with_fields("", vec![braced_field("author", "Smith, Jane and Doe, John"), braced_field("year", "2019"), braced_field("title", "Something")]);
        assert_eq!(unique_key(&entry, &[]), "smith2019something");
    }

    #[test]
    fn a_corporate_author_in_braces_is_kept_whole() {
        let entry = entry_with_fields("", vec![braced_field("author", "{World Health Organization}"), braced_field("year", "2020"), braced_field("title", "Report")]);
        assert_eq!(unique_key(&entry, &[]), "worldhealthorganization2020report");
    }

    #[test]
    fn a_collision_gets_a_letter_suffix() {
        let entry = entry_with_fields("", vec![braced_field("author", "Jane Smith"), braced_field("year", "2019"), braced_field("title", "A Great Paper")]);
        let existing = vec!["smith2019a".to_string()];
        assert_eq!(unique_key(&entry, &existing), "smith2019aa");
    }

    #[test]
    fn collision_comparison_ignores_case() {
        let entry = entry_with_fields("smith2019a", vec![]);
        let existing = vec!["SMITH2019A".to_string()];
        assert_eq!(unique_key(&entry, &existing), "smith2019aa");
    }

    #[test]
    fn every_letter_taken_falls_back_to_a_running_number() {
        let entry = entry_with_fields("k", vec![]);
        let mut existing = vec!["k".to_string()];
        for suffix in b'a'..=b'z' {
            existing.push(format!("k{}", suffix as char));
        }
        assert_eq!(unique_key(&entry, &existing), "k1");
    }

    #[test]
    fn a_missing_author_falls_back_to_unknown() {
        let entry = entry_with_fields("", vec![braced_field("year", "2021"), braced_field("title", "Untitled Work")]);
        assert_eq!(unique_key(&entry, &[]), "unknown2021untitled");
    }
}
