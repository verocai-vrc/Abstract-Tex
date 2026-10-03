//! Finds `\input`, `\include`, `\subfile` (and recognises `\import` only so its two-argument
//! form is not silently ignored) directives in raw LaTeX source.
//!
//! Text in, structured data out, nothing else: no filesystem access, so every case here is a
//! plain unit test with a string literal rather than a temp directory. `graph.rs` is the module
//! that turns what this one finds into paths on disk.

/// One include-like directive found in a document's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    /// `\input{path}` / `\include{path}` / `\subfile{path}` with a plain braced argument: no
    /// macros inside the braces, so the crate can resolve it to a file on its own. `command` is
    /// which of the three wrote it (`"input"`, `"include"` or `"subfile"`): `\include` is the one
    /// that makes a chapter in LaTeX's sense (S9.7, [`crate::graph::Chapter`]).
    Include {
        command: &'static str,
        argument: String,
        line: u32,
    },
    /// Something that reads like an include directive but whose argument this scanner cannot
    /// resolve by itself: no braces at all (`\input foo`), a macro inside the braces
    /// (`\input{\chapdir/intro}`), or a command shape this crate does not model
    /// (`\import{..}{..}`, which takes a base directory and a file rather than one path).
    ///
    /// Carrying the raw text, rather than dropping the line, is what lets `IncludeGraph` say
    /// honestly that it does not know the whole document (see `IncludeGraph::is_complete`).
    Unparsed { raw: String, line: u32 },
}

impl Directive {
    /// The 1-based source line the directive starts on, whichever variant it is.
    pub fn line(&self) -> u32 {
        match self {
            Directive::Include { line, .. } => *line,
            Directive::Unparsed { line, .. } => *line,
        }
    }
}

/// Command names this scanner watches for. `\import` is here only so it is caught and reported
/// as `Unparsed` rather than never being noticed at all.
const COMMANDS: &[&str] = &["input", "include", "subfile", "import"];

/// Scan LaTeX source for include-like directives, each carrying its 1-based line number.
///
/// `%` starts a comment that runs to the end of the line, except `\%`, which is a literal
/// percent sign — the same rule TeX itself uses.
pub fn scan_includes(source: &str) -> Vec<Directive> {
    let cleaned = strip_line_comments(source);
    let bytes = cleaned.as_bytes();
    let mut directives = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'\\' => {
                // `command` names, so it is `&str` rather than the iterator's own `&&str` item —
                // `copied()` is cheap here because `&str` is `Copy` (it is just a pointer and a
                // length), so this is not cloning the text itself.
                if let Some(command) = COMMANDS
                    .iter()
                    .copied()
                    .find(|name| starts_with_command(&cleaned[i + 1..], name))
                {
                    let command_start = i;
                    let after_command = i + 1 + command.len();
                    let (directive, end) =
                        read_directive(&cleaned, command, command_start, after_command, line);
                    // An argument almost never spans a line break, but nothing stops an author
                    // writing one that does, so keep the line counter honest rather than assume.
                    line += cleaned[command_start..end].matches('\n').count() as u32;
                    directives.push(directive);
                    i = end;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    directives
}

/// True if `text` starts with the control sequence name `name` and nothing longer — so
/// `\inputxyz` is not misread as `\input` followed by `xyz`.
fn starts_with_command(text: &str, name: &str) -> bool {
    text.starts_with(name) && !text[name.len()..].starts_with(|c: char| c.is_ascii_alphabetic())
}

/// Parses the argument(s) after one recognised command name, starting at `after_command`
/// (the byte offset just past the command's letters). Returns the directive and the byte
/// offset just past whatever it consumed, so the caller's scan can resume from there.
fn read_directive(
    text: &str,
    command: &'static str,
    command_start: usize,
    after_command: usize,
    line: u32,
) -> (Directive, usize) {
    let after_ws = skip_spaces_and_tabs(text, after_command);

    if command == "import" {
        // `\import{base dir}{file}`: two arguments and a different resolution rule (the first
        // names a directory, the second a file inside it) that this crate does not implement.
        // Reporting it as unparsed is honest; guessing which of the two names a path is not.
        return unparsed_through_argument_groups(text, command_start, after_ws, 2, line);
    }

    if !text[after_ws..].starts_with('{') {
        // No braces at all: `\input foo`. Capture the bare word so the raw text at least shows
        // what was written.
        let bare_end = text[after_ws..]
            .find(char::is_whitespace)
            .map_or(text.len(), |offset| after_ws + offset);
        let raw = text[command_start..bare_end].trim_end().to_string();
        return (Directive::Unparsed { raw, line }, bare_end);
    }

    match read_braced_argument(text, after_ws) {
        Some((argument, end)) if is_literal_argument(&argument) => (
            Directive::Include {
                command,
                argument,
                line,
            },
            end,
        ),
        // Braced, but not a plain path — e.g. `\input{\chapdir/intro}`.
        Some((_, end)) => (
            Directive::Unparsed {
                raw: text[command_start..end].to_string(),
                line,
            },
            end,
        ),
        // The brace never closes. Nothing sane to resolve; take the rest of the text.
        None => (
            Directive::Unparsed {
                raw: text[command_start..].to_string(),
                line,
            },
            text.len(),
        ),
    }
}

/// Reads `groups` consecutive `{...}` arguments (skipping whitespace between them) and reports
/// the whole span, braces included, as one `Unparsed` directive. Used for `\import`, which this
/// crate recognises only well enough to avoid silently dropping it.
fn unparsed_through_argument_groups(
    text: &str,
    command_start: usize,
    mut pos: usize,
    groups: usize,
    line: u32,
) -> (Directive, usize) {
    for _ in 0..groups {
        match read_braced_argument(text, pos) {
            Some((_, end)) => pos = skip_spaces_and_tabs(text, end),
            None => break,
        }
    }
    (
        Directive::Unparsed {
            raw: text[command_start..pos].to_string(),
            line,
        },
        pos,
    )
}

/// Reads a `{...}` group starting at `pos`, which must point at the opening brace. Tracks
/// nesting depth so a brace inside the argument does not close the group early. Returns the
/// inner text and the offset just past the closing `}`, or `None` if the braces never balance.
fn read_braced_argument(text: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b'{') {
        return None;
    }
    let mut depth: u32 = 0;
    let mut i = pos;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text[pos + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// A "literal" argument is a plain path: nothing in it needs macro expansion to know what file
/// it names. `sections/intro` is literal; `\chapdir/intro` is not, because `\chapdir` is a
/// control sequence whose value this scanner cannot know. A backslash that escapes a plain
/// character right after it — `\%` — is not a control sequence and does not disqualify the
/// argument; TeX itself draws the same line between "backslash-letters" and "backslash-symbol".
///
/// `#` disqualifies it too, for a different reason: this scanner reads raw source text, so it
/// sees straight through a macro *definition* to whatever `\input` sits in its body —
/// `\newcommand{\loadchapter}[1]{\input{chapters/#1}}` scans exactly as if `\input{chapters/#1}`
/// had been written at top level, even though `#1` is a parameter placeholder that only means
/// something once `\loadchapter{...}` is actually invoked. Treating `chapters/#1` as a literal
/// path would silently resolve to a file that can never exist (`chapters/#1.tex`) while marking
/// the graph complete — reported by reviewer on S4.1.
fn is_literal_argument(argument: &str) -> bool {
    if argument.contains('#') {
        return false;
    }
    let mut chars = argument.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if matches!(chars.peek(), Some(next) if next.is_ascii_alphabetic()) {
                return false;
            }
            chars.next(); // the escaped character itself, already accounted for
        }
    }
    true
}

fn skip_spaces_and_tabs(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    from + (rest.len() - rest.trim_start_matches([' ', '\t']).len())
}

/// Removes `%...` to end of line from every line. `\%` is a literal percent, not a comment start
/// — implemented as "a backslash escapes whatever comes right after it", which also correctly
/// leaves `\\%` alone (an escaped backslash, then a real comment) without special-casing it.
/// Newlines are kept in place so line numbers computed from the result still match the input.
fn strip_line_comments(source: &str) -> String {
    let mut cleaned = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_comment = false;

    while let Some(c) = chars.next() {
        if c == '\n' {
            in_comment = false;
            cleaned.push(c);
            continue;
        }
        if in_comment {
            continue;
        }
        if c == '\\' {
            cleaned.push(c);
            if let Some(&next) = chars.peek() {
                if next != '\n' {
                    cleaned.push(next);
                    chars.next();
                }
            }
            continue;
        }
        if c == '%' {
            in_comment = true;
            continue;
        }
        cleaned.push(c);
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_input_include_and_subfile_with_their_lines() {
        let source = "\\input{preamble}\n\\include{sections/intro}\n\\subfile{sections/appendix}\n";
        let directives = scan_includes(source);
        assert_eq!(
            directives,
            vec![
                Directive::Include {
                    command: "input",
                    argument: "preamble".into(),
                    line: 1
                },
                Directive::Include {
                    command: "include",
                    argument: "sections/intro".into(),
                    line: 2
                },
                Directive::Include {
                    command: "subfile",
                    argument: "sections/appendix".into(),
                    line: 3
                },
            ]
        );
    }

    #[test]
    fn strips_comments_but_keeps_an_escaped_percent() {
        let source = "\\input{a} % \\input{commented}\n\\input{b\\%c}\n";
        let directives = scan_includes(source);
        assert_eq!(
            directives,
            vec![
                Directive::Include {
                    command: "input",
                    argument: "a".into(),
                    line: 1
                },
                // `\%` is a literal percent inside the argument; it is not a backslash-macro,
                // so this is still a resolvable literal path.
                Directive::Include {
                    command: "input",
                    argument: "b\\%c".into(),
                    line: 2
                },
            ]
        );
    }

    #[test]
    fn a_backslash_before_a_real_comment_does_not_hide_it() {
        // `\\` is an escaped backslash; the `%` right after it is a genuine comment start.
        let source = "\\input{a}\\\\ % \\input{b}\n";
        assert_eq!(
            scan_includes(source),
            vec![Directive::Include {
                command: "input",
                argument: "a".into(),
                line: 1
            }]
        );
    }

    #[test]
    fn a_bare_word_argument_is_unparsed() {
        let directives = scan_includes("\\input foo\n");
        assert_eq!(
            directives,
            vec![Directive::Unparsed {
                raw: "\\input foo".into(),
                line: 1
            }]
        );
    }

    #[test]
    fn a_macro_inside_braces_is_unparsed() {
        let directives = scan_includes("\\input{\\chapdir/intro}\n");
        assert_eq!(
            directives,
            vec![Directive::Unparsed {
                raw: "\\input{\\chapdir/intro}".into(),
                line: 1
            }]
        );
    }

    #[test]
    fn a_macro_parameter_placeholder_is_unparsed() {
        // The literal text of a macro body, e.g.
        // `\newcommand{\loadchapter}[1]{\input{chapters/#1}}` — `#1` is not a path component.
        let directives = scan_includes("\\input{chapters/#1}\n");
        assert_eq!(
            directives,
            vec![Directive::Unparsed {
                raw: "\\input{chapters/#1}".into(),
                line: 1
            }]
        );
    }

    #[test]
    fn import_is_always_unparsed() {
        let directives = scan_includes("\\import{sections/}{intro}\n");
        assert_eq!(
            directives,
            vec![Directive::Unparsed {
                raw: "\\import{sections/}{intro}".into(),
                line: 1
            }]
        );
    }

    #[test]
    fn a_similarly_named_command_is_not_mistaken_for_input() {
        assert_eq!(scan_includes("\\inputenc{utf8}\n"), vec![]);
    }

    #[test]
    fn an_unclosed_brace_is_unparsed_rather_than_panicking() {
        let directives = scan_includes("\\input{sections/intro");
        assert_eq!(
            directives,
            vec![Directive::Unparsed {
                raw: "\\input{sections/intro".into(),
                line: 1
            }]
        );
    }
}
