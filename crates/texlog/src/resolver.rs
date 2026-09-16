//! S5.2: the paren-stack resolver DESIGN.md §5.2 names as "the whole engineering problem" —
//! deciding, at any point in a log, which file TeX had open, by walking `tokenizer.rs`'s (S5.1)
//! classified lines and treating `(`/`)` as a stack push/pop.
//!
//! The hard part was never the stack machinery — it is deciding whether a given `(` opens a real
//! file at all, and three real captures (see `crates/texlog/fixtures/README.md`) overturned every
//! hand-guessed rule tried here before landing on the one below:
//!
//! - **Position does not work.** A real file's `(` is not reliably at the start of a line or
//!   right after another paren: `fixtures/space-in-path/main.log` opens `article.cls` mid-line,
//!   straight after a version string (`L3 programming layer <2022-02-24> (article.cls`). And an
//!   incidental parenthetical *can* sit at the very start of a line: the same log's
//!   `(rerunfilecheck)             Checksum: ...` is a package echoing its own name, not a file
//!   boundary, despite opening at column 0.
//! - **A file extension is not reliably present either.** `\input{name}` with no extension given
//!   at the call site is echoed by this project's bundled Tectonic with no extension at all —
//!   `fixtures/bare-input-no-extension/main.log` shows `\input{plainchapter}` opening as
//!   `(plainchapter)`, lexically identical in shape to an incidental `(rerunfilecheck)`. There is
//!   no text-only rule that tells these two apart; the only way to be sure would be to check
//!   whether `plainchapter` is a real path in the project, which this crate is chartered
//!   (`lib.rs`) to never do — it never reads a file. This is accepted as a real, irreducible
//!   limitation rather than hidden: [`looks_like_a_file`] returns `false` for both, so a
//!   diagnostic inside a bare extensionless `\input` resolves to its *parent* file instead of
//!   itself until a caller with access to the real file tree can improve on this.
//! - **What does work: a directory separator, or an extension, anywhere in the candidate.** A
//!   path with a space in a directory name (`fixtures/space-in-path`) still has a `/` even though
//!   it has no extension; `article.cls`, `size10.clo` and every other real load in these fixtures
//!   has an extension even when it lacks a `/`. Neither signal alone covers every real file, but
//!   between them they cover everything captured so far, and neither ever fires on the incidental
//!   parentheticals also captured (`(HO)`, `(DPC)`, `(Unicode)`, `(12.0pt too wide)`) — see
//!   `fixtures/overfull-hbox/main.log` for the last of those: `Overfull \hbox (48.75pt too wide)`
//!   has a `.` in `48.75pt`, but not immediately before the closing paren, so the end-anchored
//!   check below does not mistake it for a `.pt` extension.
//!
//! Parens are only walked on [`LineKind::Text`] lines — an error or warning's own prose is never
//! where TeX reports a real file boundary, only where it sometimes quotes one in passing.

use crate::tokenizer::{LineKind, LogLine};

/// An open paren is either a real file, or "something else" — kept as a stack entry rather than
/// simply skipped, so a same-line incidental pair (`in general use (HO)`) still pops the same
/// depth it pushed and never desyncs the *real* file stack sitting underneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StackEntry {
    File(String),
    Incidental,
}

/// The stack of open files (root first, innermost last) after each of `lines` has been applied in
/// order. `result[i]` is the state once line `i` itself has been processed, so the file a
/// diagnostic *on* line `i` belongs to is `result[i].last()`, not `result[i - 1].last()`.
pub fn open_files(lines: &[LogLine]) -> Vec<Vec<String>> {
    let mut stack: Vec<StackEntry> = Vec::new();
    let mut result = Vec::with_capacity(lines.len());
    for line in lines {
        if line.kind == LineKind::Text {
            apply(&line.text, &mut stack);
        }
        // `apply`'s return value (what opened and closed again within this one line) is not
        // meaningful at this per-line granularity — see its own doc comment — so it is dropped
        // here and used directly by tests instead.
        result.push(files_only(&stack));
    }
    result
}

/// The file open at logical line `index`, the direct answer to "what file was this diagnostic
/// in" once its line index is known. `None` means either no file has opened yet (a log's first
/// few lines, before `main.tex` itself opens) or every open file has since closed.
pub fn file_at(stacks: &[Vec<String>], index: usize) -> Option<&str> {
    stacks.get(index)?.last().map(String::as_str)
}

fn files_only(stack: &[StackEntry]) -> Vec<String> {
    stack
        .iter()
        .filter_map(|entry| match entry {
            StackEntry::File(name) => Some(name.clone()),
            StackEntry::Incidental => None,
        })
        .collect()
}

/// Applies one logical line's `(`/`)` characters to `stack`, and returns every candidate this
/// call recognised as a real file, in the order it opened — including a leaf file that opens and
/// closes again within this same line (every `.sty`/`.clo` load these fixtures capture does
/// exactly this), which `open_files`'s own once-per-line snapshot can never show, since it is
/// already popped again by the time the line finishes and the snapshot is taken.
fn apply(text: &str, stack: &mut Vec<StackEntry>) -> Vec<String> {
    let mut opened = Vec::new();
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => {
                let rest = &text[at + 1..];
                let end = rest.find(['(', ')']).unwrap_or(rest.len());
                let candidate = rest[..end].trim();
                if looks_like_a_file(candidate) {
                    stack.push(StackEntry::File(candidate.to_string()));
                    opened.push(candidate.to_string());
                } else {
                    stack.push(StackEntry::Incidental);
                }
            }
            ')' => {
                // A `)` with nothing open (a truncated log, or a paren this resolver never
                // pushed for) pops nothing rather than panicking — `Vec::pop` on an empty stack
                // is `None`, not an error.
                stack.pop();
            }
            _ => {}
        }
    }
    opened
}

/// A `/` anywhere, or a `.` followed only by 1-4 ASCII letters running to the very end of the
/// candidate (a real extension, not a decimal number like `48.75pt` or a sentence-ending
/// abbreviation like `e.g.`, both of which have a `.` but not one immediately before the close).
fn looks_like_a_file(candidate: &str) -> bool {
    if candidate.is_empty() {
        return false;
    }
    if candidate.contains('/') {
        return true;
    }
    match candidate.rfind('.') {
        Some(dot) => {
            let ext = &candidate[dot + 1..];
            !ext.is_empty() && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::tokenize;

    mod looks_like_a_file_tests {
        use super::*;

        #[test]
        fn a_recognised_extension_is_a_file() {
            for name in ["article.cls", "size10.clo", "main.aux", "hyperref.sty"] {
                assert!(looks_like_a_file(name), "{name}");
            }
        }

        #[test]
        fn a_path_with_no_extension_is_still_a_file_if_it_has_a_slash() {
            assert!(looks_like_a_file("sub dir with spaces/chapter one"));
        }

        #[test]
        fn short_acronyms_hyperref_prints_are_not_files() {
            for word in ["HO", "DPC", "Unicode", "rerunfilecheck"] {
                assert!(!looks_like_a_file(word), "{word}");
            }
        }

        #[test]
        fn a_decimal_measurement_is_not_mistaken_for_an_extension() {
            assert!(!looks_like_a_file("48.75pt too wide"));
        }

        #[test]
        fn an_empty_candidate_is_not_a_file() {
            assert!(!looks_like_a_file(""));
        }

        #[test]
        fn a_bare_extensionless_input_is_not_recognised_as_a_file() {
            // The known, irreducible limitation this module's doc comment names: real, but
            // lexically identical to an incidental parenthetical from text alone.
            assert!(!looks_like_a_file("plainchapter"));
        }
    }

    mod open_files_tests {
        use super::*;

        #[test]
        fn tracks_a_single_file_opening_and_closing() {
            let lines = tokenize("(main.aux)\nsome text\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[0], Vec::<String>::new(), "closed again on the same line");
        }

        #[test]
        fn a_file_stays_open_across_lines_until_its_own_close() {
            let lines = tokenize("(main.tex\nsome preamble text\n)\nafter\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[0], vec!["main.tex".to_string()]);
            assert_eq!(stacks[1], vec!["main.tex".to_string()], "still open on the next line");
            assert_eq!(stacks[2], Vec::<String>::new(), "closed");
            assert_eq!(stacks[3], Vec::<String>::new());
        }

        #[test]
        fn nested_files_push_and_pop_in_order() {
            let lines = tokenize("(main.tex (macros.sty)\nback in main\n)\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[0], vec!["main.tex".to_string()], "macros.sty closed on the same line it opened");
            assert_eq!(stacks[1], vec!["main.tex".to_string()]);
            assert_eq!(stacks[2], Vec::<String>::new());
        }

        #[test]
        fn siblings_at_the_same_depth_do_not_leak_into_each_other() {
            let lines = tokenize("(main.tex\n(a.sty) (b.sty)\nrest\n)\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[1], vec!["main.tex".to_string()], "both siblings closed by end of their own line");
        }

        #[test]
        fn an_incidental_parenthetical_does_not_push_onto_the_real_stack() {
            let lines = tokenize("(main.tex\nToken not allowed in a PDF string (Unicode): removing.\n)\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[1], vec!["main.tex".to_string()], "(Unicode) must not appear here");
        }

        #[test]
        fn parens_inside_a_warning_line_are_ignored_even_if_they_look_like_a_file() {
            // A warning's own prose is never where a real file boundary is reported, even when it
            // happens to contain something extension-shaped: `classify` marks this whole line
            // `Warning`, and `open_files` only walks parens on `Text` lines.
            let lines = tokenize("(main.tex\nLaTeX Warning: check (notes.txt) for details on input line 4.\nstill main\n");
            let stacks = open_files(&lines);
            assert_eq!(stacks[2], vec!["main.tex".to_string()], "notes.txt must not have been pushed");
        }

        #[test]
        fn an_unbalanced_close_does_not_panic() {
            let lines = tokenize(")\n");
            assert_eq!(open_files(&lines), vec![Vec::<String>::new()]);
        }

        #[test]
        fn a_log_that_ends_with_files_still_open_reports_them() {
            // The shape a fatal error leaves behind: TeX halts before closing what it opened, and
            // that unclosed top of stack is exactly the file the fatal error was in.
            let lines = tokenize("(main.tex\n(chapter1.tex\n! Emergency stop.\n");
            let stacks = open_files(&lines);
            assert_eq!(file_at(&stacks, 2), Some("chapter1.tex"));
        }

        #[test]
        fn the_captured_space_in_path_fixture_is_recognised_as_a_file_when_it_opens() {
            // The real captured line opens and closes `sub dir with spaces/chapter one` within
            // itself (` (sub dir with spaces/chapter one) [1`), so `open_files`'s once-per-line
            // snapshot never shows it as the current top of stack — `apply`'s own return value is
            // the honest way to confirm this candidate was recognised at all.
            let log = include_str!("../fixtures/space-in-path/main.log");
            let lines = tokenize(log);
            let line = lines.iter().find(|l| l.text.contains("sub dir with spaces")).unwrap();
            let mut stack = Vec::new();
            let opened = apply(&line.text, &mut stack);
            assert_eq!(opened, vec!["sub dir with spaces/chapter one".to_string()]);
        }

        #[test]
        fn the_captured_overfull_hbox_fixture_does_not_disturb_the_stack() {
            let log = include_str!("../fixtures/overfull-hbox/main.log");
            let lines = tokenize(log);
            let stacks = open_files(&lines);
            let overfull_line = lines.iter().position(|l| l.text.starts_with("Overfull \\hbox")).unwrap();
            // main.tex is still the only thing open; the hbox's own "(48.75pt too wide)" must not
            // have pushed a second, bogus entry.
            assert_eq!(stacks[overfull_line].len(), 1, "{:?}", stacks[overfull_line]);
        }

        #[test]
        fn the_captured_bare_extensionless_input_resolves_to_its_parent_instead() {
            // Pins the documented limitation: `plainchapter` never becomes its own stack entry,
            // so a diagnostic inside it is reported against main.tex, not itself.
            let log = include_str!("../fixtures/bare-input-no-extension/main.log");
            let lines = tokenize(log);
            let stacks = open_files(&lines);
            let plainchapter_line = lines.iter().position(|l| l.text.contains("plainchapter")).unwrap();
            assert_eq!(file_at(&stacks, plainchapter_line), Some("main.tex"));
        }
    }
}
