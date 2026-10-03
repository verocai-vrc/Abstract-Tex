//! Filling a template's `{{field}}` placeholders with what the author typed.
//!
//! This module owns two things: turning a typed value into text that is safe inside a `.tex`
//! file, and replacing placeholders in one file's text. It must never touch the disk, and it
//! never invents a placeholder syntax beyond `{{name}}`: a `{{` that is not followed by a
//! lowercase name and `}}` is ordinary LaTeX (`\title{{\large A}}`) and is left exactly as written.

use std::collections::BTreeMap;

/// Make `value` safe to paste into a `.tex` file as running text.
///
/// An author who types `R&D costs 50%` into a title box must get a title that says so, not a
/// document that stops compiling at the `&`. Every character LaTeX treats as a command is
/// replaced by the command that prints it. Line breaks and tabs become a space: the form fields
/// are one line, and a stray newline inside `\title{…}` would end a paragraph there.
pub(crate) fn escape_latex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str(r"\textbackslash{}"),
            '{' => escaped.push_str(r"\{"),
            '}' => escaped.push_str(r"\}"),
            '$' | '&' | '%' | '#' | '_' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '~' => escaped.push_str(r"\textasciitilde{}"),
            '^' => escaped.push_str(r"\textasciicircum{}"),
            '\n' | '\r' | '\t' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Replace every `{{name}}` in `text` with the escaped value of `name`.
///
/// `Err(name)` means the text used a placeholder `values` has no entry for. The caller knows
/// which template and file this was and puts that in the sentence; this function does not.
pub(crate) fn fill(text: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut output = String::with_capacity(text.len());
    // `rest` is the part of `text` not yet copied to `output`. A `&str` slice is a view into the
    // original string, so walking forward through it copies nothing until we push.
    let mut rest = text;

    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        match placeholder_name(after_open) {
            Some(name) => {
                let value = values.get(name).ok_or_else(|| name.to_string())?;
                output.push_str(&rest[..open]);
                output.push_str(&escape_latex(value));
                rest = &after_open[name.len() + 2..]; // skip the name and the closing `}}`
            }
            None => {
                // Not a placeholder. Keep the first brace and look again from the second, so
                // `{{{title}}}` is read as `{` followed by `{{title}}` and then `}`.
                output.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    output.push_str(rest);
    Ok(output)
}

/// Whether `id` is allowed as a field name: a lowercase letter, then lowercase letters, digits
/// and underscores.
pub(crate) fn is_valid_field_id(id: &str) -> bool {
    let mut characters = id.chars();
    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// The name at the start of `text` if it is followed by `}}`, else `None`.
fn placeholder_name(text: &str) -> Option<&str> {
    let name_length = text
        .find(|c: char| !matches!(c, 'a'..='z' | '0'..='9' | '_'))
        .unwrap_or(text.len());
    let name = &text[..name_length];
    if is_valid_field_id(name) && text[name_length..].starts_with("}}") {
        Some(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn every_special_character_is_escaped() {
        assert_eq!(escape_latex(r"R&D 50% $5 #1 a_b {x} ~ ^ \"), r"R\&D 50\% \$5 \#1 a\_b \{x\} \textasciitilde{} \textasciicircum{} \textbackslash{}");
    }

    #[test]
    fn line_breaks_become_spaces_and_accents_pass_through() {
        assert_eq!(escape_latex("Ana\nMaría\tSilva"), "Ana María Silva");
    }

    #[test]
    fn a_placeholder_is_replaced_by_the_escaped_value() {
        let filled = fill(r"\title{{{title}}} by {{author}}", &values(&[("title", "R&D"), ("author", "Ada")]));
        assert_eq!(filled.unwrap(), r"\title{R\&D} by Ada");
    }

    #[test]
    fn braces_that_are_not_placeholders_are_left_alone() {
        let text = r"\textbf{{\large A}} {{ not }} {{Upper}} {{x";
        assert_eq!(fill(text, &values(&[])).unwrap(), text);
    }

    #[test]
    fn an_undeclared_placeholder_names_itself() {
        assert_eq!(fill("{{author}}", &values(&[("title", "T")])), Err("author".to_string()));
    }

    #[test]
    fn a_value_that_looks_like_a_placeholder_is_not_filled_again() {
        // The escaped braces can no longer form `{{`, and the scan never revisits output anyway.
        let filled = fill("{{title}}", &values(&[("title", "{{author}}")])).unwrap();
        assert_eq!(filled, r"\{\{author\}\}");
    }

    #[test]
    fn field_ids_start_with_a_letter() {
        assert!(is_valid_field_id("title") && is_valid_field_id("a_1"));
        assert!(!is_valid_field_id("") && !is_valid_field_id("1a") && !is_valid_field_id("Title") && !is_valid_field_id("a-b"));
    }
}
