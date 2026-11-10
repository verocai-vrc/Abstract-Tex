//! E3: reads BibTeX's own log, the `.blg`, which the rest of this crate never sees.
//!
//! BibTeX does not write its errors into the engine's `.log`: they go to `main.blg`, so
//! [`crate::diagnostics`] cannot know about them. Most of what BibTeX could say reaches the
//! author another way (the app's `.bib` health checks read the entries themselves, and LaTeX
//! reports an undefined citation in its own log), so this module recognises only what **nothing
//! else says**: a style file BibTeX cannot find, an entry whose key is already taken, and a
//! document that cites nothing at all.
//!
//! **Text in, data out, like the rest of the crate.** The caller reads the file. And the caller
//! must hand over only the **root document's** `.blg`: with `\include`, Tectonic and `latexmk`
//! run BibTeX on every chapter's `.aux` as well, and each of those logs carries three errors
//! ("I found no \citation commands", no `\bibdata`, no `\bibstyle`) that are not the author's.
//! Nothing here can tell a chapter's log from the root's, so choosing the right file is the
//! caller's job.
//!
//! It never guesses. A BibTeX line this module does not recognise produces nothing, not a
//! "TeX reported an error" card: BibTeX's wording is not TeX's, and a fallback explanation built
//! from it would be exactly the raw log wearing a card (DESIGN.md §2, commitment 3).

use crate::rules::{explain, Diagnostic};
use crate::QuickError;

/// Explain what the root document's `.blg` says that nothing else does.
pub fn bibtex_diagnostics(blg: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = blg.lines().collect();
    let mut found = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let line = line.trim_end();
        // The first two shapes are one line and report no place; the third carries its own.
        let error = if line.starts_with("I couldn't open style file ")
            || line.starts_with("I found no \\citation commands")
        {
            Some(unplaced(line))
        } else if let Some(rest) = line.strip_prefix("Repeated entry---line ") {
            repeated_entry(rest, lines.get(index + 1).copied())
        } else {
            None
        };
        if let Some(error) = error {
            found.push(explain(&error));
        }
    }
    found
}

/// An error BibTeX reports against no `.tex` line: it names a line of the `.aux`, which is a file
/// the author never opens, so there is no file or line to give.
fn unplaced(message: &str) -> QuickError {
    QuickError {
        message: message.to_string(),
        line: None,
        context: None,
        file: None,
        open_files: Vec::new(),
    }
}

/// `Repeated entry---line 2 of file refs.bib`, then ` : @article{a` on the next line. `rest` is
/// what follows `---line `. A line that does not parse is left alone rather than guessed at.
fn repeated_entry(rest: &str, next_line: Option<&str>) -> Option<QuickError> {
    let (number, file) = rest.split_once(" of file ")?;
    let line: u32 = number.trim().parse().ok()?;
    let file = file.trim().to_string();
    // The context line is `@article{a`, the entry's type and key up to the point BibTeX stopped.
    let context = next_line
        .and_then(|text| text.trim_start().strip_prefix(": "))
        .map(|text| text.trim().to_string());
    Some(QuickError {
        message: "Repeated entry".to_string(),
        line: Some(line),
        context,
        open_files: vec![file.clone()],
        file: Some(file),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;

    fn from_fixture(blg: &str) -> Vec<Diagnostic> {
        bibtex_diagnostics(blg)
    }

    #[test]
    fn a_style_file_bibtex_cannot_open_is_an_error_that_names_the_style() {
        let found = from_fixture(include_str!("../fixtures/bibtex-style-not-found/main.blg"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, Some("bibtex-style-not-found"));
        assert_eq!(found[0].severity, Severity::Error);
        assert!(found[0].title.contains("nosuchstyle"), "{}", found[0].title);
        assert_eq!((found[0].file.as_deref(), found[0].line), (None, None));
    }

    #[test]
    fn a_repeated_entry_points_at_the_bib_file_and_line_and_names_the_key() {
        let found = from_fixture(include_str!("../fixtures/bibtex-repeated-entry/main.blg"));
        assert_eq!(found.len(), 1, "only the repeated entry is recognised: {found:?}");
        assert_eq!(found[0].rule, Some("bibtex-repeated-entry"));
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].file.as_deref(), Some("refs.bib"));
        assert_eq!(found[0].open_files, vec!["refs.bib".to_string()]);
        assert_eq!(found[0].line, Some(2));
        assert!(found[0].title.contains("`a`"), "{}", found[0].title);
    }

    #[test]
    fn a_document_that_cites_nothing_is_a_warning_not_an_error() {
        let found = from_fixture(include_str!("../fixtures/bibtex-no-citations/main.blg"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, Some("bibtex-no-citations"));
        assert_eq!(found[0].severity, Severity::Warning);
    }

    #[test]
    fn a_log_with_nothing_wrong_gives_nothing() {
        let clean = "This is BibTeX, Version 0.99d\nThe top-level auxiliary file: main.aux\n\
                     The style file: plain.bst\nDatabase file #1: refs.bib\n";
        assert_eq!(bibtex_diagnostics(clean), Vec::new());
    }

    /// BibTeX's other messages are not this module's: a missing database file and a missing entry
    /// are said by the `.bib` health checks and by LaTeX's own log, and a half-recognised line
    /// must never become a "TeX reported an error" card.
    #[test]
    fn a_bibtex_message_this_module_does_not_know_produces_no_card() {
        let log = "I couldn't open database file nosuch.bib\n\
                   Warning--I didn't find a database entry for \"zzz\"\n\
                   Warning--empty author and editor in b\n\
                   I found no \\bibdata command---while reading file main.aux\n";
        assert_eq!(bibtex_diagnostics(log), Vec::new());
    }

    #[test]
    fn two_repeated_entries_give_two_cards_in_order() {
        let log = "Repeated entry---line 4 of file refs.bib\n : @book{x\n :        ,\n\
                   Repeated entry---line 9 of file bib/more.bib\n : @article{y\n :           ,\n";
        let found = bibtex_diagnostics(log);
        let places: Vec<(Option<&str>, Option<u32>)> =
            found.iter().map(|d| (d.file.as_deref(), d.line)).collect();
        assert_eq!(
            places,
            vec![(Some("refs.bib"), Some(4)), (Some("bib/more.bib"), Some(9))]
        );
    }

    #[test]
    fn a_repeated_entry_line_that_does_not_parse_is_skipped_not_guessed() {
        assert_eq!(
            bibtex_diagnostics("Repeated entry---line x of file refs.bib\n"),
            Vec::new()
        );
        assert_eq!(bibtex_diagnostics("Repeated entry---line 3\n"), Vec::new());
    }
}
