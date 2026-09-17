//! TeX log parsing — the differentiating subsystem (DESIGN.md §5.2).
//!
//! **S5.6 wired the resolver in.** [`rules::diagnostics`] now scans [`tokenizer::tokenize`]'s
//! unwrapped, classified lines and resolves each one's file with [`resolver::open_files`], so a
//! [`rules::Diagnostic`] carries a real `file` alongside its `line`. [`quick_errors`] below is
//! kept as the original raw, un-unwrapped scan — simpler, and still what a handful of this
//! crate's own tests reach for directly — but `diagnostics` no longer calls it; see
//! `rules.rs`'s own module doc for the scan it uses instead.
//!
//! This crate must never read a file, spawn anything, or know about the editor. Text in, data
//! out, so it can be extracted as a standalone MIT crate at S6.5.
//!
//! [`quick_errors`] is the raw scan. [`rules::diagnostics`] is what the app should call: it runs
//! the scan through the rule catalog and returns sentences instead of TeX's own wording.

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
