//! How many words of *prose* a LaTeX file holds, and which section a given line sits in.
//!
//! This exists because a writer counts words and a line diff counts markup, and the two are not
//! close enough to substitute for one another: adding `\begin{align} … \end{align}` around an
//! equation is four lines and no words, and rewording a paragraph in place is one line and
//! twenty. S10.3c needs the writer's number — for the commit message it suggests and for the
//! delta on every graph row — so it needs its own scan.
//!
//! **What it owns:** one scan that throws away everything a reader would not read aloud, and a
//! list of the headings in a file. Nothing else. It never reads a file (callers pass text), never
//! knows about Git, and never resolves `\input` — the include graph is
//! `abstract-tex-includes`'s, and a caller that wants a whole document's count adds up its files.
//!
//! **What it must never do:** never claim to be exact. TeX's own idea of a word is the output of
//! a typesetting run, and no scanner over source can match it. The contract here is *useful, and
//! never wrong by a lot*: it must not count a bibliography, a table of numbers, a code listing or
//! an equation as prose, and it must not lose a sentence because of a `\textit`. Every rule below
//! is in service of that and no more.
//!
//! ```
//! let text = r"
//! \section{Methods}
//! We sampled $n = 40$ people.  % as pre-registered
//! \cite{smith2019} agrees.
//! ";
//! // Methods, We, sampled, people, agrees: a heading is words the author wrote, while the
//! // maths, the comment and the cite key are not.
//! assert_eq!(texwords::count_prose(text), 5);
//! let headings = texwords::headings(text);
//! assert_eq!(headings[0].title, "Methods");
//! ```

/// Environments whose bodies are not prose at any word count.
///
/// Maths and verbatim, and nothing else. A `tabular` body *is* counted, which is a deliberate
/// choice rather than an omission: a table of prose cells is writing, a table of numbers
/// contributes almost nothing to a word count either way, and guessing which one a table is
/// would be the kind of cleverness this module's contract rules out.
const SKIPPED_ENVIRONMENTS: [&str; 14] = [
    "equation",
    "equation*",
    "align",
    "align*",
    "alignat",
    "alignat*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "displaymath",
    "eqnarray",
    "eqnarray*",
    "verbatim",
];

/// Environments whose body is code: `lstlisting`, `minted`, `Verbatim` and friends, matched by
/// name so that the `[options]` they take do not have to be understood.
const CODE_ENVIRONMENTS: [&str; 5] = ["lstlisting", "minted", "Verbatim", "verbatim*", "filecontents"];

/// Commands whose braces hold a name, a key or a path rather than words, with how many of those
/// groups to throw away.
///
/// `\href{url}{the visible text}` is why this is a count and not a flag: its first group is a
/// URL and its second is prose the reader reads. Anything not listed keeps its groups, so
/// `\textit{a lovely phrase}` still counts three words — losing those would be far worse than
/// counting a stray `\label`.
const NON_PROSE_ARGUMENTS: [(&str, usize); 26] = [
    ("label", 1),
    ("ref", 1),
    ("eqref", 1),
    ("pageref", 1),
    ("autoref", 1),
    ("cref", 1),
    ("Cref", 1),
    ("cite", 1),
    ("citep", 1),
    ("citet", 1),
    ("citeauthor", 1),
    ("citeyear", 1),
    ("autocite", 1),
    ("textcite", 1),
    ("parencite", 1),
    ("nocite", 1),
    ("input", 1),
    ("include", 1),
    ("subfile", 1),
    ("includegraphics", 1),
    ("usepackage", 1),
    ("documentclass", 1),
    ("bibliography", 1),
    ("bibliographystyle", 1),
    ("url", 1),
    ("href", 1), // the *first* group only: the second is the link's visible text
];

/// One heading, as the file has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1-based, the way every other line number in this project is counted.
    pub line: usize,
    /// 0 for `\part`, rising to 5 for `\paragraph` — the same scale `src/lib/outline.ts` uses,
    /// so the two never have to be reconciled.
    pub level: usize,
    /// The text between the braces, exactly as the author wrote it.
    pub title: String,
}

/// The headings in one file, in the order they appear.
///
/// Titles only, and no numbers: see [`section_of`]'s note. A `\section*{…}` is included, because
/// a writer who reworded an unnumbered section still reworded a section.
pub fn headings(text: &str) -> Vec<Heading> {
    let mut found = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'\\' {
                at += 1;
                continue;
            }
            let (name, after_name) = control_word(line, at + 1);
            let Some(level) = section_level(name) else {
                at = after_name.max(at + 1);
                continue;
            };
            // `\section*{…}` and `\section[short]{long}` both reach the braces a step later.
            let mut cursor = after_name;
            if bytes.get(cursor) == Some(&b'*') {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'[') {
                cursor = skip_group(bytes, cursor, b'[', b']');
            }
            if bytes.get(cursor) == Some(&b'{') {
                let end = skip_group(bytes, cursor, b'{', b'}');
                // `end` is one past the `}` when it was balanced, and the length when it was not.
                let inner_end = if end > cursor + 1 { end - 1 } else { cursor + 1 };
                let title = line[cursor + 1..inner_end.min(line.len())].trim();
                found.push(Heading {
                    line: index + 1,
                    level,
                    title: title.to_string(),
                });
                cursor = end;
            }
            at = cursor.max(at + 1);
        }
    }
    found
}

/// The heading a line belongs to: the last one at or above it.
///
/// Deliberately the *title* and not "§3.2 Methods". Numbering a section correctly needs the whole
/// document walked in `\input` order, `\appendix` understood and `\section*` left out of the
/// count — and a wrong number in a commit message is a lie that outlives the commit, whereas a
/// name is true wherever the section moves to. If numbering is ever wanted it belongs on top of
/// [`headings`] and `abstract-tex-includes`, not inside this scan.
pub fn section_of(headings: &[Heading], line: usize) -> Option<&Heading> {
    headings.iter().rfind(|heading| heading.line <= line)
}

/// How many words of prose `text` holds.
///
/// One pass, no allocation: the scan hands each character it keeps to a counter rather than
/// building the stripped text first. That matters because this runs once per changed file per
/// row of the history — see the measurement in `abstract-tex-git`'s
/// `tests/against_real_git.rs`, which is what made the allocating version worth replacing.
pub fn count_prose(text: &str) -> usize {
    let mut counter = WordCounter::default();
    scan(text, &mut counter);
    counter.finish()
}

/// The prose with the markup taken out: what [`count_prose`] counts, exposed because it is the
/// only way to see *why* a count came out as it did when one looks wrong.
pub fn prose_of(text: &str) -> String {
    let mut kept = Kept(String::with_capacity(text.len()));
    scan(text, &mut kept);
    kept.0
}

/// Where the scan sends the characters it decides are prose.
///
/// A trait with two implementations rather than one function returning a `String`, because the
/// counting caller does not want the string and allocating one per file per commit was the whole
/// of the cost the measurement found.
trait Sink {
    fn keep(&mut self, character: char);
    /// A word boundary with no character of its own — what markup leaves behind.
    fn separate(&mut self);
}

/// Counts words: any run of non-whitespace holding at least one alphanumeric character.
#[derive(Default)]
struct WordCounter {
    words: usize,
    in_word: bool,
    word_has_letter: bool,
}

impl WordCounter {
    fn end_word(&mut self) {
        if self.in_word && self.word_has_letter {
            self.words += 1;
        }
        self.in_word = false;
        self.word_has_letter = false;
    }

    fn finish(mut self) -> usize {
        self.end_word();
        self.words
    }
}

impl Sink for WordCounter {
    fn keep(&mut self, character: char) {
        if character.is_whitespace() {
            self.end_word();
            return;
        }
        self.in_word = true;
        self.word_has_letter |= character.is_alphanumeric();
    }

    fn separate(&mut self) {
        self.end_word();
    }
}

/// Collects the prose itself, for [`prose_of`].
struct Kept(String);

impl Sink for Kept {
    fn keep(&mut self, character: char) {
        self.0.push(character);
    }

    fn separate(&mut self) {
        self.0.push(' ');
    }
}

/// The one scan both public functions are built on.
///
/// It walks *bytes*, not chars, and that is safe rather than clever: every character the grammar
/// cares about — `\`, `%`, `$`, braces, brackets, letters — is ASCII, and in UTF-8 no byte of a
/// multi-byte character can be mistaken for an ASCII one. A non-ASCII byte is decoded once, where
/// it is met, so that `café` is one word and a stray `…` is none.
fn scan(text: &str, sink: &mut impl Sink) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            // An escaped percent never arrives here: `\%` is consumed by the backslash arm.
            b'%' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'$' => {
                let fence = if bytes.get(at + 1) == Some(&b'$') { 2 } else { 1 };
                at = skip_until_dollar(bytes, at + fence, fence);
                sink.separate(); // maths sits between words; it must not glue them together
            }
            b'\\' => at = command(text, at, sink),
            byte if byte < 0x80 => {
                sink.keep(byte as char);
                at += 1;
            }
            _ => {
                // The start of a multi-byte character. `chars().next()` cannot fail here: `at` is
                // a character boundary, because every arm above advances by whole characters.
                let character = text[at..].chars().next().unwrap_or(' ');
                sink.keep(character);
                at += character.len_utf8();
            }
        }
    }
}

/// Handle one `\…` at `at`, keeping whatever of it is prose, and answer where to carry on.
fn command(text: &str, at: usize, sink: &mut impl Sink) -> usize {
    let bytes = text.as_bytes();
    let (name, after_name) = control_word(text, at + 1);
    if name.is_empty() {
        // A control *symbol*: `\%`, `\&`, `\_`, `\\`, `\,`. The escaped character belongs to the
        // word around it (`50\%` is one word), except a line break, which separates words.
        match bytes.get(at + 1) {
            Some(b'\\') => sink.separate(),
            Some(&symbol) if symbol < 0x80 => sink.keep(symbol as char),
            _ => sink.separate(),
        }
        return at + 2;
    }
    match name {
        // `\(…\)` and `\[…\]`: inline and display maths in their modern spelling.
        "(" | "[" => {
            sink.separate();
            return skip_math_delimiter(bytes, after_name, name);
        }
        "begin" => return begin_environment(text, after_name, sink),
        // The name of the environment being closed is not prose either.
        "end" => {
            let (_, after) = braced_name(text, after_name);
            sink.separate();
            return after;
        }
        _ => {}
    }
    if let Some((_, groups)) = NON_PROSE_ARGUMENTS.iter().find(|(command, _)| *command == name) {
        let mut cursor = after_name;
        // A `[…]` between the command and its braces is options, never prose.
        while bytes.get(cursor) == Some(&b'[') {
            cursor = skip_group(bytes, cursor, b'[', b']');
        }
        for _ in 0..*groups {
            if bytes.get(cursor) == Some(&b'{') {
                cursor = skip_group(bytes, cursor, b'{', b'}');
            }
        }
        sink.separate();
        return cursor;
    }
    // Any other command: the name itself is not a word, but its braces usually hold prose, so
    // they are simply left in place. Nothing is kept and nothing separates, so
    // `\textit{well}-known` stays one word.
    after_name
}

/// `\(` and `\[` skip to their closing partner; a stray one runs to the end, which is what TeX
/// would also do with it.
fn skip_math_delimiter(bytes: &[u8], from: usize, opening: &str) -> usize {
    let closing = if opening == "(" { b')' } else { b']' };
    let mut at = from;
    while at + 1 < bytes.len() {
        if bytes[at] == b'\\' && bytes[at + 1] == closing {
            return at + 2;
        }
        at += 1;
    }
    bytes.len()
}

/// `\begin{…}`: either skip the whole body, or carry on and count it.
fn begin_environment(text: &str, after_begin: usize, sink: &mut impl Sink) -> usize {
    let (name, after_name) = braced_name(text, after_begin);
    sink.separate();
    let skipped = SKIPPED_ENVIRONMENTS.contains(&name) || CODE_ENVIRONMENTS.contains(&name);
    if !skipped {
        return after_name;
    }
    // `\end{name}` ends it. Nested same-name environments do not happen in practice for any of
    // these, and pretending to handle them would be untested code.
    let closing = format!("\\end{{{name}}}");
    match text[after_name..].find(&closing) {
        Some(offset) => after_name + offset + closing.len(),
        None => text.len(),
    }
}

/// The `{name}` right after `\begin`/`\end`, and where it ends. Tolerates `\begin {name}`.
fn braced_name(text: &str, from: usize) -> (&str, usize) {
    let bytes = text.as_bytes();
    let mut at = from;
    while bytes.get(at).is_some_and(|byte| byte.is_ascii_whitespace()) {
        at += 1;
    }
    if bytes.get(at) != Some(&b'{') {
        return ("", at);
    }
    let end = skip_group(bytes, at, b'{', b'}');
    let inner_end = if end > at + 1 { end - 1 } else { at + 1 };
    (text[at + 1..inner_end.min(text.len())].trim(), end)
}

/// The control word starting at `from` (the byte after the backslash), and where it ends.
///
/// A TeX control word is the whole run of letters, which is the rule that stops `\partial` being
/// read as `\part` — the same trap `src/lib/outline.ts` documents on its regex. A control
/// *symbol* is one non-letter character and comes back as an empty name.
fn control_word(text: &str, from: usize) -> (&str, usize) {
    let bytes = text.as_bytes();
    let mut at = from;
    while bytes.get(at).is_some_and(|byte| byte.is_ascii_alphabetic()) {
        at += 1;
    }
    if at == from {
        // Not a letter: `(` and `[` are named here because they open maths and the caller
        // switches on them; every other symbol comes back empty.
        return match bytes.get(from) {
            Some(b'(') => ("(", from + 1),
            Some(b'[') => ("[", from + 1),
            _ => ("", from),
        };
    }
    (&text[from..at], at)
}

/// Past a balanced group that starts at `from` (which must hold `open`). Answers one past the
/// closing byte, or the length for an unbalanced group.
fn skip_group(bytes: &[u8], from: usize, open: u8, close: u8) -> usize {
    let mut depth = 0usize;
    let mut at = from;
    while at < bytes.len() {
        // An escaped brace is a character, not a delimiter.
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at] == open {
            depth += 1;
        } else if bytes[at] == close {
            depth -= 1;
            if depth == 0 {
                return at + 1;
            }
        }
        at += 1;
    }
    bytes.len()
}

/// Past the closing `$` or `$$` of a maths run that started before `from`.
fn skip_until_dollar(bytes: &[u8], from: usize, fence: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at] == b'$' {
            return at + fence;
        }
        at += 1;
    }
    bytes.len()
}

/// The `%` that starts a real comment: one preceded by an even number of backslashes, since each
/// backslash escapes the next. `src/lib/outline.ts` reasons this out the same way on the other
/// side of the IPC boundary.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (position, byte) in bytes.iter().enumerate() {
        if *byte != b'%' {
            continue;
        }
        let backslashes = bytes[..position]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if backslashes % 2 == 0 {
            return &line[..position];
        }
    }
    line
}

fn section_level(name: &str) -> Option<usize> {
    match name {
        "part" => Some(0),
        "chapter" => Some(1),
        "section" => Some(2),
        "subsection" => Some(3),
        "subsubsection" => Some(4),
        "paragraph" => Some(5),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prose_counts_its_words() {
        assert_eq!(count_prose("The quick brown fox jumps."), 5);
        assert_eq!(count_prose(""), 0);
        assert_eq!(count_prose("   \n\n  "), 0);
    }

    #[test]
    fn markup_is_not_words_but_what_it_wraps_is() {
        // The command names vanish; `\textit{a lovely phrase}` keeps its three words.
        assert_eq!(count_prose(r"\textit{a lovely phrase} of \emph{prose}"), 5);
        // And the wrapped word does not come apart from what it is joined to.
        assert_eq!(count_prose(r"\textit{well}-known"), 1);
    }

    #[test]
    fn maths_is_never_prose() {
        assert_eq!(count_prose(r"We sampled $n = 40$ people."), 3); // We, sampled, people
        assert_eq!(count_prose(r"so \( x^2 + y^2 \) holds"), 2);
        assert_eq!(count_prose(r"and \[ \int_0^1 f \] again"), 2);
        assert_eq!(count_prose("display $$a+b$$ maths"), 2);
        // Wrapping an existing equation in an environment is four lines of diff and no words.
        assert_eq!(
            count_prose("one\n\\begin{align}\na &= b \\\\ c &= d\n\\end{align}\ntwo"),
            2
        );
    }

    #[test]
    fn a_comment_is_a_note_to_self_and_not_part_of_the_paper() {
        assert_eq!(count_prose("two words % and four more words here"), 2);
        // An escaped percent is prose, and stays attached to the number it belongs to.
        assert_eq!(count_prose(r"about 50\% of them"), 4);
    }

    #[test]
    fn keys_paths_and_citations_are_not_prose() {
        assert_eq!(
            count_prose(r"see \cite{smith2019, jones2020} and \ref{fig:one}"),
            2
        ); // see, and
        assert_eq!(
            count_prose(r"\includegraphics[width=0.8\textwidth]{figures/plot-one.pdf}"),
            0
        );
        assert_eq!(count_prose(r"\label{sec:methods}\input{sections/methods}"), 0);
        // A link's visible text is read aloud; its URL is not.
        assert_eq!(count_prose(r"\href{https://example.org/a/b}{the data set}"), 3);
    }

    #[test]
    fn a_code_listing_is_not_prose_however_many_words_it_contains() {
        let text = "before\n\\begin{lstlisting}\nfor word in words: print(word, word, word)\n\\end{lstlisting}\nafter";
        assert_eq!(count_prose(text), 2);
    }

    #[test]
    fn a_verbatim_percent_is_not_a_comment_and_a_verbatim_dollar_is_not_maths() {
        // The whole body is skipped, so neither can be mistaken for markup outside it.
        assert_eq!(
            count_prose("a\n\\begin{verbatim}\n% $ not maths $\n\\end{verbatim}\nb"),
            2
        );
    }

    #[test]
    fn an_unclosed_environment_or_group_ends_the_scan_rather_than_hanging() {
        assert_eq!(
            count_prose("words\n\\begin{verbatim}\nand then nothing closed it"),
            1
        );
        assert_eq!(count_prose(r"\cite{unclosed and the rest"), 0);
        // The prose before the unclosed maths still counts; only the maths itself is lost.
        assert_eq!(count_prose("maths $ that never closes"), 1);
    }

    #[test]
    fn headings_are_found_with_their_level_and_line() {
        let text = "\\section{Methods}\nwords\n\\subsection{Sampling}\nmore\n\\section*{Unnumbered}\n";
        assert_eq!(
            headings(text),
            vec![
                Heading {
                    line: 1,
                    level: 2,
                    title: "Methods".into()
                },
                Heading {
                    line: 3,
                    level: 3,
                    title: "Sampling".into()
                },
                Heading {
                    line: 5,
                    level: 2,
                    title: "Unnumbered".into()
                },
            ]
        );
    }

    #[test]
    fn a_heading_with_a_short_title_or_nested_braces_keeps_the_long_one() {
        let found = headings(r"\section[Short]{The \emph{long} title}");
        assert_eq!(found[0].title, r"The \emph{long} title");
    }

    /// The trap `outline.ts` documents: a control word is the whole run of letters.
    #[test]
    fn partial_is_not_part() {
        assert!(headings(r"\partial x").is_empty());
        assert!(headings(r"\paragraphindent{2em}").is_empty());
    }

    #[test]
    fn a_commented_out_heading_is_not_a_heading() {
        assert!(headings(r"% \section{Dropped for now}").is_empty());
    }

    #[test]
    fn a_line_belongs_to_the_last_heading_at_or_above_it() {
        let found = headings("\\section{One}\na\n\\section{Two}\nb\n");
        assert_eq!(section_of(&found, 1).map(|h| h.title.as_str()), Some("One"));
        assert_eq!(section_of(&found, 2).map(|h| h.title.as_str()), Some("One"));
        assert_eq!(section_of(&found, 3).map(|h| h.title.as_str()), Some("Two"));
        assert_eq!(section_of(&found, 99).map(|h| h.title.as_str()), Some("Two"));
        // Text before the first heading belongs to no section, which is a real state: a preamble.
        assert_eq!(section_of(&found, 0), None);
    }

    /// The claim in the module doc, on something shaped like a real paragraph.
    #[test]
    fn a_realistic_paragraph_is_counted_within_a_word_or_two_of_by_hand() {
        let text = r"
\subsection{Results}
We found that the effect held for all $n = 40$ participants
\cite{smith2019}, as \autoref{fig:main} shows.  % check the CI
";
        // By hand, reading it aloud: We(1) found(2) that(3) the(4) effect(5) held(6) for(7)
        // all(8) participants(9) as(10) shows(11). "Results" is a heading, not a sentence, but it
        // is words the author wrote, so 12.
        assert_eq!(count_prose(text), 12);
    }
}
