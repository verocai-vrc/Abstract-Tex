//! A field's value as written, and how it resolves to text.
//!
//! BibTeX lets a value be `{braced}`, `"quoted"`, a bare number, or the name of an `@string`
//! macro, and lets several of those be joined with `#`: `month = jan`, `journal = ieee # " Trans."`.
//! This module keeps that structure ([`Value`] is a list of [`ValuePart`]s) rather than
//! flattening it at parse time, because the whole point of this crate is that the file is the
//! author's — a tool that rewrote `jan` as `{January}` on every save would be taking the file
//! hostage. Resolution happens on request, in [`Value::resolve_with`], against whatever macro
//! table the caller has; [`crate::Bibliography::resolve`] supplies the file's own.
//!
//! It must never read a file or know about the editor (see `lib.rs`).

/// One piece of a field's value, between `#` signs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum ValuePart {
    /// `{...}` — the text between the outer braces, inner braces kept. This is the form every
    /// export tool writes and the only one that can contain a `"`.
    Braced(String),
    /// `"..."` — the text between the quotes. Braces inside are balanced and kept, and a `"`
    /// inside braces does not end the string, per BibTeX's own rule.
    Quoted(String),
    /// A bare run of digits, `year = 2019`. Kept as text: a `year` of `2019a` is not a number
    /// but `2019` is, and neither needs arithmetic here.
    Number(String),
    /// A bare name, resolved against the file's `@string` definitions and BibTeX's built-in
    /// month macros. Stored as written; macro names compare case-insensitively.
    Macro(String),
}

/// A field's whole value: one or more parts joined with `#`. Almost always exactly one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Value {
    /// In the order written.
    pub parts: Vec<ValuePart>,
}

impl Value {
    /// The macro names this value refers to, as written, in order. A health check that wants
    /// to report an undefined `@string` walks these and asks the bibliography for each.
    pub fn macros(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().filter_map(|part| match part {
            ValuePart::Macro(name) => Some(name.as_str()),
            _ => None,
        })
    }

    /// The value as text, with each macro replaced by whatever `lookup` returns for it. A macro
    /// `lookup` does not know becomes the empty string, which is what BibTeX itself does (with a
    /// warning); [`Value::macros`] is how a caller finds those beforehand.
    ///
    /// Runs of whitespace inside braced and quoted text — including the line breaks export
    /// tools put in long abstracts — collapse to one space, again matching BibTeX. Inner braces
    /// are kept: `{DNA}` in a title means "do not change my case", and only a caller that
    /// knows what it is rendering for can decide what to do with that.
    ///
    /// `lookup` is a closure so this module need not know where macros come from — the
    /// bibliography's own `@string`s, a merged table across several files, or a test's fixed
    /// map all fit the same shape.
    pub fn resolve_with(&self, mut lookup: impl FnMut(&str) -> Option<String>) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                ValuePart::Braced(text) | ValuePart::Quoted(text) => push_collapsed(&mut out, text),
                ValuePart::Number(digits) => out.push_str(digits),
                ValuePart::Macro(name) => {
                    if let Some(expansion) = lookup(name) {
                        push_collapsed(&mut out, &expansion);
                    }
                }
            }
        }
        out
    }
}

/// Append `text` to `out` with every run of whitespace reduced to a single space.
fn push_collapsed(out: &mut String, text: &str) {
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
    }
    // A value that ends in whitespace (`"Smith "`) keeps one trailing space, as BibTeX does
    // when concatenating — dropping it would glue `"Smith " # "Jones"` into `SmithJones`.
    if pending_space && !out.is_empty() {
        out.push(' ');
    }
}

/// BibTeX's twelve built-in month macros. Every standard `.bst` style defines these, so a
/// `.bib` file uses `month = jan` without an `@string` for it, and a parser that did not know
/// them would report twelve undefined macros in every second bibliography.
pub fn builtin_month(name: &str) -> Option<&'static str> {
    const MONTHS: [(&str, &str); 12] = [
        ("jan", "January"),
        ("feb", "February"),
        ("mar", "March"),
        ("apr", "April"),
        ("may", "May"),
        ("jun", "June"),
        ("jul", "July"),
        ("aug", "August"),
        ("sep", "September"),
        ("oct", "October"),
        ("nov", "November"),
        ("dec", "December"),
    ];
    MONTHS
        .iter()
        .find(|(short, _)| short.eq_ignore_ascii_case(name))
        .map(|(_, long)| *long)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_macros(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_single_braced_part_resolves_to_its_text() {
        let value = Value {
            parts: vec![ValuePart::Braced("Hello {World}".into())],
        };
        assert_eq!(value.resolve_with(no_macros), "Hello {World}");
    }

    #[test]
    fn whitespace_runs_collapse_but_inner_braces_survive() {
        let value = Value {
            parts: vec![ValuePart::Braced("A   long\n  title {DNA}".into())],
        };
        assert_eq!(value.resolve_with(no_macros), "A long title {DNA}");
    }

    #[test]
    fn concatenation_joins_parts_in_order() {
        let value = Value {
            parts: vec![
                ValuePart::Macro("ieee".into()),
                ValuePart::Quoted(" Trans. ".into()),
                ValuePart::Number("12".into()),
            ],
        };
        let text = value.resolve_with(|name| (name == "ieee").then(|| "IEEE".to_string()));
        assert_eq!(text, "IEEE Trans. 12");
    }

    #[test]
    fn an_unknown_macro_becomes_nothing_but_is_still_listed() {
        let value = Value {
            parts: vec![ValuePart::Macro("nosuch".into()), ValuePart::Quoted("x".into())],
        };
        assert_eq!(value.resolve_with(no_macros), "x");
        assert_eq!(value.macros().collect::<Vec<_>>(), vec!["nosuch"]);
    }

    #[test]
    fn months_are_known_in_any_case() {
        assert_eq!(builtin_month("jan"), Some("January"));
        assert_eq!(builtin_month("DEC"), Some("December"));
        assert_eq!(builtin_month("january"), None);
    }
}
