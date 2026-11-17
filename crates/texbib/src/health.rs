//! Two of the five bibliography health checks from DESIGN.md §5.4 that need nothing but one
//! parsed `.bib` file: a missing required field for the entry's type, and a page range written
//! with a hyphen where BibTeX wants an en-dash. The other three — undefined citation, never
//! cited, duplicate DOI — need the project's `.tex` files and entries from every `.bib` file at
//! once, which only `src-tauri/src/bibliography.rs`'s `BibliographyIndex` has; they are methods
//! there, not here (S8.3's own card explains the split).
//!
//! Pure — a [`Bibliography`] in, [`Finding`]s out — the same "text in, data out" rule the rest of
//! this crate follows (`lib.rs`), so it is testable with a literal string and ships in the
//! published crate with no extra feature flag.
//!
//! `BibliographyIndex` (the type that owns the other three checks) lives in `src-tauri`, not in
//! this crate — it is app state, not something a script using this crate on its own would have.

use crate::{Bibliography, Entry, Span, ValuePart};

/// How much the author should care. Matches `texlog::rules::Severity`'s two levels — this crate
/// and that one describe unrelated problems, but a health check is either "this bibliography will
/// not build the citation the author expects" (error) or "this is probably a mistake, but the
/// bibliography still works" (warning), the same two-way split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// A problem that keeps the entry from doing what its type promises.
    Error,
    /// Correct enough to build, but worth a second look.
    Warning,
}

/// One health-check result, with a place to click to. `at` is a [`Span`] rather than a line,
/// like every other position this crate hands out (`lib.rs`'s "byte span on everything" rule);
/// the caller converts to a line the way it already does for [`crate::ParseError`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Which check found this, so a caller can filter or explain further: `"missing-field"` or
    /// `"page-range-dash"` for the two checks in this module.
    pub rule: &'static str,
    /// How much the author should care.
    pub severity: Severity,
    /// One complete sentence, ready to show without further wording (DESIGN.md §2, rule 3).
    pub message: String,
    /// The entry's citation key, for a caller that wants to group findings by entry.
    pub key: String,
    /// Where to point the author: the field's span when the problem is one field, else the
    /// whole entry's span.
    pub at: Span,
}

/// Fields BibTeX's own manual requires for a type, checked case-insensitively against
/// [`Entry::entry_type`] and [`Entry::field`]. Scoped to the types DESIGN.md and the fixture
/// corpus actually use (S8.3's card), not BibTeX's complete appendix B table — a type not listed
/// here is simply never checked, which is the same "a rule never guesses" discipline
/// `texlog::rules` follows for a log line it does not recognise.
const REQUIRED_FIELDS: &[(&str, &[&str])] = &[
    ("article", &["author", "title", "journal", "year"]),
    ("book", &["title", "year"]),
    ("inproceedings", &["author", "title", "booktitle", "year"]),
    ("incollection", &["author", "title", "booktitle", "year"]),
    ("phdthesis", &["author", "title", "school", "year"]),
    ("mastersthesis", &["author", "title", "school", "year"]),
    ("techreport", &["author", "title", "institution", "year"]),
    ("misc", &[]),
];

/// Every entry-level problem in `bibliography`: a missing required field per entry per missing
/// field, then a wrong-dash page range per entry that has one. Order is file order, and within
/// one entry, missing fields before the dash — stable, so a caller building a list does not need
/// its own tie-break.
pub fn check(bibliography: &Bibliography) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in bibliography.entries() {
        findings.extend(missing_required_fields(entry));
    }
    for entry in bibliography.entries() {
        if let Some(finding) = wrong_dash_in_page_range(entry) {
            findings.push(finding);
        }
    }
    findings
}

/// `book`'s `editor` stands in for `author` the same way [`crate`]'s own `EntrySummary` treats
/// them (S7.2) — an edited volume with no listed author is not missing anything.
fn has_author_or_editor(entry: &Entry) -> bool {
    entry.field("author").is_some() || entry.field("editor").is_some()
}

/// BibLaTeX's name for a field next to BibTeX's, for the fields [`REQUIRED_FIELDS`] asks for: an
/// article with `journaltitle` has its journal, one with `date` has its year, a thesis with
/// `institution` has its school. The table lists BibTeX's names, so a BibLaTeX bibliography was
/// reported as missing fields it has, as errors (review finding R8.23).
const BIBLATEX_NAMES: &[(&str, &str)] = &[
    ("journal", "journaltitle"),
    ("year", "date"),
    ("school", "institution"),
];

/// Whether `entry` has `name`, or the BibLaTeX name for it.
fn has_field(entry: &Entry, name: &str) -> bool {
    entry.field(name).is_some()
        || BIBLATEX_NAMES
            .iter()
            .any(|(bibtex, biblatex)| *bibtex == name && entry.field(biblatex).is_some())
}

fn missing_required_fields(entry: &Entry) -> Vec<Finding> {
    let Some((_, required)) = REQUIRED_FIELDS.iter().find(|(ty, _)| entry.is_type(ty)) else {
        return Vec::new();
    };
    // An entry that names a parent (`crossref` in BibTeX and BibLaTeX, `xdata` in BibLaTeX) may take
    // any field it lacks from it — the usual way to cite several papers of one proceedings leaves
    // `booktitle` and `year` to the parent. This check reads one file and the parent may be in
    // another, so it cannot tell what is missing; an entry that names one is not checked, rather than
    // reported as missing what it inherits (review finding R8.23).
    if entry.field("crossref").is_some() || entry.field("xdata").is_some() {
        return Vec::new();
    }
    required
        .iter()
        .filter(|field| {
            if **field == "author" && has_author_or_editor(entry) {
                return false;
            }
            !has_field(entry, field)
        })
        .map(|field| Finding {
            rule: "missing-field",
            severity: Severity::Error,
            message: format!(
                "'{}' has no '{field}' field, which every @{} entry needs.",
                entry.key, entry.entry_type
            ),
            key: entry.key.clone(),
            at: entry.span,
        })
        .collect()
}

/// `pages = {12-15}`: one hyphen between two numbers, which BibTeX renders as a single hyphen
/// rather than the en-dash a range needs (`12--15`). Only a single hyphen between digits counts —
/// `12--15` (already correct), `7` (no range), and `12-15-20` or a non-numeric page label like
/// `e12345` (not a simple range) are all left alone, since this check only fires when the fix is
/// unambiguous: turn that one hyphen into two.
///
/// Reads the field's own written text, not [`Bibliography::resolve`] — a page range is never a
/// `@string` macro in practice, and the card asks for "a regex over the raw field text before
/// resolution" so this check works even on an entry whose macro table this function never sees.
fn wrong_dash_in_page_range(entry: &Entry) -> Option<Finding> {
    let field = entry.field("pages")?;
    let text: String = field
        .value
        .parts
        .iter()
        .map(|part| match part {
            ValuePart::Braced(text) | ValuePart::Quoted(text) => text.as_str(),
            ValuePart::Number(digits) => digits.as_str(),
            ValuePart::Macro(_) => "",
        })
        .collect();
    let trimmed = text.trim();
    let (before, after) = trimmed.split_once('-')?;
    if before.is_empty() || after.is_empty() {
        return None;
    }
    if !before.chars().all(|c| c.is_ascii_digit()) || !after.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // A second `-` means this was already `--` (an en-dash) or something stranger than a plain
    // range; `split_once` already consumed the first one, so a leading `-` on `after` is `--`.
    if after.starts_with('-') {
        return None;
    }
    Some(Finding {
        rule: "page-range-dash",
        severity: Severity::Warning,
        message: format!(
            "'{}' has pages = {{{trimmed}}}, a hyphen; a page range wants an en-dash: {before}--{after}.",
            entry.key
        ),
        key: entry.key.clone(),
        at: field.span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn a_clean_entry_has_no_findings() {
        let bib =
            parse("@article{k, author = {A}, title = {T}, journal = {J}, year = 2019, pages = {12--15}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_missing_required_field_is_named() {
        let bib = parse("@article{k, author = {A}, title = {T}, year = 2019}");
        let findings = check(&bib);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "missing-field");
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(
            findings[0].message.contains("'journal'"),
            "{}",
            findings[0].message
        );
        assert_eq!(findings[0].key, "k");
    }

    #[test]
    fn several_missing_fields_are_each_their_own_finding() {
        let bib = parse("@article{k, title = {T}}");
        let findings = check(&bib);
        let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
        assert_eq!(
            rules,
            vec!["missing-field", "missing-field", "missing-field"],
            "{findings:#?}"
        );
    }

    #[test]
    fn editor_stands_in_for_author_on_a_book() {
        let bib = parse("@book{k, editor = {E}, title = {T}, year = 2019}");
        assert_eq!(check(&bib), Vec::new());
    }

    // Review finding R8.23: BibLaTeX's own names (`journaltitle`, `date`, `institution`) and a `crossref`
    // or `xdata` parent were all reported as missing fields, as errors, in complete entries.
    #[test]
    fn a_biblatex_article_with_journaltitle_and_date_is_complete() {
        let bib = parse("@article{k, author = {A}, title = {T}, journaltitle = {J}, date = {2019-05-01}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_biblatex_thesis_with_institution_is_complete() {
        let bib = parse("@phdthesis{k, author = {A}, title = {T}, institution = {U}, date = {2019}}");
        assert_eq!(check(&bib), Vec::new());
        let bib = parse("@mastersthesis{k, author = {A}, title = {T}, institution = {U}, year = 2019}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_name_is_still_missing_when_neither_it_nor_its_biblatex_name_is_there() {
        let bib = parse("@article{k, author = {A}, title = {T}, journaltitle = {J}}");
        let findings = check(&bib);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(findings[0].message.contains("'year'"), "{}", findings[0].message);
        let bib = parse("@article{k, author = {A}, title = {T}, date = {2019}}");
        let findings = check(&bib);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].message.contains("'journal'"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn an_entry_with_a_crossref_may_take_its_fields_from_the_parent() {
        // The standard BibTeX way to cite several papers of one proceedings: `booktitle` and `year`
        // are the parent's. This crate reads one file and cannot see a parent that is in another.
        let bib = parse("@inproceedings{a, author = {A}, title = {T}, pages = {1--2}, crossref = {conf}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn an_entry_with_xdata_may_take_its_fields_from_the_parent() {
        let bib = parse("@article{a, author = {A}, title = {T}, xdata = {ieeetcbb}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_page_range_is_still_checked_on_an_entry_with_a_crossref() {
        let bib = parse("@inproceedings{a, author = {A}, title = {T}, pages = {1-2}, crossref = {conf}}");
        let rules: Vec<&str> = check(&bib).iter().map(|finding| finding.rule).collect();
        assert_eq!(rules, vec!["page-range-dash"]);
    }

    #[test]
    fn a_type_outside_the_table_is_never_checked() {
        let bib = parse("@online{k, title = {T}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_hyphenated_page_range_is_flagged_with_the_fix_spelled_out() {
        let bib = parse("@misc{k, pages = {12-15}}");
        let findings = check(&bib);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "page-range-dash");
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("12--15"), "{}", findings[0].message);
    }

    #[test]
    fn an_en_dash_range_is_not_flagged() {
        let bib = parse("@misc{k, pages = {12--15}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn a_single_page_or_non_numeric_label_is_not_flagged() {
        let bib = parse("@misc{k, pages = {12}}");
        assert_eq!(check(&bib), Vec::new());
        let bib = parse("@misc{k, pages = {e12345}}");
        assert_eq!(check(&bib), Vec::new());
        let bib = parse("@misc{k, pages = {12-15-20}}");
        assert_eq!(check(&bib), Vec::new());
    }

    #[test]
    fn findings_point_at_the_right_span() {
        let source = "@misc{k, pages = {12-15}}";
        let bib = parse(source);
        let findings = check(&bib);
        assert_eq!(findings[0].at.text(source), "pages = {12-15}");

        let source = "@article{k, title = {T}}";
        let bib = parse(source);
        let findings = check(&bib);
        assert_eq!(findings[0].at.text(source), source);
    }
}
