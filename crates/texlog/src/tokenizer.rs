//! S5.1: the layer beneath the paren-stack resolver (S5.2). This module turns a raw TeX log
//! into a flat, ordered stream of classified *logical* lines, with the 79-column wrapping the
//! transcript writer does undone first — DESIGN.md §5.2 names both of these ("messages wrap at
//! 79 columns mid-word") as the reason resolving a message to a real `file:line` is hard.
//!
//! What this module deliberately does not do: decide which `(`/`)` pair belongs to which file,
//! or which is a real file boundary at all rather than a stray character inside a warning's own
//! prose (`(Unicode)`, `(HO)`). It only reports where the candidate characters are and, for a
//! `(`, what token immediately follows it — the raw material S5.2's stack-tracking resolver
//! needs, not the resolved answer. Splitting it this way means this module's own correctness
//! (unwrapping, classifying) can be tested without also having to get the much harder
//! disambiguation problem right in the same pass.
//!
//! Text in, data out, same as the rest of this crate (`lib.rs`'s module doc explains why: this
//! crate is extracted as a standalone MIT crate at S6.5, so it must never read a file or know
//! about the editor).

/// How a logical line of the log begins. Every one of TeX's own structural markers — an error,
/// a source-context line, a warning banner — is something TeX prints at the *start* of a line;
/// this is what every fixture this crate has captured from a real engine shows, and is the same
/// assumption `lib.rs`'s existing `quick_errors` already relies on for `!` and `l.NN`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// `! <message>` — the message with the leading `! ` (and, for Tectonic's doubled-bang
    /// package errors, only the *first* one) stripped, matching `quick_errors`'s own behaviour.
    Error(String),
    /// `l.NN <context>` — TeX's own line-number claim, frequently approximate, and the source
    /// excerpt printed after it.
    LineMarker { line: u32, context: String },
    /// A warning banner: `LaTeX Warning:`, `LaTeX Font Warning:`, `Package <name> Warning:`,
    /// `Class <name> Warning:`, and any workalike where `Warning: ` appears close enough to the
    /// start of the line to be the "who is warning" prefix rather than a coincidence deep inside
    /// unrelated prose. `message` is the text after `Warning: `.
    Warning { message: String },
    /// Neither of the above — the bulk of a log: package banners, font tables, page numbers,
    /// prose warnings' second line, and everything else. Its own `(`/`)` occurrences (if any)
    /// are still reported in `LogLine::parens`, since a file can open on a line that is *also* a
    /// warning or an error's own text (`Package hyperref Warning: ... (hyperref)`).
    Text,
}

/// Where a `(` or `)` sits in a [`LogLine`], and, for a `(`, the token immediately following it
/// — the run of non-whitespace characters up to the next paren or the end of the line. This is
/// what a real file-opening line looks like (`(article.cls`, `(/abs/path/to/chapter.tex`); it is
/// also what an incidental parenthetical looks like (`(Unicode)`, `(HO)`), which is exactly why
/// this crate does not try to tell the two apart here — that is S5.2's job, with the fixtures
/// for the pathological cases this loop's card asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParenEvent {
    Open {
        /// Byte offset of the `(` within the logical line's own text.
        at: usize,
        /// The non-whitespace run right after the `(`, empty if the line ends (or whitespace
        /// starts) immediately after it.
        candidate: String,
    },
    Close {
        /// Byte offset of the `)` within the logical line's own text.
        at: usize,
    },
}

/// One logical line: already unwrapped, classified, and with its own paren events extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub kind: LineKind,
    /// The full unwrapped text, kept for the "raw log" affordance and so a caller with its own
    /// classification needs can still get at it.
    pub text: String,
    /// In left-to-right order. Empty for the overwhelming majority of lines.
    pub parens: Vec<ParenEvent>,
}

/// TeX's transcript writer hard-wraps any line at this many columns, with no continuation
/// marker — the next physical line just *is* the rest of the logical one, even mid-word. Every
/// engine this project targets (Tectonic's bundled XeTeX, and the pdfTeX/LuaTeX this project may
/// support later) uses this same default; DESIGN.md §5.2 names the behaviour without pinning the
/// number, and 79 is what `crates/texlog/fixtures/wrapped-file-open/main.log` (a real capture)
/// confirms in this project's own bundled engine.
const WRAP_COLUMN: usize = 79;

/// Undo the 79-column wrap: a *physical* line whose own length is exactly [`WRAP_COLUMN`]
/// characters continues, with no separator, on the next physical line — and if that next
/// physical line is *itself* also exactly `WRAP_COLUMN` characters long, the chain continues
/// again, and so on. This is checked against each raw physical line's own length, not the
/// cumulative length of the logical line built up so far: TeX's transcript writer counts columns
/// from zero on every physical line it emits, so a message long enough to wrap twice produces
/// two consecutive 79-character physical lines before the shorter final remainder, and a rule
/// based on the running total would stop joining after the first one. Length is counted in
/// `char`s, not bytes, so a log with non-ASCII author names (a `\title` in the transcript, say)
/// still lines up with what a person looking at a terminal actually sees.
///
/// This is a real heuristic with a real failure mode, stated rather than hidden: a line whose
/// *unwrapped* content genuinely happens to be exactly 79 characters looks identical to a
/// wrapped one and gets merged with whatever follows. Observed in practice to be rare enough not
/// to matter — real multi-line package banners this project's fixtures have captured are not
/// coincidentally 79 characters wide (see `unwrap_lines`'s own test for one, at 73 and 27
/// characters, correctly left unmerged) — and a resolver built on top of this can always fall
/// back to the raw log when a rule fails to make sense of what it sees.
pub fn unwrap_lines(log: &str) -> Vec<String> {
    let mut logical: Vec<String> = Vec::new();
    let mut previous_raw_len = 0usize;
    for raw in log.lines() {
        if previous_raw_len == WRAP_COLUMN {
            if let Some(current) = logical.last_mut() {
                current.push_str(raw);
            }
        } else {
            logical.push(raw.to_string());
        }
        previous_raw_len = raw.chars().count();
    }
    logical
}

/// `l.87 The sample size n_` → `Some((87, " The sample size n_"))`. Mirrors `lib.rs`'s
/// `parse_line_marker`, kept as its own copy rather than shared: that function is `quick_errors`'
/// own implementation detail today, and duplicating four lines here is cheaper than making the
/// two crates' modules depend on each other's internals for something this small.
fn parse_line_marker(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix("l.")?;
    let digits_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    if digits_end == 0 {
        return None;
    }
    let num: u32 = rest[..digits_end].parse().ok()?;
    Some((num, &rest[digits_end..]))
}

/// `Warning: ` must appear, and appear close to the start of the line — the "who is warning"
/// prefix (`LaTeX`, `LaTeX Font`, `Package hyperref`, `Class report`, …) is always short. Capped
/// rather than searched for anywhere in the line, so a warning's own message text mentioning the
/// word "Warning" deep in a sentence is not mistaken for a second banner.
fn classify_warning(line: &str) -> Option<String> {
    const MARKER: &str = "Warning: ";
    let idx = line.find(MARKER)?;
    if idx > 40 {
        return None;
    }
    Some(line[idx + MARKER.len()..].trim().to_string())
}

/// The non-whitespace run starting at `start` in `line`, stopping at the next paren too — a
/// filename never legitimately contains an unescaped `(` or `)`, and stopping there means a
/// same-line `(a.sty)(b.sty)` reports two candidates instead of one run swallowing both.
fn candidate_after(line: &str, start: usize) -> &str {
    let rest = &line[start..];
    let end = rest.find(|c: char| c.is_whitespace() || c == '(' || c == ')').unwrap_or(rest.len());
    &rest[..end]
}

/// Every `(`/`)` in `line`, in order, with a `(`'s immediately-following candidate token.
fn paren_events(line: &str) -> Vec<ParenEvent> {
    let mut events = Vec::new();
    for (at, ch) in line.char_indices() {
        match ch {
            '(' => events.push(ParenEvent::Open { at, candidate: candidate_after(line, at + 1).to_string() }),
            ')' => events.push(ParenEvent::Close { at }),
            _ => {}
        }
    }
    events
}

fn classify(line: &str) -> LineKind {
    if let Some(message) = line.strip_prefix("! ") {
        return LineKind::Error(message.trim_end().to_string());
    }
    if let Some((line_no, context)) = parse_line_marker(line) {
        return LineKind::LineMarker { line: line_no, context: context.to_string() };
    }
    if let Some(message) = classify_warning(line) {
        return LineKind::Warning { message };
    }
    LineKind::Text
}

/// Unwrap and classify a whole log. The only entry point a caller outside this module needs.
pub fn tokenize(log: &str) -> Vec<LogLine> {
    unwrap_lines(log)
        .into_iter()
        .map(|text| LogLine { kind: classify(&text), parens: paren_events(&text), text })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod unwrap_lines_tests {
        use super::*;

        #[test]
        fn leaves_ordinary_lines_alone() {
            let log = "line one\nline two\nline three\n";
            assert_eq!(unwrap_lines(log), vec!["line one", "line two", "line three"]);
        }

        #[test]
        fn joins_a_line_of_exactly_79_characters_with_the_next() {
            let first = "a".repeat(79);
            let log = format!("{first}\nrest of the message\nunrelated next line\n");
            let joined = unwrap_lines(&log);
            assert_eq!(joined, vec![format!("{first}rest of the message"), "unrelated next line".to_string()]);
        }

        #[test]
        fn keeps_joining_across_a_message_long_enough_to_wrap_twice() {
            // Two consecutive 79-character physical lines, the shape a message wraps into when
            // it is itself longer than 158 characters: TeX counts columns from zero on every
            // physical line, not against the logical line's running total, so both must trigger
            // a join even though the *combined* length is never 79 again after the first one.
            let log = format!("{}\n{}\n{}\nunrelated\n", "a".repeat(79), "b".repeat(79), "ccccc");
            let joined = unwrap_lines(&log);
            assert_eq!(joined, vec![format!("{}{}ccccc", "a".repeat(79), "b".repeat(79)), "unrelated".to_string()]);
        }

        #[test]
        fn does_not_join_an_intentional_two_line_banner_that_is_not_79_characters() {
            // The real shape captured in atbegshi-ltx's own banner (wrapped-file-open/main.log):
            // 73 characters, then 27 — a package author's own two-line message, not a wrap.
            let log = format!("{}\n{}\n", "x".repeat(73), "y".repeat(27));
            let joined = unwrap_lines(&log);
            assert_eq!(joined, vec!["x".repeat(73), "y".repeat(27)]);
        }

        #[test]
        fn counts_characters_not_bytes_so_multibyte_text_still_wraps_correctly() {
            // 79 non-ASCII characters, each more than one UTF-8 byte — a byte-counting version of
            // this function would never fire the wrap at all for a line like this.
            let first: String = "é".repeat(79);
            let log = format!("{first}\ntail\n");
            assert_eq!(unwrap_lines(&log), vec![format!("{first}tail")]);
        }

        #[test]
        fn unwraps_the_real_captured_file_open_line() {
            let log = include_str!("../fixtures/wrapped-file-open/main.log");
            let joined = unwrap_lines(log);
            assert!(
                joined.iter().any(|line| line
                    == "(subdirectory-with-a-genuinely-long-name-to-force-wrapping-of-the-open-paren-line/chapter"),
                "the wrapped path should read as one unbroken logical line: {joined:#?}"
            );
        }
    }

    mod classify_tests {
        use super::*;

        #[test]
        fn recognises_an_error() {
            assert_eq!(classify("! Undefined control sequence."), LineKind::Error("Undefined control sequence.".to_string()));
        }

        #[test]
        fn keeps_a_doubled_bang_in_the_message_like_quick_errors_does() {
            // Tectonic's own package-not-found wording (S2.6's fixture): only the first "! " is
            // structural, the second is part of the message text.
            assert_eq!(
                classify("! ! LaTeX Error: File `x.sty' not found.."),
                LineKind::Error("! LaTeX Error: File `x.sty' not found..".to_string())
            );
        }

        #[test]
        fn recognises_a_line_marker() {
            assert_eq!(
                classify("l.87 The sample size n_"),
                LineKind::LineMarker { line: 87, context: " The sample size n_".to_string() }
            );
        }

        #[test]
        fn recognises_the_three_shapes_of_warning_banner() {
            for (line, who) in [
                ("LaTeX Warning: Citation `x' undefined.", "Citation `x' undefined."),
                ("LaTeX Font Warning: Font shape undefined.", "Font shape undefined."),
                ("Package hyperref Warning: Rerun to get /PageLabels entry.", "Rerun to get /PageLabels entry."),
            ] {
                assert_eq!(classify(line), LineKind::Warning { message: who.to_string() }, "{line}");
            }
        }

        #[test]
        fn does_not_mistake_the_word_warning_deep_in_a_sentence_for_a_banner() {
            let line = "This document contains a well-known warning: results may vary widely.";
            assert_eq!(classify(line), LineKind::Text);
        }

        #[test]
        fn plain_transcript_output_is_text() {
            assert_eq!(classify("\\c@part=\\count181"), LineKind::Text);
        }
    }

    mod paren_events_tests {
        use super::*;

        #[test]
        fn finds_a_simple_file_open_and_its_candidate_path() {
            let events = paren_events("(article.cls");
            assert_eq!(events, vec![ParenEvent::Open { at: 0, candidate: "article.cls".to_string() }]);
        }

        #[test]
        fn finds_a_bare_close() {
            assert_eq!(paren_events(")"), vec![ParenEvent::Close { at: 0 }]);
        }

        #[test]
        fn two_files_opened_and_closed_on_one_line_do_not_swallow_each_other() {
            // A real shape from wrapped-file-open/main.log: "(hycolor.sty) (letltxmacro.sty)".
            let events = paren_events("(hycolor.sty) (letltxmacro.sty)");
            assert_eq!(
                events,
                vec![
                    ParenEvent::Open { at: 0, candidate: "hycolor.sty".to_string() },
                    ParenEvent::Close { at: 12 },
                    ParenEvent::Open { at: 14, candidate: "letltxmacro.sty".to_string() },
                    ParenEvent::Close { at: 30 },
                ]
            );
        }

        #[test]
        fn an_incidental_parenthetical_is_reported_the_same_way_as_a_real_file_open() {
            // This is the whole reason disambiguation is a separate loop (S5.2): at this layer,
            // "(Unicode)" and "(article.cls" look identical in shape.
            let line = "Token not allowed in a PDF string (Unicode): removing.";
            let events = paren_events(line);
            assert_eq!(
                events,
                vec![
                    ParenEvent::Open { at: 34, candidate: "Unicode".to_string() },
                    ParenEvent::Close { at: 42 },
                ]
            );
        }

        #[test]
        fn a_candidate_stops_at_whitespace_not_just_at_the_next_paren() {
            let events = paren_events("(main.tex and some trailing prose");
            assert_eq!(events, vec![ParenEvent::Open { at: 0, candidate: "main.tex".to_string() }]);
        }

        #[test]
        fn a_line_with_no_parens_reports_nothing() {
            assert_eq!(paren_events("No file main.aux."), vec![]);
        }
    }

    mod tokenize_tests {
        use super::*;

        #[test]
        fn walks_a_whole_log_in_order() {
            let log = "! Undefined control sequence.\nl.12 \\textbold\n(main.aux)\n";
            let lines = tokenize(log);
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[0].kind, LineKind::Error("Undefined control sequence.".to_string()));
            assert_eq!(lines[1].kind, LineKind::LineMarker { line: 12, context: " \\textbold".to_string() });
            assert_eq!(lines[2].kind, LineKind::Text);
            assert_eq!(
                lines[2].parens,
                vec![ParenEvent::Open { at: 0, candidate: "main.aux".to_string() }, ParenEvent::Close { at: 9 }]
            );
        }

        #[test]
        fn a_wrapped_file_path_reaches_the_open_event_intact() {
            let log = include_str!("../fixtures/wrapped-file-open/main.log");
            let lines = tokenize(log);
            let opened_the_long_path = lines.iter().any(|l| {
                l.parens.iter().any(|p| match p {
                    ParenEvent::Open { candidate, .. } => candidate.contains("open-paren-line/chapter"),
                    ParenEvent::Close { .. } => false,
                })
            });
            assert!(opened_the_long_path, "the wrapped path should survive as one candidate: {lines:#?}");
        }

        #[test]
        fn the_real_fixture_undefined_control_sequence_still_classifies_correctly() {
            // Cross-check against a fixture `rules.rs` already trusts, so this module's own
            // classification is judged against the same real bytes the rule catalog is.
            let log = include_str!("../fixtures/undefined-control-sequence/main.log");
            let lines = tokenize(log);
            assert!(matches!(&lines.iter().find(|l| matches!(l.kind, LineKind::Error(_))).unwrap().kind,
                LineKind::Error(m) if m == "Undefined control sequence."));
            assert!(lines.iter().any(|l| matches!(l.kind, LineKind::LineMarker { line: 4, .. })));
        }
    }
}
