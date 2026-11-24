//! Making text that came from somewhere else safe to put between the braces of a `.bib` field.
//!
//! BibTeX copies a field's text into the `.bbl` as TeX, so a bare `&` in a title is an alignment
//! tab, a `%` swallows the rest of its line, a `_` outside maths is "Missing $ inserted", and a
//! brace with no partner either ends the field early or never ends it (which loses every entry
//! after it — and BibTeX counts every brace, so a backslash does not protect one: `\{` still opens). Two kinds of source reach this crate, and they need different treatment:
//!
//! - **plain text** (Open Library's catalogue): nothing in it is TeX, so every character TeX gives
//!   a meaning to is escaped ([`escape_plain_text`]);
//! - **text written as TeX** (arXiv's titles and abstracts, where `$\Lambda$CDM` is maths and
//!   `Q\&A` is already right): what is already valid is left alone and only the mistakes are
//!   escaped ([`repair_tex_text`]).
//!
//! [`balance_braces`] is the floor under both: the renderer applies it to every value it writes, so
//! whatever a caller hands in, the `.bib` file stays parseable.
//!
//! It must never read a file or make a request, like the rest of this crate.

/// Plain text turned into TeX that prints the same characters. Used for sources that are not TeX at
/// all, where `&` is an ampersand and `\` is a backslash and nothing is markup.
pub(crate) fn escape_plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        match character {
            // The characters TeX would otherwise read as syntax: a backslash before them prints them.
            '&' | '%' | '$' | '#' | '_' => {
                out.push('\\');
                out.push(character);
            }
            // These have no escape BibTeX would accept (it counts a `\{` as a brace like any other),
            // so a named command prints each, and its own braces pair up.
            '{' => out.push_str("\\textbraceleft{}"),
            '}' => out.push_str("\\textbraceright{}"),
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(character),
        }
    }
    out
}

/// Text that is *meant* to be TeX, with its mistakes escaped: an unescaped `&`, `%`, `#` or `_`
/// outside `$…$`, a `$` with no partner, and braces with no partner. Valid TeX passes through
/// unchanged, so running it twice changes nothing.
pub(crate) fn repair_tex_text(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    // An odd number of maths delimiters means one has no partner and there is no telling which, so
    // none of them opens maths: each is escaped and the text is repaired as plain prose.
    let delimiters_pair_up = count_maths_delimiters(&characters).is_multiple_of(2);
    let mut out = String::with_capacity(text.len() + 8);
    let mut in_maths = false;
    // The character after a backslash is part of that backslash's command (`\&`, `\$`, `\\`), not
    // a character of its own, so it is copied as it is.
    let mut after_backslash = false;
    let mut position = 0;
    while position < characters.len() {
        let character = characters[position];
        position += 1;
        if after_backslash {
            out.push(character);
            after_backslash = false;
            continue;
        }
        match character {
            '\\' => {
                out.push(character);
                after_backslash = true;
            }
            '$' if delimiters_pair_up => {
                in_maths = !in_maths;
                out.push(character);
                // `$$` is one delimiter (display maths), not two.
                if characters.get(position) == Some(&'$') {
                    out.push('$');
                    position += 1;
                }
            }
            '$' => out.push_str("\\$"),
            // Wrong in text and in maths alike (`\&`, `\%` and `\#` are valid in both).
            '&' | '%' | '#' => {
                out.push('\\');
                out.push(character);
            }
            // A subscript is what `_` is for inside maths.
            '_' if !in_maths => out.push_str("\\_"),
            _ => out.push(character),
        }
    }
    balance_braces(&out)
}

/// How many maths delimiters `characters` holds: each unescaped `$`, with `$$` counted once.
fn count_maths_delimiters(characters: &[char]) -> usize {
    let mut count = 0;
    let mut after_backslash = false;
    let mut position = 0;
    while position < characters.len() {
        let character = characters[position];
        position += 1;
        if after_backslash {
            after_backslash = false;
        } else if character == '\\' {
            after_backslash = true;
        } else if character == '$' {
            count += 1;
            if characters.get(position) == Some(&'$') {
                position += 1;
            }
        }
    }
    count
}

/// `text` with every brace that has no partner replaced by the command that prints it
/// (`\textbraceleft{}`, `\textbraceright{}`). Text whose braces already pair up comes back unchanged.
///
/// Every brace counts, escaped or not: BibTeX does not know that `\{` is "a literal brace", so a
/// lone `\{` leaves the field open exactly as a lone `{` does. A backslash directly before an
/// unpaired brace is part of the replacement, not left in front of it.
pub(crate) fn balance_braces(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    // Positions of `{` still waiting for a `}`, and of the braces found to have no partner.
    let mut waiting: Vec<usize> = Vec::new();
    let mut unpaired: Vec<usize> = Vec::new();
    for (position, &character) in characters.iter().enumerate() {
        match character {
            '{' => waiting.push(position),
            // `pop` answers `None` when no `{` is waiting: this `}` has no partner.
            '}' if waiting.pop().is_none() => unpaired.push(position),
            _ => {}
        }
    }
    // A `{` never closed is just as unpaired as a `}` never opened.
    unpaired.extend(waiting);
    if unpaired.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + unpaired.len() * 16);
    for (position, &character) in characters.iter().enumerate() {
        if !unpaired.contains(&position) {
            out.push(character);
            continue;
        }
        // `\{` with nothing before the backslash that escapes it: the backslash belongs to the brace.
        let backslashes_before = characters[..position]
            .iter()
            .rev()
            .take_while(|c| **c == '\\')
            .count();
        if backslashes_before % 2 == 1 {
            out.pop();
        }
        out.push_str(if character == '{' {
            "\\textbraceleft{}"
        } else {
            "\\textbraceright{}"
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_has_every_tex_special_escaped() {
        assert_eq!(escape_plain_text("John Wiley & Sons"), r"John Wiley \& Sons");
        assert_eq!(escape_plain_text("50% of #1 in a_b"), r"50\% of \#1 in a\_b");
        assert_eq!(
            escape_plain_text("cost $5 {x} ~ ^"),
            r"cost \$5 \textbraceleft{}x\textbraceright{} \textasciitilde{} \textasciicircum{}"
        );
        assert_eq!(escape_plain_text(r"a\b"), r"a\textbackslash{}b");
    }

    #[test]
    fn plain_text_that_needs_nothing_is_unchanged_including_accents() {
        assert_eq!(escape_plain_text("João Gonçalves, 2020"), "João Gonçalves, 2020");
    }

    #[test]
    fn a_bare_ampersand_percent_hash_or_underscore_in_tex_text_is_escaped() {
        assert_eq!(repair_tex_text("Learning from Q&A"), r"Learning from Q\&A");
        assert_eq!(repair_tex_text("50% faster"), r"50\% faster");
        assert_eq!(repair_tex_text("issue #4"), r"issue \#4");
        assert_eq!(repair_tex_text("a_b"), r"a\_b");
    }

    #[test]
    fn valid_tex_is_left_alone() {
        for text in [
            r"Q\&A",
            r"$\Lambda$CDM cosmology",
            r"$x_i^2$ and $$y_j$$",
            r"The {BERT} model",
            r"escaped \{ and \} braces",
            r"50\% faster",
            "plain words",
        ] {
            assert_eq!(repair_tex_text(text), text, "{text}");
        }
    }

    #[test]
    fn an_underscore_inside_maths_is_not_escaped_but_one_outside_is() {
        assert_eq!(repair_tex_text("$x_i$ and a_b"), r"$x_i$ and a\_b");
    }

    #[test]
    fn a_dollar_with_no_partner_is_text_and_everything_is_escaped_as_text() {
        assert_eq!(repair_tex_text("costs $5 and a_b"), r"costs \$5 and a\_b");
    }

    #[test]
    fn braces_with_no_partner_are_escaped_and_pairs_are_kept() {
        assert_eq!(repair_tex_text("a } b"), r"a \textbraceright{} b");
        assert_eq!(repair_tex_text("{a"), r"\textbraceleft{}a");
        assert_eq!(repair_tex_text("{a} {b"), r"{a} \textbraceleft{}b");
        assert_eq!(repair_tex_text("}{"), r"\textbraceright{}\textbraceleft{}");
    }

    #[test]
    fn repairing_twice_changes_nothing() {
        for text in ["Q&A 50% $x_i$ a_b {", "costs $5", "} a { b"] {
            let once = repair_tex_text(text);
            assert_eq!(repair_tex_text(&once), once, "{text}");
        }
    }

    #[test]
    fn balance_braces_only_touches_unpaired_ones() {
        assert_eq!(balance_braces("{a} b"), "{a} b");
        assert_eq!(
            balance_braces(r"\{a\}"),
            r"\{a\}",
            "an escaped pair still pairs up for BibTeX"
        );
        assert_eq!(
            balance_braces("a } b { c"),
            r"a \textbraceright{} b \textbraceleft{} c"
        );
        assert_eq!(balance_braces("{{a}"), r"\textbraceleft{}{a}");
        // A lone escaped brace is still a brace to BibTeX; the backslash goes with it.
        assert_eq!(balance_braces(r"x \{ y"), r"x \textbraceleft{} y");
        assert_eq!(balance_braces(r"x \} y"), r"x \textbraceright{} y");
    }
}
