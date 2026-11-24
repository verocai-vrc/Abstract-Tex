//! The title of a document, read out of its `\title{…}` as a person would read it aloud (S17.3).
//!
//! **What it owns:** finding the first `\title` that says something, and turning its LaTeX into plain
//! text: accents become letters, `---` a dash, `\thanks{…}` nothing.
//!
//! **What it must never do:** fail. A title it cannot read is `None`, and an unknown command is dropped
//! and its argument kept, because half a title is still recognisable and a command name is not.
//!
//! A pass of its own rather than `prose_of`: that one exists to count words, so it keeps braces,
//! `\thanks` and `~`, and reads `Caf\'e` as `Caf'e`.

/// The document's title, or `None` when it has none that says anything.
pub fn title_of(source: &str) -> Option<String> {
    let text = without_comments(source);
    let mut position = 0;
    while let Some(offset) = text[position..].iter().position(|&c| c == '\\') {
        position += offset + 1;
        let name = command_name(&text, &mut position);
        if name != "title" {
            continue;
        }
        let mut after = position;
        skip_spaces(&text, &mut after);
        skip_optional(&text, &mut after);
        skip_spaces(&text, &mut after);
        // Looked at on a copy of the position: `\renewcommand{\title}` is followed by no group, and the
        // search goes on right behind the name.
        let Some(body) = group(&text, &mut after) else {
            continue;
        };
        let title = plain(body);
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

/// Everything TeX would read: from each `%` to the end of its line is gone, and so are the line's end
/// and the next line's indentation, which TeX skips too.
fn without_comments(source: &str) -> Vec<char> {
    let mut kept = Vec::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    while let Some(c) = characters.next() {
        match c {
            '\\' => {
                kept.push(c);
                kept.extend(characters.next()); // `\%` is a percent sign, `\\` a line break
            }
            '%' => {
                for skipped in characters.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
                while characters.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
            }
            _ => kept.push(c),
        }
    }
    kept
}

/// Read a command's name, with `position` just behind its backslash: letters, or one other character.
/// `@` counts as a letter, so that `\@title` is not taken for `\title`.
fn command_name(text: &[char], position: &mut usize) -> String {
    let start = *position;
    while text
        .get(*position)
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == '@')
    {
        *position += 1;
    }
    if *position == start && *position < text.len() {
        *position += 1;
    }
    text[start..*position].iter().collect()
}

fn skip_spaces(text: &[char], position: &mut usize) {
    while text.get(*position).is_some_and(|c| c.is_whitespace()) {
        *position += 1;
    }
}

/// Step over one `[…]`, if there is one here. Braces inside it may hold a `]`.
fn skip_optional(text: &[char], position: &mut usize) {
    if text.get(*position) != Some(&'[') {
        return;
    }
    let mut depth = 0usize;
    let mut at = *position + 1;
    while let Some(&c) = text.get(at) {
        match c {
            '\\' => at += 1,
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ']' if depth == 0 => {
                *position = at + 1;
                return;
            }
            _ => {}
        }
        at += 1;
    }
}

/// The inside of the `{…}` that starts here, leaving `position` behind it. `None`, and `position`
/// unmoved, when there is none or it is never closed.
fn group<'a>(text: &'a [char], position: &mut usize) -> Option<&'a [char]> {
    if text.get(*position) != Some(&'{') {
        return None;
    }
    let start = *position + 1;
    let mut depth = 1usize;
    let mut at = start;
    while let Some(&c) = text.get(at) {
        match c {
            '\\' => at += 1,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    *position = at + 1;
                    return Some(&text[start..at]);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// Commands whose braces hold something a person would not read aloud, and how many of them.
const DROPPED_ARGUMENTS: [(&str, usize); 17] = [
    ("thanks", 1),
    ("footnote", 1),
    ("footnotetext", 1),
    ("footnotemark", 0),
    ("label", 1),
    ("index", 1),
    ("cite", 1),
    ("vspace", 1),
    ("hspace", 1),
    ("color", 1),
    ("textcolor", 1),
    ("fontsize", 2),
    ("includegraphics", 1),
    ("href", 1),
    ("rule", 2),
    ("setlength", 2),
    // Its first group is for the page and its second is plain text for a list like ours.
    ("texorpdfstring", 1),
];

/// Commands that are themselves something to read.
const SPOKEN: [(&str, &str); 28] = [
    ("LaTeX", "LaTeX"),
    ("LaTeXe", "LaTeX2e"),
    ("TeX", "TeX"),
    ("XeLaTeX", "XeLaTeX"),
    ("ldots", "…"),
    ("dots", "…"),
    ("textellipsis", "…"),
    ("textendash", "–"),
    ("textemdash", "—"),
    ("textbackslash", "\\"),
    ("par", " "),
    ("newline", " "),
    ("linebreak", " "),
    ("quad", " "),
    ("qquad", " "),
    ("ss", "ß"),
    ("o", "ø"),
    ("O", "Ø"),
    ("ae", "æ"),
    ("AE", "Æ"),
    ("oe", "œ"),
    ("OE", "Œ"),
    ("aa", "å"),
    ("AA", "Å"),
    ("l", "ł"),
    ("L", "Ł"),
    ("i", "i"),
    ("j", "j"),
];

/// A LaTeX accent, and the letters that have one Unicode character with it, each beside the plain
/// letter. A letter that is not here keeps its plain form.
const ACCENTS: [(&str, &str, &str); 7] = [
    ("'", "aeiouyAEIOUYcCnNsSzZ", "áéíóúýÁÉÍÓÚÝćĆńŃśŚźŹ"),
    ("`", "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
    ("^", "aeiouAEIOUcCgGhHsSwWyY", "âêîôûÂÊÎÔÛĉĈĝĜĥĤŝŜŵŴŷŶ"),
    ("~", "anoANOiIuU", "ãñõÃÑÕĩĨũŨ"),
    ("\"", "aeiouyAEIOUY", "äëïöüÿÄËÏÖÜŸ"),
    ("c", "cCsStT", "çÇşŞţŢ"),
    ("v", "cCsSzZeErRnNdD", "čČšŠžŽěĚřŘňŇďĎ"),
];

/// A title's LaTeX as plain text.
fn plain(body: &[char]) -> String {
    let mut out = String::new();
    let mut position = 0;
    while let Some(&c) = body.get(position) {
        position += 1;
        match c {
            '\\' => command(body, &mut position, &mut out),
            // Groups and maths are only punctuation here: what they hold is read like the rest, which
            // is why no recursion is needed and no depth of nesting can overflow the stack.
            '{' | '}' | '$' => {}
            '~' => out.push(' '),
            '-' => {
                let dashes = 1 + body[position..].iter().take_while(|c| **c == '-').take(2).count();
                position += dashes - 1;
                out.push_str(["-", "–", "—"][dashes - 1]);
            }
            '`' | '\'' => {
                let doubled = body.get(position) == Some(&c);
                position += usize::from(doubled);
                out.push(match (c, doubled) {
                    ('`', true) => '“',
                    ('`', false) => '‘',
                    (_, true) => '”',
                    (_, false) => '’',
                });
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read one command, with `position` just behind its backslash, and write what a person would say.
fn command(body: &[char], position: &mut usize, out: &mut String) {
    let name = command_name(body, position);
    if let Some((_, plain_letters, accented)) = ACCENTS.iter().find(|(accent, _, _)| *accent == name) {
        if let Some(letter) = accented_letter(body, position) {
            let with_accent = plain_letters
                .chars()
                .position(|plain| plain == letter)
                .and_then(|index| accented.chars().nth(index));
            out.push(with_accent.unwrap_or(letter));
        }
        return;
    }
    let is_word = name.starts_with(|c: char| c.is_ascii_alphabetic());
    if !is_word {
        match name.as_str() {
            "\\" => {
                if body.get(*position) == Some(&'*') {
                    *position += 1;
                }
                skip_optional(body, position);
                out.push(' ');
            }
            "," | ";" | ":" | " " | "\n" | "\t" => out.push(' '),
            "&" | "%" | "$" | "#" | "_" | "{" | "}" => out.push_str(&name),
            _ => {} // `\-`, `\/`, `\!`: nothing to read
        }
        return;
    }
    // TeX skips the spaces behind a command's name.
    skip_spaces(body, position);
    if let Some((_, spoken)) = SPOKEN.iter().find(|(command, _)| *command == name) {
        out.push_str(spoken);
        return;
    }
    if let Some((_, groups)) = DROPPED_ARGUMENTS.iter().find(|(command, _)| *command == name) {
        if body.get(*position) == Some(&'*') {
            *position += 1;
        }
        skip_optional(body, position);
        for _ in 0..*groups {
            skip_spaces(body, position);
            group(body, position);
        }
    }
}

/// The letter an accent sits on: `e`, `{e}`, `\i` or `{\i}`.
fn accented_letter(body: &[char], position: &mut usize) -> Option<char> {
    skip_spaces(body, position);
    let mut braced = false;
    if body.get(*position) == Some(&'{') {
        braced = true;
        *position += 1;
    }
    let letter = match body.get(*position) {
        Some('\\') => {
            *position += 1;
            let name = command_name(body, position);
            skip_spaces(body, position);
            name.chars().next().filter(|c| c.is_alphabetic()) // `\i` and `\j` are the letters without a dot
        }
        Some(&c) if c.is_alphabetic() => {
            *position += 1;
            Some(c)
        }
        _ => None,
    };
    if braced && body.get(*position) == Some(&'}') {
        *position += 1;
    }
    letter
}

#[cfg(test)]
mod tests {
    use super::title_of;

    #[test]
    fn a_title_is_read_as_a_person_would() {
        let cases = [
            (r"\title{On Bees}", Some("On Bees")),
            (r"\title[Bees]{On the Life of Bees}", Some("On the Life of Bees")),
            (r"\title {On Bees\thanks{Funded by {X}.}}", Some("On Bees")),
            (
                r"\title{The {\em very} \textbf{Big} Idea}",
                Some("The very Big Idea"),
            ),
            ("\\title{One\\\\[2ex]\n  Two}", Some("One Two")),
            ("% \\title{Old}\n\\title{New % a note\n}", Some("New")),
            (
                r"\title{Salt \& Light --- a $p$-value}",
                Some("Salt & Light — a p-value"),
            ),
            (
                r#"\title{Caf\'e \`a S\~{a}o Paulo, fa\c{c}ade, \"Uber}"#,
                Some("Café à São Paulo, façade, Über"),
            ),
            (r"\title{}", None),
            (r"\title{\thanks{x}}", None),
            (r"\title{never closed", None),
            (r"\titleformat{\section}{x}{y}{z}{}", None),
            ("no title", None),
        ];
        for (source, title) in cases {
            assert_eq!(title_of(source).as_deref(), title, "{source}");
        }
    }

    #[test]
    fn the_rarer_spellings_too() {
        let cases = [
            // An accent on a dotless i, on a letter after a space, and one with no single character.
            (
                r#"\title{Mar\'\i a, na\"{\i}ve, gar\c con, \v{S}koda}"#,
                Some("María, naïve, garçon, Škoda"),
            ),
            (r"\title{\'q}", Some("q")),
            (
                r#"\title{Stra\ss e, \O resund, {\AA}ngstr\"om, \L{}\'od\'z}"#,
                Some("Straße, Øresund, Ångström, Łódź"),
            ),
            (
                r"\title{\LaTeX{} at 100\% --- pages 1--2\ldots}",
                Some("LaTeX at 100% — pages 1–2…"),
            ),
            (r"\title{``Quoted'' and it's}", Some("“Quoted” and it’s")),
            (
                r"\title{\textcolor{red}{Red} \href{http://x}{link}~end}",
                Some("Red link end"),
            ),
            (r"\title{A\footnote{note}\vspace{1cm} B}", Some("A B")),
            (
                r"\title{\texorpdfstring{$H_0$}{H0} and after}",
                Some("H0 and after"),
            ),
            (r"\renewcommand{\title}{x} \title{Real}", Some("Real")),
            (r"\title{} \title{Second}", Some("Second")),
            (r"\title{Brace \{kept\}}", Some("Brace {kept}")),
            (
                "\\title{\n  \\Large\\bfseries Big\n\n  Title\n}",
                Some("Big Title"),
            ),
        ];
        for (source, title) in cases {
            assert_eq!(title_of(source).as_deref(), title, "{source}");
        }
    }

    #[test]
    fn nothing_makes_it_panic_or_hang() {
        for source in [
            r"\title",
            r"\title[",
            r"\title{\",
            r"\title{\'",
            r"\title{\c",
            r"\title{\thanks{",
            r"\title{\thanks}",
            "\\title{%",
            r"\title{é\'é}",
        ] {
            let _ = title_of(source);
        }
        let deep = format!("\\title{}x{}", "{".repeat(50_000), "}".repeat(50_000));
        assert_eq!(title_of(&deep).as_deref(), Some("x"));
    }
}
