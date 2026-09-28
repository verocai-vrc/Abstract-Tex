//! Paste-to-cite (S7.6, DESIGN.md §5.4: "the entry appears, deduplicated"): what happens once a
//! pasted string has been identified (`texbib::acquire::identify`) and its entry fetched
//! (`texbib::acquire::{doi, arxiv, isbn}`) — deciding whether it is already in the project's
//! bibliography, and if not, where and how it is written.
//!
//! Split from `commands.rs` the way `synctex.rs` is: this module holds everything that can be
//! tested without a network request or a real file — dedup and key generation take the already-
//! fetched [`texbib::Entry`] and the project's own [`crate::bibliography::BibliographyIndex`] as
//! plain data. Only the fetch itself (a real HTTP request) and the actual disk write are the
//! command's job, because pulling either into a unit test would mean faking a transport or a
//! filesystem this module has no reason to know about.

use texbib::Entry;

use crate::bibliography::{BibliographyIndex, EntrySummary};

/// What pasting a recognised identifier resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteOutcome {
    /// The identifier (or, failing that, the normalised title and year) already matches an entry
    /// in the index. Nothing is written; the existing key is what gets cited.
    Existing { key: String },
    /// No match. `bib_file` is the project-relative path of the `.bib` the entry should be
    /// appended to — the first one the document names, matching S7.2's own "first `.bib` file"
    /// convention nothing else in this codebase has needed until now. `keys_in_use` is every key
    /// already in the index, for [`render_new_entry`] to generate a unique one against once the
    /// caller has read `bib_file`'s current text.
    New { bib_file: String, keys_in_use: Vec<String> },
}

/// A pasted identifier resolved to nothing this app can act on: no `.bib` file to append to
/// (a document with none is not wrong, just not ready for this feature yet), or the resolved
/// title/year could not be told apart from every entry already in the file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PasteError {
    /// The document names no `.bib` file at all — nowhere to put a new entry. A future loop
    /// could offer to create one; today the author is told plainly instead of one appearing
    /// silently in a place they never asked for.
    #[error("this project has no .bib file yet — add \\bibliography{{...}} or \\addbibresource{{...}} first")]
    NoBibFile,
}

/// Decide what pasting `fetched` (an entry already resolved from a DOI/arXiv id/ISBN, with
/// `0..0` or real spans depending on its source — this function never reads a span) means against
/// `index`: an existing key to reuse, or the file to append a new one to (not yet rendered — that
/// needs that file's current text, which the caller has not necessarily read yet; see
/// [`render_new_entry`]).
///
/// Dedup order, per the card: an identifier match (DOI, then arXiv id, then ISBN — whichever the
/// fetched entry actually carries; a DOI-sourced entry has no `eprint` to match on and vice
/// versa) beats a title+year match, because two entries can legitimately share a title (a
/// preprint and its published version are the common case) but never legitimately share a DOI.
/// Title+year is the fallback for exactly that pair: an arXiv paste of something already in the
/// bibliography as its DOI-sourced journal version, where the two records carry no common
/// identifier field at all.
pub fn resolve_paste(fetched: &Entry, index: &BibliographyIndex) -> Result<PasteOutcome, PasteError> {
    if let Some(hit) = find_by_identifier(fetched, index) {
        return Ok(PasteOutcome::Existing { key: hit.key.clone() });
    }
    if let Some(hit) = find_by_title_year(fetched, index) {
        return Ok(PasteOutcome::Existing { key: hit.key.clone() });
    }
    let bib_file = index.files.first().ok_or(PasteError::NoBibFile)?;
    Ok(PasteOutcome::New { bib_file: bib_file.path.clone(), keys_in_use: index.entries.iter().map(|entry| entry.key.clone()).collect() })
}

/// A generated key and the whole new text of `bib_file`, once the caller has read it — the part
/// of [`PasteOutcome::New`] that needed a real file read and so could not happen inside
/// [`resolve_paste`] itself.
pub fn render_new_entry(fetched: &Entry, keys_in_use: &[String], bib_file_text: &str) -> (String, String) {
    let key = texbib::unique_key(fetched, keys_in_use);
    let mut keyed = fetched.clone();
    keyed.key = key.clone();
    (key, texbib::append_entry(bib_file_text, &keyed))
}

/// The first existing entry whose DOI, arXiv `eprint`, or ISBN matches the one field `fetched`
/// itself carries — a DOI-sourced entry only ever has a `doi` to compare, an arXiv one only an
/// `eprint`, an ISBN one only an `isbn`, since each source builds exactly one of the three
/// (`texbib::acquire`'s three modules). Values on both sides are already normalised the same way
/// (`bibliography.rs`'s `doi_of`/`eprint_of`/`isbn_of` for the index, and the identifier passed
/// into `resolve_paste` is normalised before the entry is even fetched), so a plain `==` suffices
/// — no comparison-time re-normalisation to keep in sync with either source's own rules.
fn find_by_identifier<'a>(fetched: &Entry, index: &'a BibliographyIndex) -> Option<&'a EntrySummary> {
    if let Some(doi) = normalized_field(fetched, "doi") {
        return index.entries.iter().find(|entry| entry.doi.as_deref() == Some(doi.as_str()));
    }
    if fetched.field("eprinttype").is_some_and(|field| field.value.parts.iter().any(|part| part_text(part).eq_ignore_ascii_case("arxiv"))) {
        if let Some(eprint) = normalized_field(fetched, "eprint") {
            return index.entries.iter().find(|entry| entry.eprint.as_deref() == Some(eprint.as_str()));
        }
    }
    if let Some(isbn) = normalized_field(fetched, "isbn") {
        return index.entries.iter().find(|entry| entry.isbn.as_deref() == Some(isbn.as_str()));
    }
    None
}

/// A raw field's text, unnormalised — used only to build the identifier lookups above, which
/// already compare against pre-normalised index values, so the field itself is read as written.
fn normalized_field(entry: &Entry, name: &str) -> Option<String> {
    entry.field(name).map(|field| field.value.parts.iter().map(part_text).collect())
}

fn part_text(part: &texbib::ValuePart) -> &str {
    match part {
        texbib::ValuePart::Braced(text) | texbib::ValuePart::Quoted(text) => text,
        texbib::ValuePart::Number(digits) => digits,
        texbib::ValuePart::Macro(name) => name,
    }
}

/// The first existing entry whose title and year both match `fetched`'s, comparing titles
/// case-insensitively after collapsing whitespace — a fallback for the pair of sources that share
/// no identifier field at all (see `resolve_paste`'s doc comment). Both fields must be present
/// and non-empty on both sides: an entry missing a year is not "the same paper regardless of
/// year," it is simply not enough to go on, and treating it as a match risks silently discarding
/// a real second reference with the same working title.
fn find_by_title_year<'a>(fetched: &Entry, index: &'a BibliographyIndex) -> Option<&'a EntrySummary> {
    let title = normalized_field(fetched, "title").map(|t| collapse_and_lower(&t)).filter(|t| !t.is_empty())?;
    let year = normalized_field(fetched, "year").filter(|y| !y.is_empty())?;
    index.entries.iter().find(|entry| {
        let entry_title = entry.title.as_deref().map(collapse_and_lower).unwrap_or_default();
        entry_title == title && entry.year.as_deref() == Some(year.as_str())
    })
}

fn collapse_and_lower(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibliography::{BibFile, BibOrigin, Problem};
    use texbib::{Field, Span, Value, ValuePart};

    fn zero_span() -> Span {
        Span { start: 0, end: 0 }
    }

    fn braced_field(name: &str, text: &str) -> Field {
        Field { span: zero_span(), name: name.to_string(), value_span: zero_span(), value: Value { parts: vec![ValuePart::Braced(text.to_string())] } }
    }

    fn fetched_entry(fields: Vec<Field>) -> Entry {
        Entry { span: zero_span(), entry_type: "online".to_string(), key: String::new(), key_span: zero_span(), fields }
    }

    fn summary(key: &str, title: Option<&str>, year: Option<&str>, doi: Option<&str>, eprint: Option<&str>, isbn: Option<&str>) -> EntrySummary {
        EntrySummary {
            key: key.to_string(),
            entry_type: "article".to_string(),
            author: None,
            year: year.map(str::to_string),
            title: title.map(str::to_string),
            doi: doi.map(str::to_string),
            eprint: eprint.map(str::to_string),
            isbn: isbn.map(str::to_string),
            file: "refs.bib".to_string(),
            span: zero_span(),
        }
    }

    fn index_with(entries: Vec<EntrySummary>) -> BibliographyIndex {
        BibliographyIndex {
            files: vec![BibFile { path: "refs.bib".to_string(), exists: true, entry_count: entries.len(), problems: Vec::<Problem>::new(), origin: BibOrigin::Named { file: "main.tex".to_string(), line: 1 } }],
            entries,
            citations: Vec::new(),
            has_nocite_star: false,
        }
    }

    #[test]
    fn a_matching_doi_reuses_the_existing_key() {
        let fetched = fetched_entry(vec![braced_field("doi", "10.1109/tcbb.2019.000001"), braced_field("title", "A Paper")]);
        let index = index_with(vec![summary("smith2019", Some("A Paper"), Some("2019"), Some("10.1109/tcbb.2019.000001"), None, None)]);
        assert_eq!(resolve_paste(&fetched, &index).unwrap(), PasteOutcome::Existing { key: "smith2019".to_string() });
    }

    #[test]
    fn a_matching_arxiv_eprint_reuses_the_existing_key() {
        let fetched = fetched_entry(vec![braced_field("eprint", "1706.03762"), braced_field("eprinttype", "arxiv"), braced_field("title", "Attention")]);
        let index = index_with(vec![summary("vaswani2017", Some("Attention"), Some("2017"), None, Some("1706.03762"), None)]);
        assert_eq!(resolve_paste(&fetched, &index).unwrap(), PasteOutcome::Existing { key: "vaswani2017".to_string() });
    }

    #[test]
    fn a_matching_isbn_reuses_the_existing_key() {
        let fetched = fetched_entry(vec![braced_field("isbn", "9780262033848"), braced_field("title", "A Book")]);
        let index = index_with(vec![summary("knuth1984", Some("A Book"), Some("1984"), None, None, Some("9780262033848"))]);
        assert_eq!(resolve_paste(&fetched, &index).unwrap(), PasteOutcome::Existing { key: "knuth1984".to_string() });
    }

    #[test]
    fn a_title_and_year_match_reuses_the_key_when_neither_side_has_an_identifier() {
        let fetched = fetched_entry(vec![braced_field("title", "  A   Great   Paper "), braced_field("year", "2019")]);
        let index = index_with(vec![summary("smith2019a", Some("A Great Paper"), Some("2019"), None, None, None)]);
        assert_eq!(resolve_paste(&fetched, &index).unwrap(), PasteOutcome::Existing { key: "smith2019a".to_string() });
    }

    #[test]
    fn a_title_match_with_a_different_year_is_not_a_dedup_hit() {
        let fetched = fetched_entry(vec![braced_field("title", "A Great Paper"), braced_field("year", "2020")]);
        let index = index_with(vec![summary("smith2019a", Some("A Great Paper"), Some("2019"), None, None, None)]);
        let outcome = resolve_paste(&fetched, &index).unwrap();
        assert!(matches!(outcome, PasteOutcome::New { .. }));
    }

    #[test]
    fn no_match_appends_a_new_entry_with_a_generated_key() {
        let fetched = fetched_entry(vec![braced_field("author", "Jane Smith"), braced_field("title", "A New Paper"), braced_field("year", "2024")]);
        let index = index_with(vec![]);
        let outcome = resolve_paste(&fetched, &index).unwrap();
        match outcome {
            PasteOutcome::New { bib_file, keys_in_use } => {
                assert_eq!(bib_file, "refs.bib");
                let (key, text) = render_new_entry(&fetched, &keys_in_use, "@article{a, title = {A}}\n");
                assert_eq!(key, "smith2024a");
                assert!(text.starts_with("@article{a, title = {A}}\n"));
                assert!(text.contains("smith2024a"));
            }
            other => panic!("expected New, got {other:?}"),
        }
    }

    #[test]
    fn a_generated_key_avoids_a_real_collision_in_the_index() {
        let fetched = fetched_entry(vec![braced_field("author", "Jane Smith"), braced_field("title", "A New Paper"), braced_field("year", "2024")]);
        let index = index_with(vec![summary("smith2024a", Some("Something Else"), Some("2024"), None, None, None)]);
        let outcome = resolve_paste(&fetched, &index).unwrap();
        match outcome {
            PasteOutcome::New { keys_in_use, .. } => {
                let (key, _text) = render_new_entry(&fetched, &keys_in_use, "");
                assert_eq!(key, "smith2024aa");
            }
            other => panic!("expected New, got {other:?}"),
        }
    }

    #[test]
    fn no_bib_file_is_a_named_error_not_a_panic() {
        let fetched = fetched_entry(vec![braced_field("title", "A Paper")]);
        let index = BibliographyIndex::default();
        assert_eq!(resolve_paste(&fetched, &index), Err(PasteError::NoBibFile));
    }

    #[test]
    fn appending_the_same_doi_twice_yields_one_entry_and_the_same_key_both_times() {
        let fetched = fetched_entry(vec![braced_field("doi", "10.1/x"), braced_field("author", "Jane Smith"), braced_field("title", "Once"), braced_field("year", "2024")]);
        let index = index_with(vec![]);
        let first = resolve_paste(&fetched, &index).unwrap();
        let (first_key, text_after_first) = match first {
            PasteOutcome::New { keys_in_use, .. } => render_new_entry(&fetched, &keys_in_use, ""),
            other => panic!("expected New, got {other:?}"),
        };

        // Simulate the index after the first paste landed: one entry, with the DOI recorded.
        let index_after = index_with(vec![summary(&first_key, Some("Once"), Some("2024"), Some("10.1/x"), None, None)]);
        let second = resolve_paste(&fetched, &index_after).unwrap();
        assert!(text_after_first.contains(&first_key));
        assert_eq!(second, PasteOutcome::Existing { key: first_key });
    }
}
