//! E7: names the fragile command that `texlog` could not, by reading the diagnosed source line.
//!
//! The `fragile-command-in-moving-argument` rule recovers the offending command from the `l.NN`
//! context line TeX prints, but TeX prints only the last 50 characters of the source up to the
//! error. A realistic `\section{A long chapter title for a real thesis\footnote{...}}` is longer
//! than that, so `\footnote` is cut off, the rule finds no command, and the one-click `\protect`
//! fix is never offered. `texlog` cannot do better: it never reads source (`texlog/src/lib.rs`).
//! This module is the caller that has the file.
//!
//! It reads from disk, not from the editor's buffer, on purpose: the build compiled the files on
//! disk, so the log's `l.NN` counts lines of exactly that text.
//!
//! **This module must never write a file, and never guess.** It offers a fix only when the line
//! holds exactly one fragile command, once, and not already behind a `\protect`; with two
//! candidates it cannot tell which one the error is about, and it says nothing. It also never
//! replaces a fix a rule already offered.

use std::collections::HashMap;
use std::path::Path;

use texlog::{Diagnostic, Fix};

/// The rule this module completes. Must match `rules.rs`'s `id`.
const FRAGILE_RULE: &str = "fragile-command-in-moving-argument";

/// Commands that are fragile in a moving argument and that an author really writes in a section
/// title or a caption. Deliberately short: a command is listed only when `\protect` before it is
/// the standard cure. Whole names only — `\footnote` must not match the start of `\footnotemark`.
const FRAGILE_COMMANDS: [&str; 3] = ["\\footnote", "\\footnotemark", "\\footnotetext"];

/// Give each fragile-command diagnostic that has no fix one, when its source line says which
/// command to protect. `project_dir` is where the diagnostics' project-relative `file`s live.
///
/// `&mut [Diagnostic]` (a mutable slice, not a `Vec`) because this edits elements in place and
/// never adds or removes one, the same shape `place_unlocated_errors` in `compile.rs` has.
pub fn offer_missing_fixes(diagnostics: &mut [Diagnostic], project_dir: &Path) {
    // Each file is read at most once, and only if some diagnostic needs it: most builds have no
    // such diagnostic and never come here. `None` caches "could not be read" so a missing file is
    // not retried for every diagnostic that names it.
    let mut files_read: HashMap<String, Option<String>> = HashMap::new();

    for diagnostic in diagnostics.iter_mut() {
        if diagnostic.rule != Some(FRAGILE_RULE) || diagnostic.fix.is_some() {
            continue;
        }
        let (Some(file), Some(line)) = (diagnostic.file.as_ref(), diagnostic.line) else {
            continue;
        };
        let text = files_read
            .entry(file.clone())
            .or_insert_with(|| std::fs::read_to_string(project_dir.join(file)).ok());
        let Some(text) = text else { continue };
        // `line` is 1-based, and `nth` is 0-based. `saturating_sub` turns a line 0 (which TeX
        // never prints) into the first line rather than an underflow panic.
        let Some(source_line) = text.lines().nth((line as usize).saturating_sub(1)) else {
            continue;
        };
        diagnostic.fix = fix_for_line(source_line);
    }
}

/// The `\protect` fix for `line`, or `None` when it cannot be chosen without a guess.
fn fix_for_line(line: &str) -> Option<Fix> {
    let mut found: Vec<(usize, &str)> = Vec::new();
    for command in FRAGILE_COMMANDS {
        for (at, _) in line.match_indices(command) {
            let after = &line[at + command.len()..];
            // `\footnote` followed by a letter is some longer command's name, not this one.
            if after.starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            found.push((at, command));
        }
    }
    // Exactly one occurrence: with two, `Fix.find` would match the first on the line whichever
    // one the error is about, and a second click would stack a second `\protect` on the first.
    let [(at, command)] = found.as_slice() else {
        return None;
    };
    if line[..*at].ends_with("\\protect") {
        return None;
    }
    Some(Fix {
        description: format!("Add \\protect before {command}"),
        find: (*command).to_string(),
        replace: format!("\\protect{command}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A log shaped like `footnote-in-moving-arg`'s capture, but with a title long enough that
    /// TeX's 50-character context cut `\footnote` off — the case this module exists for.
    const TRUNCATED_LOG: &str = "\
! Argument of \\@sect has an extra }.
<inserted text>
                \\par
l.3 ...a long title for a real thesis chapter{a note}}

No pages of output.
";

    const LONG_TITLE_LINE: &str = "\\section{A long title for a real thesis chapter\\footnote{a note}}";

    fn the_fragile_diagnostic(log: &str) -> Diagnostic {
        let mut found = texlog::diagnostics(&format!("(main.tex\n{log}"));
        assert_eq!(found.len(), 1, "{found:?}");
        found.remove(0)
    }

    fn project_with(main_tex: &str) -> tempfile::TempDir {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("main.tex"), main_tex).unwrap();
        project
    }

    /// The premise: texlog names no command from a truncated context, so it offers no fix.
    #[test]
    fn texlog_alone_offers_no_fix_when_the_context_was_cut_off() {
        let diagnostic = the_fragile_diagnostic(TRUNCATED_LOG);
        assert_eq!(diagnostic.rule, Some(FRAGILE_RULE));
        assert_eq!(diagnostic.fix, None);
        assert_eq!(diagnostic.line, Some(3));
    }

    #[test]
    fn a_cut_off_command_is_found_on_the_source_line() {
        let project = project_with(&format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{LONG_TITLE_LINE}\n"
        ));
        let mut found = vec![the_fragile_diagnostic(TRUNCATED_LOG)];

        offer_missing_fixes(&mut found, project.path());

        assert_eq!(
            found[0].fix,
            Some(Fix {
                description: "Add \\protect before \\footnote".to_string(),
                find: "\\footnote".to_string(),
                replace: "\\protect\\footnote".to_string(),
            })
        );
    }

    #[test]
    fn a_whole_command_name_is_matched_not_the_start_of_a_longer_one() {
        let fix = fix_for_line("\\section{Title\\footnotemark}").unwrap();
        assert_eq!(fix.find, "\\footnotemark");
        assert_eq!(fix.replace, "\\protect\\footnotemark");
    }

    #[test]
    fn two_candidates_on_one_line_are_not_guessed_between() {
        assert_eq!(fix_for_line("\\section{A\\footnote{x} and B\\footnote{y}}"), None);
        assert_eq!(
            fix_for_line("\\section{A\\footnote{x} and B\\footnotemark}"),
            None
        );
    }

    #[test]
    fn a_command_already_behind_protect_is_left_alone() {
        assert_eq!(fix_for_line("\\section{A\\protect\\footnote{x}}"), None);
    }

    #[test]
    fn a_line_with_no_known_fragile_command_gets_no_fix() {
        assert_eq!(fix_for_line("\\section{A title with \\emph{words}}"), None);
    }

    #[test]
    fn a_fix_a_rule_already_offered_is_never_replaced() {
        let project = project_with(&format!("one\ntwo\n{LONG_TITLE_LINE}\n"));
        let mut found = vec![the_fragile_diagnostic(TRUNCATED_LOG)];
        let existing = Fix {
            description: "Add \\protect before \\emph".to_string(),
            find: "\\emph".to_string(),
            replace: "\\protect\\emph".to_string(),
        };
        found[0].fix = Some(existing.clone());

        offer_missing_fixes(&mut found, project.path());

        assert_eq!(found[0].fix, Some(existing));
    }

    #[test]
    fn another_rules_diagnostic_is_not_touched() {
        let project = project_with(&format!("one\ntwo\n{LONG_TITLE_LINE}\n"));
        let mut found = vec![the_fragile_diagnostic(TRUNCATED_LOG)];
        found[0].rule = Some("undefined-control-sequence");

        offer_missing_fixes(&mut found, project.path());

        assert_eq!(found[0].fix, None);
    }

    #[test]
    fn a_file_that_cannot_be_read_or_a_line_past_its_end_offers_nothing_and_does_not_panic() {
        let project = project_with("only one line\n");
        let mut found = vec![the_fragile_diagnostic(TRUNCATED_LOG)];
        offer_missing_fixes(&mut found, project.path()); // line 3 of a one-line file
        assert_eq!(found[0].fix, None);

        found[0].file = Some("no-such-file.tex".to_string());
        offer_missing_fixes(&mut found, project.path());
        assert_eq!(found[0].fix, None);
    }
}
