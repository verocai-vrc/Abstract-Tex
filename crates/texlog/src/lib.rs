//! Parses a TeX `.log` file into diagnostics a person can act on: which file, which line, a
//! plain-language explanation, and — when the correction cannot be wrong — a [`Fix`].
//!
//! Start at [`diagnostics`]: it runs the whole pipeline — [`tokenizer::tokenize`] undoes the
//! transcript writer's 79-column wrap and classifies each line, [`resolver::open_files`] walks
//! the `(`/`)` trail to decide which file was open at each one, and [`rules`]'s catalog turns
//! TeX's own wording into a sentence. Each layer is public on its own for callers that need
//! less than all three.
//!
//! **This crate must never read a file, spawn anything, or know about the editor.** Text in,
//! data out. That rule is what lets it be published on its own (MIT) and used from a GUI, an
//! LSP, a CI check or a shell script alike; every module doc restates it where it bites.
//!
//! [`quick_errors`] is the original raw scan — no unwrapping, no file resolution — kept because
//! it is simpler and a handful of this crate's own tests reach for it directly. `diagnostics`
//! no longer calls it; `rules.rs`'s own module doc describes the scan it uses instead.
//!
//! Every rule ships with a real log captured from a real engine run, under `fixtures/`, and
//! `build.rs` turns each into a test. `fixtures/README.md` records what each capture taught.

// Every public item must carry a doc comment. `-D warnings` in the verify gate turns this into
// an error, so an undocumented field cannot reach crates.io.
#![warn(missing_docs)]

pub mod resolver;
pub mod rules;
pub mod tokenizer;

pub use resolver::{file_at, open_files};
pub use rules::{diagnostics, Diagnostic, Fix, Severity};
pub use tokenizer::{tokenize, LineKind, LogLine, ParenEvent};

/// The crudest useful diagnostic: TeX's own message and the line it claims.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct QuickError {
    /// The text after `! `, e.g. `Undefined control sequence.`
    pub message: String,
    /// The number from the first `l.NN` marker after the message, if any. TeX's claim, which is
    /// often approximate; the resolver in S5.2 will do better.
    pub line: Option<u32>,
    /// The source excerpt TeX printed on the `l.NN` line, if any. Useful context for a person.
    pub context: Option<String>,
    /// The file open on the log's own timeline when this error was printed, resolved by
    /// [`resolver::open_files`]. Always `None` here: this raw scan has no notion of files at
    /// all, unlike `rules.rs`'s own resolver-backed scan, which sets it for real.
    pub file: Option<String>,
}

/// Scan a log for `! ...` errors.
pub fn quick_errors(log: &str) -> Vec<QuickError> {
    let mut errors: Vec<QuickError> = Vec::new();
    let mut lines = log.lines().peekable();

    while let Some(line) = lines.next() {
        let Some(message) = line.strip_prefix("! ") else {
            continue;
        };
        let mut error =
            QuickError { message: message.trim().to_string(), line: None, context: None, file: None };

        // TeX prints the `l.NN` marker within the next few lines; stop looking at the next `!`
        // or after a generous window, whichever comes first.
        let mut looked = 0;
        while let Some(next) = lines.peek() {
            if next.starts_with("! ") || looked > 12 {
                break;
            }
            let next = lines.next().unwrap();
            looked += 1;
            if let Some((num, rest)) = parse_line_marker(next) {
                error.line = Some(num);
                let ctx = rest.trim();
                if !ctx.is_empty() {
                    error.context = Some(ctx.to_string());
                }
                break;
            }
        }
        errors.push(error);
    }
    errors
}

/// `l.87 The sample size n_` → `(87, " The sample size n_")`.
fn parse_line_marker(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix("l.")?;
    let digits_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    if digits_end == 0 {
        return None;
    }
    let num: u32 = rest[..digits_end].parse().ok()?;
    Some((num, &rest[digits_end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_undefined_control_sequence_with_its_line() {
        let log = "\
This is XeTeX, Version 3.141592653
! Undefined control sequence.
l.12 \\textbold
              {Introduction}
?
";
        let errs = quick_errors(log);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].message, "Undefined control sequence.");
        assert_eq!(errs[0].line, Some(12));
        assert_eq!(errs[0].context.as_deref(), Some("\\textbold"));
    }

    #[test]
    fn missing_dollar_error_from_the_design_doc() {
        let log = "\
! Missing $ inserted.
<inserted text>
                $
l.87 The sample size n_
                       {max} was 240.
";
        let errs = quick_errors(log);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].message, "Missing $ inserted.");
        assert_eq!(errs[0].line, Some(87));
        assert_eq!(errs[0].context.as_deref(), Some("The sample size n_"));
    }

    #[test]
    fn errors_without_a_line_marker_still_count() {
        let log = "! LaTeX Error: File `nosuch.sty' not found.\n\nType X to quit.\n";
        let errs = quick_errors(log);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, None);
    }

    #[test]
    fn two_errors_are_kept_apart() {
        let log = "! First.\nl.1 a\n! Second.\nl.2 b\n";
        let errs = quick_errors(log);
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[1].line, Some(2));
    }

    #[test]
    fn captured_fixture_from_a_real_tectonic_run() {
        // Captured in S1.11 from fixtures/broken; see crates/texlog/fixtures/README.md.
        let log = include_str!("../fixtures/broken-underscore/main.log");
        let errs = quick_errors(log);
        assert!(!errs.is_empty(), "fixture log should contain at least one error");
        assert!(errs.iter().any(|e| e.message.starts_with("Missing $ inserted")), "{errs:?}");
    }
}
