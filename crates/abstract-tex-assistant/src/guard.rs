//! The citation-fabrication guard (S12.2, DESIGN.md §5.5 "Hard constraint — not negotiable").
//!
//! **The rule:** the assistant may never introduce a citation that does not already exist in the
//! project's `.bib`. Language models invent plausible references, the consequences land on the
//! author at review and in public, and an editor for scientists that can do it silently is not fit
//! for purpose.
//!
//! This module owns one question — *given the text as it was and the text as it would be, does the
//! edit introduce a citation the `.bib` does not hold?* — and answers it with a [`Verdict`] that
//! names every offending byte range, so the per-hunk accept of S12.3 can refuse exactly the hunks
//! that touch one. It never reads a file, never asks a model anything, and never edits the text:
//! it only looks.
//!
//! **Why it takes both texts.** An author who asks the assistant to tighten a paragraph that
//! already holds a mistyped `\cite{smtih2020}` must not be refused for it; the guard refuses what
//! the *edit* adds. Everything an original contains — an unknown key, a `\def`, a plain-text
//! "(Smith, 2019)" — is allowed to stay, once per occurrence.
//!
//! **What counts as introducing a citation**, in the four ways a model can do it:
//!
//! 1. [`FindingKind::UnknownKey`] — a key in a cite command that no `.bib` entry has. The commands
//!    are every control word with "cite" in it, in any case ([`texbib::is_citation_command`], the
//!    very predicate the editor's own undefined-citation scan uses, so the two cannot disagree),
//!    plus the author's own wrappers if the caller passes them ([`citation_macros`] finds them).
//!    The key is compared exactly: `Smith2020` is not `smith2020`, and a zero-width space or a
//!    Cyrillic letter makes a different key.
//! 2. [`FindingKind::OwnReferenceList`] — a `\bibitem`, `thebibliography`, a `filecontents` that
//!    writes a `.bib`, or a bibliography file the project did not have: a reference list the
//!    `.bib` does not hold.
//! 3. [`FindingKind::HiddenCommand`] — anything a prose edit has no business containing and that
//!    can build a citation this scan would not see: a macro definition, `\csname`, `\catcode`,
//!    `\input`, TeX's `^^5c` spelling of a backslash.
//! 4. [`FindingKind::PlainTextCitation`] — "(Smith et al., 2019)", a reference with no `\cite` at
//!    all. A heuristic, and the one place the guard knowingly over-refuses: a false refusal costs
//!    the author a hand edit, a false pass costs them a fabricated reference.
//!
//! **What this does not do.** It cannot know whether a *known* key is the right one for the
//! sentence; that is the author's judgement, and the diff exists so they can exercise it. And it is
//! only as good as the text it is given: S12.3 must run it on the text the buffer would hold after
//! the author's accept/reject choices, not on the model's raw answer, so a combination of hunks
//! cannot make a key the guard never saw.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// What was found, and why it matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingKind {
    /// A key in a cite command that the project's `.bib` files do not have.
    UnknownKey,
    /// A reference list written into the text, or a bibliography file the project did not have.
    OwnReferenceList,
    /// A construct that can make a citation without this scan seeing it.
    HiddenCommand,
    /// A reference written in plain text, with no cite command at all.
    PlainTextCitation,
}

/// One thing the edit introduces that must not reach the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    /// Where it is in the *result* text, in bytes. For a citation, the whole command: from its
    /// backslash to its last argument. A hunk that touches any of it is refused.
    pub span: Range<usize>,
    /// The key for [`FindingKind::UnknownKey`]; for the others, the construct as written.
    pub text: String,
}

impl Finding {
    /// A sentence for the person, in the app's voice: what was held back and what to do.
    pub fn sentence(&self) -> String {
        match self.kind {
            FindingKind::UnknownKey => format!(
                "The edit cites \u{201c}{}\u{201d}, which is not an entry in your .bib files, so it was held back.",
                self.text
            ),
            FindingKind::OwnReferenceList => format!(
                "The edit writes its own reference list ({}), which your .bib files do not hold, so it was held back.",
                self.text
            ),
            FindingKind::HiddenCommand => format!(
                "The edit adds {}, which changes how commands work and could hide a citation, so it was held back. \
                 Make that change by hand if you want it.",
                self.text
            ),
            FindingKind::PlainTextCitation => format!(
                "The edit adds a reference in plain text ({}) that is not a citation of an entry in your .bib files, \
                 so it was held back.",
                self.text
            ),
        }
    }
}

/// The guard's answer for one proposed edit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verdict {
    /// In the order they appear in the result text.
    pub findings: Vec<Finding>,
}

impl Verdict {
    /// Nothing in the edit is a citation the project lacks.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The unknown keys, each once, in the order they first appear: what a reference lookup
    /// should be offered for.
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.findings
            .iter()
            .filter(|finding| finding.kind == FindingKind::UnknownKey)
            .filter(|finding| seen.insert(finding.text.clone()))
            .map(|finding| finding.text.clone())
            .collect()
    }

    /// Whether a hunk occupying `range` of the result text must be refused.
    ///
    /// A non-empty range is refused if it shares any byte with a finding. An empty one — a pure
    /// deletion, which leaves nothing of itself in the result — is refused only if it sits
    /// strictly *inside* a finding, because that is a deletion that changed the command.
    pub fn blocks(&self, range: &Range<usize>) -> bool {
        self.findings.iter().any(|finding| {
            if range.is_empty() {
                finding.span.start < range.start && range.start < finding.span.end
            } else {
                range.start < finding.span.end && finding.span.start < range.end
            }
        })
    }
}

/// The project's citation keys, and the author's own citation commands.
#[derive(Debug, Clone, Default)]
pub struct Guard {
    known_keys: HashSet<String>,
    /// Lower-case control words that are citation commands although their names do not say so.
    extra_commands: HashSet<String>,
}

impl Guard {
    /// A guard for a project whose `.bib` files hold exactly these keys.
    pub fn new(known_keys: impl IntoIterator<Item = String>) -> Self {
        Self { known_keys: known_keys.into_iter().collect(), extra_commands: HashSet::new() }
    }

    /// Also treat these control words (no backslash) as citation commands: the author's `\see`
    /// that wraps `\cite`, found by [`citation_macros`]. Without it a wrapper is invisible.
    pub fn with_citation_commands(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.extra_commands.extend(names.into_iter().map(|name| name.to_ascii_lowercase()));
        self
    }

    /// Judge an edit: `original` is the text before, `result` the text the buffer would hold.
    pub fn check(&self, original: &str, result: &str) -> Verdict {
        let mut findings = Vec::new();

        // 1. Unknown keys: those in the result that no .bib entry has and the original did not
        // already use.
        let already_used: HashSet<String> =
            self.citations_in(original).into_iter().flat_map(|citation| citation.keys).collect();
        for citation in self.citations_in(result) {
            for key in citation.keys {
                if !self.known_keys.contains(&key) && !already_used.contains(&key) {
                    findings.push(Finding { kind: FindingKind::UnknownKey, span: citation.span.clone(), text: key });
                }
            }
        }

        // 2 and 3. Constructs: flagged when the edit has more of one than the original did.
        let before = constructs_in(original);
        let after = constructs_in(result);
        let mut count_before: HashMap<&str, usize> = HashMap::new();
        for construct in &before {
            *count_before.entry(construct.label.as_str()).or_default() += 1;
        }
        let mut count_after: HashMap<&str, usize> = HashMap::new();
        for construct in &after {
            *count_after.entry(construct.label.as_str()).or_default() += 1;
        }
        for construct in &after {
            if count_after[construct.label.as_str()] > count_before.get(construct.label.as_str()).copied().unwrap_or(0) {
                findings.push(Finding { kind: construct.kind, span: construct.span.clone(), text: construct.shown.clone() });
            }
        }

        // 4. References in plain text that the original did not already contain.
        let old_plain: HashSet<String> = plain_text_citations(original).into_iter().map(|(_, text)| text).collect();
        for (span, text) in plain_text_citations(result) {
            if !old_plain.contains(&text) {
                findings.push(Finding { kind: FindingKind::PlainTextCitation, span, text });
            }
        }

        findings.sort_by_key(|finding| (finding.span.start, finding.span.end));
        Verdict { findings }
    }

    fn is_citation_command(&self, name: &str) -> bool {
        texbib::is_citation_command(name) || self.extra_commands.contains(&name.to_ascii_lowercase())
    }

    /// Every citation command in `text`, with the keys it names.
    fn citations_in(&self, text: &str) -> Vec<Citation> {
        let mut found = Vec::new();
        for word in control_words(text) {
            if !self.is_citation_command(word.name) {
                continue;
            }
            let (keys, end) = read_arguments(text, word.end);
            found.push(Citation { span: word.start..end, keys });
        }
        found
    }
}

struct Citation {
    span: Range<usize>,
    keys: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Reading TeX, just enough
// ---------------------------------------------------------------------------------------------

/// A `\name` in the text. The scan is over the raw text — comments and `\verb` included — because
/// a citation in a comment is still a fabricated reference to the person reading the diff, and
/// because "is this a comment?" is a question a determined edit can make the scanner get wrong.
struct ControlWord<'a> {
    name: &'a str,
    /// Byte offset of the backslash.
    start: usize,
    /// Byte offset just past the name.
    end: usize,
}

/// Every control word in `text`, including those inside the arguments of others.
///
/// `\\` (a line break) and any other backslash-plus-symbol are stepped over as a pair, so
/// `a\\cite{x}` — a line break and the word "cite" — is prose and `\\\cite{x}` is a line break
/// and a real command. `@` counts as a letter: `\@cite` is a real command inside a package.
fn control_words(text: &str) -> Vec<ControlWord<'_>> {
    let bytes = text.as_bytes();
    let mut words = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            index += 1;
            continue;
        }
        let name_start = index + 1;
        let mut name_end = name_start;
        while name_end < bytes.len() && (bytes[name_end].is_ascii_alphabetic() || bytes[name_end] == b'@') {
            name_end += 1;
        }
        if name_end == name_start {
            // A control symbol (`\\`, `\%`, `\{`): skip the character it escapes, whole.
            let escaped_len = text[name_start..].chars().next().map_or(0, char::len_utf8);
            index = name_start + escaped_len;
            continue;
        }
        words.push(ControlWord { name: &text[name_start..name_end], start: index, end: name_end });
        index = name_end;
    }
    words
}

/// Read the arguments of the command whose name ends at `from`: returns the keys they name and
/// the offset just past the last argument.
///
/// TeX reads through spaces, newlines and `%` comments between a command and its first argument,
/// so this does too: `\cite%⏎{key}` is `\cite{key}`. Once a braced argument has been read, more
/// are accepted only if they follow it directly (`\cites{a}{b}`), because `\cite{a} {\bf b}` is a
/// citation followed by prose. `[…]` and `(…)` are options (natbib's, and biblatex's
/// `\parencites(pre)(post)`); the braced groups hold comma-separated keys.
fn read_arguments(text: &str, from: usize) -> (Vec<String>, usize) {
    let bytes = text.as_bytes();
    let mut keys = Vec::new();
    let mut index = from;
    if bytes.get(index) == Some(&b'*') {
        index += 1;
    }
    let mut read_a_key_group = false;
    loop {
        let probe = if read_a_key_group { index } else { skip_blanks_and_comments(text, index) };
        match bytes.get(probe) {
            Some(b'[') => index = matching_close(text, probe, b'[', b']').unwrap_or(probe + 1),
            Some(b'(') => index = matching_close(text, probe, b'(', b')').unwrap_or(probe + 1),
            Some(b'{') => {
                // An unclosed group is read to the end of the text: the reading that loses no key.
                let (inner_end, after) = match matching_close(text, probe, b'{', b'}') {
                    Some(after) => (after - 1, after),
                    None => (bytes.len(), bytes.len()),
                };
                keys.extend(keys_of(&text[probe + 1..inner_end]));
                read_a_key_group = true;
                index = after;
            }
            _ => break,
        }
    }
    (keys, index)
}

/// The keys in one braced argument. Comments are removed first, as TeX removes them, so
/// `{a,% why⏎b}` is `a` and `b`; the keys are trimmed of ASCII whitespace only, because a no-break
/// space or a zero-width one is a different character in a key and TeX will not match it.
fn keys_of(argument: &str) -> Vec<String> {
    let mut without_comments = String::with_capacity(argument.len());
    let mut in_comment = false;
    for character in argument.chars() {
        match (in_comment, character) {
            (false, '%') => in_comment = true,
            (true, '\n') => in_comment = false,
            (true, _) => {}
            (false, _) => without_comments.push(character),
        }
    }
    without_comments
        .split(',')
        .map(|key| key.trim_matches(|c: char| c.is_ascii_whitespace()))
        .filter(|key| !key.is_empty() && *key != "*") // `\nocite{*}` cites no one in particular
        .map(str::to_string)
        .collect()
}

fn skip_blanks_and_comments(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = from;
    loop {
        match bytes.get(index) {
            Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0c) => index += 1,
            Some(b'%') => {
                index = text[index..].find('\n').map_or(bytes.len(), |newline| index + newline + 1);
            }
            _ => return index,
        }
    }
}

/// Offset just past the bracket closing the one at `open_at`, counting nesting; an escaped
/// bracket (`\]`) is text. `None` when it never closes.
fn matching_close(text: &str, open_at: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = open_at;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == open => {
                depth += 1;
                index += 1;
            }
            byte if byte == close => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Constructs a prose edit has no business containing
// ---------------------------------------------------------------------------------------------

/// Control words that define, redefine or obscure commands, or pull in text from elsewhere.
const HIDING_COMMANDS: &[&str] = &[
    "def", "gdef", "edef", "xdef", "let", "futurelet", "newcommand", "renewcommand", "providecommand",
    "DeclareRobustCommand", "newrobustcmd", "renewrobustcmd", "providerobustcmd", "NewDocumentCommand",
    "RenewDocumentCommand", "ProvideDocumentCommand", "DeclareDocumentCommand", "NewExpandableDocumentCommand",
    "newenvironment", "renewenvironment", "NewDocumentEnvironment", "RenewDocumentEnvironment", "csname",
    "catcode", "scantokens", "expandafter", "AtBeginDocument", "AtEndDocument", "AtEndPreamble",
    "AfterEndPreamble", "input", "include", "InputIfFileExists", "subfile", "subfileinclude", "import",
    "directlua", "luaexec", "write", "immediate", "openout", "openin", "read", "protected@edef",
];

/// Control words that write a reference list or name a bibliography file.
const REFERENCE_LIST_COMMANDS: &[&str] = &["bibitem", "bibliography", "addbibresource", "addglobalbib", "addsectionbib"];

/// Environments that are, or write, a reference list.
const REFERENCE_LIST_ENVIRONMENTS: &[&str] = &["thebibliography", "filecontents", "filecontents*"];

struct Construct {
    kind: FindingKind,
    /// What is counted: the control word, the environment, or `^^`.
    label: String,
    span: Range<usize>,
    /// How to show it in a sentence.
    shown: String,
}

fn constructs_in(text: &str) -> Vec<Construct> {
    let mut found = Vec::new();
    for word in control_words(text) {
        if HIDING_COMMANDS.contains(&word.name) {
            found.push(Construct {
                kind: FindingKind::HiddenCommand,
                label: word.name.to_string(),
                span: word.start..word.end,
                shown: format!("\\{}", word.name),
            });
        } else if REFERENCE_LIST_COMMANDS.contains(&word.name) {
            found.push(Construct {
                kind: FindingKind::OwnReferenceList,
                label: word.name.to_string(),
                span: word.start..word.end,
                shown: format!("\\{}", word.name),
            });
        } else if word.name == "begin" {
            // `\begin{thebibliography}`: the environment's name is the braced word that follows.
            let open = skip_blanks_and_comments(text, word.end);
            if text.as_bytes().get(open) == Some(&b'{') {
                if let Some(after) = matching_close(text, open, b'{', b'}') {
                    let environment = text[open + 1..after - 1].trim();
                    if REFERENCE_LIST_ENVIRONMENTS.contains(&environment) {
                        found.push(Construct {
                            kind: FindingKind::OwnReferenceList,
                            label: format!("begin:{environment}"),
                            span: word.start..after,
                            shown: format!("\\begin{{{environment}}}"),
                        });
                    }
                }
            }
        }
    }
    // TeX's `^^` notation: `^^5c` is a backslash, so `^^5ccite` is `\cite` to TeX and nothing to
    // a scan for a backslash. There is no reason for it in prose.
    let mut from = 0;
    while let Some(found_at) = text[from..].find("^^") {
        let start = from + found_at;
        found.push(Construct {
            kind: FindingKind::HiddenCommand,
            label: "^^".to_string(),
            span: start..start + 2,
            shown: "TeX\u{2019}s ^^ character notation".to_string(),
        });
        from = start + 2;
    }
    found
}

/// The control words that define a command whose body uses a citation command: the author's
/// `\newcommand{\see}[1]{(see \cite{#1})}` makes `see` one. Pass the result to
/// [`Guard::with_citation_commands`]. Reads the project's preamble (or any text); finds what it
/// can and misses what it cannot, which is why the guard also refuses new definitions.
pub fn citation_macros(text: &str) -> Vec<String> {
    const DEFINERS: &[&str] = &[
        "newcommand", "renewcommand", "providecommand", "DeclareRobustCommand", "newrobustcmd", "renewrobustcmd",
        "providerobustcmd", "NewDocumentCommand", "RenewDocumentCommand", "ProvideDocumentCommand",
        "DeclareDocumentCommand", "def", "gdef", "edef",
    ];
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    for word in control_words(text) {
        if !(DEFINERS.contains(&word.name) || word.name == "xdef") {
            continue;
        }
        // The name: `{\see}`, or `\see` bare (as `\def\see#1{…}` and `\providecommand\see{…}` write it).
        let mut index = skip_blanks_and_comments(text, word.end);
        if bytes.get(index) == Some(&b'*') {
            index = skip_blanks_and_comments(text, index + 1);
        }
        let braced = bytes.get(index) == Some(&b'{');
        if braced {
            index = skip_blanks_and_comments(text, index + 1);
        }
        if bytes.get(index) != Some(&b'\\') {
            continue;
        }
        let name_start = index + 1;
        let mut name_end = name_start;
        while name_end < bytes.len() && (bytes[name_end].is_ascii_alphabetic() || bytes[name_end] == b'@') {
            name_end += 1;
        }
        if name_end == name_start {
            continue;
        }
        let name = &text[name_start..name_end];
        index = name_end;
        if braced {
            index = skip_blanks_and_comments(text, index);
            if bytes.get(index) == Some(&b'}') {
                index += 1;
            }
        }

        // The body is the last braced group in the run of `[…]` and `{…}` groups that follows
        // (`[1]{body}`, `{m}{body}`); for `\def`, the first brace after the parameter text.
        let mut body: Option<Range<usize>> = None;
        if matches!(word.name, "def" | "gdef" | "edef" | "xdef") {
            if let Some(open) = text[index..].find('{').map(|offset| index + offset) {
                if let Some(after) = matching_close(text, open, b'{', b'}') {
                    body = Some(open + 1..after - 1);
                }
            }
        } else {
            loop {
                let probe = skip_blanks_and_comments(text, index);
                match bytes.get(probe) {
                    Some(b'[') => match matching_close(text, probe, b'[', b']') {
                        Some(after) => index = after,
                        None => break,
                    },
                    Some(b'{') => match matching_close(text, probe, b'{', b'}') {
                        Some(after) => {
                            body = Some(probe + 1..after - 1);
                            index = after;
                        }
                        None => break,
                    },
                    _ => break,
                }
            }
        }
        if let Some(body) = body {
            if control_words(&text[body]).iter().any(|inner| texbib::is_citation_command(inner.name)) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

// ---------------------------------------------------------------------------------------------
// References with no \cite at all
// ---------------------------------------------------------------------------------------------

/// Words that start a capital letter and are not a surname, so "(Figure 3, 2019)" and "(in March
/// 2019)" are not references.
const NOT_SURNAMES: &[&str] = &[
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December", "Jan", "Feb", "Mar", "Apr", "Jun", "Jul", "Aug", "Sep", "Sept", "Oct", "Nov", "Dec", "Figure",
    "Figures", "Fig", "Figs", "Table", "Tables", "Section", "Sections", "Chapter", "Chapters", "Appendix", "Equation",
    "Eq", "Eqs", "Theorem", "Lemma", "Proposition", "Corollary", "Definition", "Example", "Algorithm", "Listing",
    "Part", "Volume", "Vol", "Page", "Version",
];

/// How far back from a year to look for the parenthesis it sits in, in characters.
const LOOK_BACK: usize = 80;

/// Plain-text author–year references in `text`, as `(byte range, the text as written)`.
///
/// Two shapes: a year inside a parenthesis or bracket that also holds a capitalised word that is
/// not a month or a "Figure" — `(Smith et al., 2019)`, `(see Müller, 2020)`, `[Lee 2019]` — and
/// "et al." followed by a year — `Smith et al. (2019)`. The narrative `Smith (2019)` is not
/// matched: "Python (2020)" and "World War II (1945)" would be refused for no gain.
fn plain_text_citations(text: &str) -> Vec<(Range<usize>, String)> {
    let bytes = text.as_bytes();
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let Some(year_end) = year_at(text, index) else {
            index += 1;
            continue;
        };
        let year_start = index;
        index = year_end;

        let span = parenthetical_reference(text, year_start, year_end).or_else(|| et_al_reference(text, year_start, year_end));
        if let Some(span) = span {
            // Two shapes can match the same reference ("(Smith et al., 2019)"): keep the first.
            if found.last().map_or(true, |(last, _)| span.start >= last.end) {
                found.push((span.clone(), text[span].to_string()));
            }
        }
    }
    found
}

/// The end of a year (1800–2099, with an optional `a`–`z` suffix) that starts at `at`, if one does.
fn year_at(text: &str, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = bytes.get(at..at + 4)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if at > 0 && (bytes[at - 1].is_ascii_alphanumeric()) {
        return None; // part of a longer number or word
    }
    let year: u32 = text[at..at + 4].parse().ok()?;
    if !(1800..=2099).contains(&year) {
        return None;
    }
    let mut end = at + 4;
    match bytes.get(end) {
        Some(byte) if byte.is_ascii_digit() => return None, // a five-digit number
        Some(byte) if byte.is_ascii_lowercase() => {
            // `2015a` is a year with a suffix; `2015abc` is not a year.
            if bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric) {
                return None;
            }
            end += 1;
        }
        _ => {}
    }
    Some(end)
}

fn parenthetical_reference(text: &str, year_start: usize, year_end: usize) -> Option<Range<usize>> {
    // Walk back to the nearest bracket still open at the year.
    let mut depth = 0usize;
    let mut opener = None;
    for (offset, character) in text[..year_start].char_indices().rev().take(LOOK_BACK) {
        match character {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                if depth == 0 {
                    opener = Some(offset);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let opener = opener?;
    if !has_surname(&text[opener + 1..year_start]) {
        return None;
    }
    // Through the closing bracket if it comes soon, else just the year.
    let rest = &text[year_end..];
    let closes = rest.char_indices().take(60).find(|(_, c)| matches!(c, ')' | ']')).map(|(offset, c)| year_end + offset + c.len_utf8());
    Some(opener..closes.unwrap_or(year_end))
}

/// A capitalised word of two or more letters that is not on the list of things that are not names.
fn has_surname(window: &str) -> bool {
    window
        .split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '\u{2019}' || c == '-'))
        .map(|word| word.trim_matches('-'))
        .any(|word| {
            let mut letters = word.chars();
            letters.next().is_some_and(char::is_uppercase)
                && word.chars().filter(|c| c.is_alphabetic()).count() >= 2
                && !NOT_SURNAMES.contains(&word)
        })
}

fn et_al_reference(text: &str, year_start: usize, year_end: usize) -> Option<Range<usize>> {
    // Before the year, past any `(`, `,` and spaces, the text must end in "et al" or "et al.".
    let before = text[..year_start].trim_end_matches(|c: char| c == '(' || c == ',' || c == '[' || c.is_whitespace());
    let before_et_al = before.strip_suffix("et al.").or_else(|| before.strip_suffix("et al"))?;
    // Start at the surname in front of it, if there is one.
    let name = before_et_al.trim_end();
    let name_start = name
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic() || *c == '\'' || *c == '\u{2019}' || *c == '-')
        .last()
        .map_or(name.len(), |(offset, _)| offset);
    let closes = text[year_end..].strip_prefix([')', ']']).map_or(0, |_| 1);
    Some(name_start..year_end + closes)
}
