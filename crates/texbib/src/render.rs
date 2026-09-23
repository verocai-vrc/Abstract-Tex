//! Renders an [`Entry`] back to `.bib` text, and appends one to a file's own text without
//! touching any other byte of it — the two operations S7.6's paste-to-cite needs: a hand-built
//! entry from `acquire` has no text of its own yet (`acquire::arxiv`'s `zero_span` doc comment
//! says so), and this module is "the first time" that comment forward references.
//!
//! Must never read a file or make a network request (see `lib.rs`); `append_entry` takes the
//! file's current text as a `&str` and returns new text, the same "text in, data out" shape
//! `preamble-reconcile::diff_ops` uses so a caller can turn the result into a normal buffer edit
//! rather than a raw filesystem write.

use crate::{Entry, Value, ValuePart};

/// One field per line, aligned so every `=` lines up under the longest field name — the shape
/// Better BibTeX writes (`crates/texbib/fixtures/better-bibtex/main.bib`), chosen so a pasted
/// entry looks like it could have come from the same export tool as its neighbours rather than
/// standing out as machine-written. Field order is `entry.fields`' own order, which every caller
/// in this crate (`acquire::arxiv::entry_from_feed`, `isbn`'s equivalent) already builds in a
/// sensible reading order (title, author, year, ...) — nothing here re-sorts it.
///
/// Every value is rendered braced, regardless of how [`ValuePart`] stored it: a hand-built entry
/// (this function's only real caller) only ever has [`ValuePart::Braced`] parts to begin with
/// (see `keys.rs`'s `plain_text` doc comment), and forcing braces on whatever *is* there — a
/// [`ValuePart::Quoted`] or [`ValuePart::Number`], if a future caller ever hands one in — keeps
/// the output valid BibTeX either way rather than trying to preserve a form this function was
/// never given a reason to be told about.
pub fn render_entry(entry: &Entry) -> String {
    let name_width = entry.fields.iter().map(|field| field.name.len()).max().unwrap_or(0);
    let mut out = format!("@{}{{{},\n", entry.entry_type, entry.key);
    for field in &entry.fields {
        out.push_str(&format!("  {:width$} = {{{}}},\n", field.name, render_value(&field.value), width = name_width));
    }
    out.push_str("}\n");
    out
}

/// A value's text with no macro resolution — a hand-built entry's fields have nothing to
/// resolve ([`crate::acquire`]'s entries never reference an `@string`) — parts simply
/// concatenated, which for the single-part case every real caller produces is just that part's
/// text.
fn render_value(value: &Value) -> String {
    value
        .parts
        .iter()
        .map(|part| match part {
            ValuePart::Braced(text) | ValuePart::Quoted(text) => text.as_str(),
            ValuePart::Number(digits) => digits.as_str(),
            ValuePart::Macro(name) => name.as_str(),
        })
        .collect()
}

/// `source` with `entry` appended after its last item, touching no other byte — the property
/// S7.1's fixture harness already checks for the parser's own spans (concatenating every item's
/// span reproduces the input byte for byte) is what makes this safe: appending is exactly "keep
/// every byte of `source`, then add more," so nothing already in the file can be disturbed by
/// construction, not by care taken here.
///
/// A blank line separates the new entry from whatever precedes it, unless the file is empty or
/// already ends in one — matching the blank line already between entries in every multi-entry
/// fixture this crate has (`better-bibtex`, `jabref`), so the appended entry reads as one more
/// item in the same file rather than as a special case glued onto the end.
pub fn append_entry(source: &str, entry: &Entry) -> String {
    let rendered = render_entry(entry);
    if source.is_empty() {
        return rendered;
    }
    let mut out = source.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&rendered);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Field, Span};

    fn zero_span() -> Span {
        Span { start: 0, end: 0 }
    }

    fn braced_field(name: &str, text: &str) -> Field {
        Field { span: zero_span(), name: name.to_string(), value_span: zero_span(), value: Value { parts: vec![ValuePart::Braced(text.to_string())] } }
    }

    fn sample_entry() -> Entry {
        Entry {
            span: zero_span(),
            entry_type: "online".to_string(),
            key: "vaswani2017attention".to_string(),
            key_span: zero_span(),
            fields: vec![
                braced_field("title", "Attention Is All You Need"),
                braced_field("author", "Ashish Vaswani and Noam Shazeer"),
                braced_field("year", "2017"),
            ],
        }
    }

    #[test]
    fn an_entry_renders_with_aligned_fields() {
        let rendered = render_entry(&sample_entry());
        assert_eq!(
            rendered,
            "@online{vaswani2017attention,\n  title  = {Attention Is All You Need},\n  author = {Ashish Vaswani and Noam Shazeer},\n  year   = {2017},\n}\n"
        );
    }

    #[test]
    fn rendered_text_parses_back_to_an_equivalent_entry() {
        let rendered = render_entry(&sample_entry());
        let bib = crate::parse(&rendered);
        let parsed = bib.entries().next().expect("one entry");
        assert_eq!(parsed.key, "vaswani2017attention");
        assert!(parsed.is_type("online"));
        assert_eq!(bib.resolve(&parsed.field("title").unwrap().value), "Attention Is All You Need");
        assert_eq!(bib.resolve(&parsed.field("year").unwrap().value), "2017");
    }

    #[test]
    fn appending_to_an_empty_file_is_just_the_rendered_entry() {
        assert_eq!(append_entry("", &sample_entry()), render_entry(&sample_entry()));
    }

    #[test]
    fn appending_touches_no_byte_of_the_existing_text() {
        let existing = "@article{k, title = {Existing}}\n";
        let result = append_entry(existing, &sample_entry());
        assert!(result.starts_with(existing));
        assert_eq!(result, format!("{existing}\n{}", render_entry(&sample_entry())));
    }

    #[test]
    fn appending_after_text_with_no_trailing_newline_still_separates_with_a_blank_line() {
        let existing = "@article{k, title = {Existing}}";
        let result = append_entry(existing, &sample_entry());
        assert_eq!(result, format!("{existing}\n\n{}", render_entry(&sample_entry())));
    }

    #[test]
    fn appending_after_a_file_that_already_ends_in_a_blank_line_adds_no_extra_one() {
        let existing = "@article{k, title = {Existing}}\n\n";
        let result = append_entry(existing, &sample_entry());
        assert_eq!(result, format!("{existing}{}", render_entry(&sample_entry())));
    }

    #[test]
    fn a_diff_of_before_and_after_shows_only_the_appended_entry() {
        let existing = "@article{a, title = {A}}\n\n@article{b, title = {B}}\n";
        let result = append_entry(existing, &sample_entry());
        assert!(result.starts_with(existing), "existing text must survive untouched at the front");
        assert_eq!(&result[existing.len()..], &format!("\n{}", render_entry(&sample_entry())));
    }
}
