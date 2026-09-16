//! The rule catalog (DESIGN.md §5.2): TeX's message in, a sentence a person can act on out.
//!
//! Owns: recognising a small set of known failures and explaining them in complete sentences.
//! It must never read a file, spawn anything, or know about the editor — text in, data out —
//! so this crate can be extracted as a standalone MIT crate at S6.5.
//!
//! **Six rule ids of about forty (S2.6's five errors, one split into two ids), landed early in
//! sprint 2 so the v0.1 demo could show an explanation instead of a raw log (SPRINTS.md §2)
//! rather than waiting for the sprint the rule catalog otherwise belongs to.** What is
//! deliberately *not* here yet: the paren-stack resolver that says which *file* an error is in
//! is built (`resolver.rs`, S5.2) but not wired into [`diagnostics`], which still runs on
//! [`crate::quick_errors`]'s un-unwrapped scan — see `lib.rs`'s own module doc — and *applying*
//! a fix (S6.2) rather than only describing one. [`Rule`] is the matcher-plus-explanation-plus-
//! optional-fix shape every entry in [`CATALOG`] implements (S5.5); [`FnRule`] is the one
//! implementation this crate needs today, wrapping the plain functions each of these six rules
//! already had. A future rule with no logic at all — a fixed prefix and a fixed sentence, no
//! dynamic content — would implement [`Rule`] directly instead, as pure data with no `fn`.
//!
//! A rule never guesses. If no rule matches, the diagnostic still appears with TeX's own words
//! rather than being dropped, because an unexplained error the author can see beats a silent
//! one — but it is marked `rule: None` so the drawer can tell the two apart.

use crate::QuickError;

/// How much the author should care. Sprint 2 needs only these two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// The build stopped, or produced a PDF that is wrong in a way TeX noticed.
    Error,
    /// The build finished, but something in it is not what the author meant.
    Warning,
}

/// One explained problem, ready for the drawer. This is what the frontend renders; it never
/// sees a log line (DESIGN.md §2, "never show a raw log by default").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// A short noun phrase naming the mistake: "Underscore used outside maths".
    pub title: String,
    /// Complete sentences saying what happened and what to do. Never TeX's own wording.
    pub explanation: String,
    /// TeX's `l.NN` claim, which is frequently approximate and sometimes absent.
    pub line: Option<u32>,
    pub severity: Severity,
    /// The rule that recognised this, or `None` when nothing in the catalog matched and
    /// `explanation` is a fallback built from TeX's own message.
    pub rule: Option<&'static str>,
    /// TeX's original message, kept for the "raw log" affordance and for support reports.
    pub raw_message: String,
    /// A safe, unambiguous edit the drawer can offer as a button, or `None` — most rules have no
    /// automatic fix (DESIGN.md §5.2's own rule: only offer one when the correction cannot be
    /// wrong). *Applying* it is S6.2's job; this crate only describes one.
    pub fix: Option<Fix>,
}

/// One unambiguous edit, named the way DESIGN.md §5.2's own worked example does: "Escape as
/// `\_`". `find`/`replace` are literal substrings, not byte offsets — this crate never reads the
/// `.tex` source (`lib.rs`'s own rule), so it has no position to give, only the text either side
/// of the change. The caller, which does have the source line (the file, plus this diagnostic's
/// `line`), finds `find` on that line and replaces it with `replace`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    /// Shown on the button, e.g. "Escape as \_".
    pub description: String,
    pub find: String,
    pub replace: String,
}

/// What every entry in [`CATALOG`] must be able to do: recognise a message, explain it, and
/// optionally offer a [`Fix`]. `fix` defaults to `None` because that is the common case —
/// DESIGN.md §5.2 is explicit that most rules have nothing safe to offer.
///
/// [`CATALOG`] holds these as *trait objects* (`&dyn Rule`, a fat pointer of data plus a vtable
/// of these three methods) rather than one concrete type, because not every rule S6.1 adds will
/// need the same shape: some will be pure data (a fixed prefix, a fixed sentence, no logic at
/// all), others will need real code the way [`explain_missing_dollar`] does (picking between
/// "subscript" and "superscript"). A trait lets both kinds sit in the same slice; today
/// [`FnRule`] is the only implementation this crate needs, since none of its six rules are simple
/// enough yet to be pure data.
trait Rule {
    /// Stable identifier, used by tests and shown to nobody.
    fn id(&self) -> &'static str;
    fn severity(&self) -> Severity;
    /// Does this rule recognise the message? Cheap; called for every rule until one says yes.
    fn matches(&self, error: &QuickError) -> bool;
    /// Build the title and the sentences. Only called for the rule that matched.
    fn explain(&self, error: &QuickError) -> (String, String);
    fn fix(&self, error: &QuickError) -> Option<Fix> {
        let _ = error;
        None
    }
}

/// A [`Rule`] built from plain functions — what every rule in this catalog is today.
/// `fn(&QuickError) -> bool` is a *function pointer*, not a closure: every one of these is
/// written with no captured state, so a `FnRule` value needs no allocation and can be built
/// straight inside the `const` [`CATALOG`] slice below.
struct FnRule {
    id: &'static str,
    severity: Severity,
    matches: fn(&QuickError) -> bool,
    explain: fn(&QuickError) -> (String, String),
    /// `None` for every rule below except `missing-dollar`, the one case in this catalog where
    /// the fix is unambiguous enough to offer automatically.
    fix: Option<fn(&QuickError) -> Option<Fix>>,
}

impl Rule for FnRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn matches(&self, error: &QuickError) -> bool {
        (self.matches)(error)
    }

    fn explain(&self, error: &QuickError) -> (String, String) {
        (self.explain)(error)
    }

    fn fix(&self, error: &QuickError) -> Option<Fix> {
        self.fix.and_then(|f| f(error))
    }
}

/// The catalog, tried in order. Order matters only where two matchers could both fire; keep the
/// more specific rule first.
const CATALOG: &[&dyn Rule] = &[
    &FnRule {
        id: "undefined-control-sequence",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Undefined control sequence"),
        explain: explain_undefined_control_sequence,
        fix: None,
    },
    &FnRule {
        id: "missing-dollar",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Missing $ inserted"),
        explain: explain_missing_dollar,
        fix: Some(fix_missing_dollar),
    },
    &FnRule {
        id: "unbalanced-braces",
        severity: Severity::Error,
        // `File ended while scanning use of \cmd` is the wording XeTeX actually produces for an
        // unclosed argument — `Runaway argument?` is printed *above* it without a `!`, so it
        // never reaches us as a message at all. Captured from the fixture, not guessed.
        matches: |e| {
            e.message.starts_with("Missing } inserted")
                || e.message.starts_with("Too many }'s")
                || e.message.starts_with("File ended while scanning")
                || e.message.contains("ended before") && e.message.contains("was complete")
        },
        explain: explain_unbalanced_braces,
        fix: None,
    },
    &FnRule {
        id: "file-not-found",
        severity: Severity::Error,
        matches: |e| e.message.contains("not found") && e.message.contains("File "),
        explain: explain_file_not_found,
        fix: None,
    },
    &FnRule {
        id: "undefined-reference",
        severity: Severity::Warning,
        matches: |e| e.message.starts_with("Reference ") && e.message.contains("undefined"),
        explain: explain_undefined_reference,
        fix: None,
    },
    &FnRule {
        id: "undefined-citation",
        severity: Severity::Warning,
        matches: |e| e.message.starts_with("Citation ") && e.message.contains("undefined"),
        explain: explain_undefined_citation,
        fix: None,
    },
];

/// Explain everything in a log: the `!` errors, plus the warnings that mean the PDF is wrong.
///
/// This is the only entry point the app should call. [`crate::quick_errors`] stays as the raw
/// scan underneath it.
pub fn diagnostics(log: &str) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = crate::quick_errors(log).iter().map(explain).collect();
    found.extend(undefined_reference_warnings(log).iter().map(explain));
    found
}

/// Run one raw error through the catalog.
pub fn explain(error: &QuickError) -> Diagnostic {
    for rule in CATALOG {
        if rule.matches(error) {
            let (title, explanation) = rule.explain(error);
            return Diagnostic {
                title,
                explanation,
                line: error.line,
                severity: rule.severity(),
                rule: Some(rule.id()),
                raw_message: error.message.clone(),
                fix: rule.fix(error),
            };
        }
    }
    // The long tail. S13.2 sends these to a model; until then the author at least sees that
    // something went wrong, and where, without having to open the log.
    Diagnostic {
        title: "TeX reported an error".to_string(),
        explanation: format!(
            "Preamble does not have an explanation for this one yet. TeX said: \"{}\". \
             The raw log has the surrounding output.",
            error.message.trim_end_matches('.')
        ),
        line: error.line,
        severity: Severity::Error,
        rule: None,
        raw_message: error.message.clone(),
        fix: None,
    }
}

/// LaTeX reports undefined `\ref`s and `\cite`s as warnings, not `!` errors, so `quick_errors`
/// never sees them — but a paper full of bold `??` marks is exactly the kind of thing an author
/// wants told to their face. This is a deliberately narrow scan for those two lines; the general
/// warning tokenizer is S5.1.
fn undefined_reference_warnings(log: &str) -> Vec<QuickError> {
    log.lines()
        .filter_map(|line| {
            // `LaTeX Warning: Reference `fig:setup' on page 3 undefined on input line 42.`
            let rest = line.trim().strip_prefix("LaTeX Warning: ")?;
            if !rest.contains("undefined") {
                return None;
            }
            if !rest.starts_with("Reference ") && !rest.starts_with("Citation ") {
                return None;
            }
            Some(QuickError {
                message: rest.trim().to_string(),
                line: input_line_number(rest),
                context: None,
            })
        })
        .collect()
}

/// `… undefined on input line 42.` → `Some(42)`. The warning carries its own line number in
/// prose rather than as an `l.NN` marker.
fn input_line_number(text: &str) -> Option<u32> {
    let after = text.split("on input line ").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The quoted name in a message like ``Reference `fig:setup' on page 3 undefined``. TeX quotes
/// with a backtick and a straight apostrophe, never a matching pair.
fn quoted_name(text: &str) -> Option<&str> {
    let start = text.find('`')? + 1;
    let end = text[start..].find('\'')? + start;
    Some(&text[start..end])
}

/// The command TeX choked on. It prints the source up to the offending token on the `l.NN`
/// line, so the *last* control sequence on that line is the one it could not digest.
fn trailing_command(context: Option<&String>) -> Option<String> {
    let context = context?;
    let start = context.rfind('\\')?;
    let name: String = context[start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(format!("\\{name}"))
}

fn explain_undefined_control_sequence(error: &QuickError) -> (String, String) {
    match trailing_command(error.context.as_ref()) {
        Some(command) => (
            format!("{command} is not a command TeX knows"),
            format!(
                "TeX reached {command} and had no definition for it. Either it is a typo for a \
                 command that does exist, or it comes from a package this document does not load \
                 yet — check the spelling first, then the \\usepackage lines in your preamble."
            ),
        ),
        None => (
            "An unknown command".to_string(),
            "TeX reached a command it has no definition for. Either it is a typo, or it comes \
             from a package this document does not load yet."
                .to_string(),
        ),
    }
}

/// The character that broke maths mode, and its name — the two shapes this crate recognises.
/// Shared between the explanation ([`explain_missing_dollar`]) and the fix
/// ([`fix_missing_dollar`]) so the two can never end up naming different symbols for the same
/// error.
fn detect_math_symbol(context: &str) -> Option<(&'static str, &'static str)> {
    if context.ends_with('_') || context.contains('_') {
        Some(("_", "subscript"))
    } else if context.ends_with('^') || context.contains('^') {
        Some(("^", "superscript"))
    } else {
        None
    }
}

fn explain_missing_dollar(error: &QuickError) -> (String, String) {
    // Naming the actual character the author typed is the whole difference between this and
    // TeX's own message, which names a `$` they did not type (DESIGN.md §5.2).
    let context = error.context.as_deref().unwrap_or("");
    let Some((symbol, name)) = detect_math_symbol(context) else {
        return (
            "A maths symbol used outside maths".to_string(),
            "This line uses a character that only means something inside maths mode, so TeX \
             tried to open maths mode for you and then gave up. Wrap the mathematical part of \
             the line in $…$."
                .to_string(),
        );
    };
    (
        format!("{symbol} used outside maths"),
        format!(
            "`{symbol}` means \"{name}\" and only works inside maths mode. This line is ordinary \
             text, so TeX tried to open maths mode for you and gave up. Wrap the mathematical \
             part in $…$ — or, if you meant a literal {symbol}, write \\{symbol}."
        ),
    )
}

/// The one unambiguous fix in this catalog so far: escaping the literal symbol TeX choked on.
/// The explanation's *other* suggestion — "wrap the mathematical part in $…$" — is not offered as
/// a button: this crate never reads the `.tex` source (`lib.rs`'s own rule), so it has no way to
/// know where the mathematical part *starts*, only where the symbol that broke it is. DESIGN.md
/// §5.2's own design rule is exactly this: a fix may only be automatic when it cannot be wrong,
/// and escaping one known character always qualifies where guessing a span does not.
fn fix_missing_dollar(error: &QuickError) -> Option<Fix> {
    let (symbol, _name) = detect_math_symbol(error.context.as_deref().unwrap_or(""))?;
    Some(Fix {
        description: format!("Escape as \\{symbol}"),
        find: symbol.to_string(),
        replace: format!("\\{symbol}"),
    })
}

fn explain_unbalanced_braces(error: &QuickError) -> (String, String) {
    if error.message.starts_with("Too many }'s") {
        return (
            "One closing brace too many".to_string(),
            "There is a } here with no { to match it. Usually a brace was deleted from the \
             opening side of a command's argument, or one was typed twice at the end."
                .to_string(),
        );
    }
    // "File ended while scanning use of \emph ." and "Paragraph ended before \emph was
    // complete." both name the command whose argument ran away, which is far more useful than
    // the line number — TeX only notices at the end of the file or paragraph, so `l.NN`, when
    // there is one at all, points at the end of the runaway rather than at its start.
    match trailing_command(Some(&error.message)) {
        Some(command) => (
            format!("{command} is missing its closing brace"),
            format!(
                "The argument to {command} was opened with {{ and never closed, so TeX kept \
                 collecting text until it ran out. Look for the {command} whose argument is \
                 missing its }} — it is earlier in the file than any line number reported here."
            ),
        ),
        None => (
            "A brace was never closed".to_string(),
            "A { was opened and the group it started never ended, so TeX ran past the end of \
             the argument it was collecting. The mistake is earlier than the line reported \
             here: look for a command whose argument is missing its closing }."
                .to_string(),
        ),
    }
}

fn explain_file_not_found(error: &QuickError) -> (String, String) {
    let name = quoted_name(&error.message).unwrap_or("that file");
    let is_package = name.ends_with(".sty") || !name.contains('.');
    let explanation = if is_package {
        format!(
            "This document asks for `{name}`, and the engine could not find it. If it is a \
             package, check the name for a typo; Preamble's bundled engine downloads packages \
             on demand, so a correctly spelled one will be fetched the next time you build with \
             a network connection."
        )
    } else {
        format!(
            "This document includes `{name}`, and it is not where TeX looked. Check the spelling \
             and the path — it is resolved relative to the folder holding the main file, not the \
             file that includes it."
        )
    };
    (format!("`{name}` could not be found"), explanation)
}

fn explain_undefined_reference(error: &QuickError) -> (String, String) {
    let name = quoted_name(&error.message).unwrap_or("a label");
    (
        format!("`{name}` is referenced but never defined"),
        format!(
            "Nothing in this document calls \\label{{{name}}}, so the reference printed as ?? \
             in the PDF. Check the spelling against the \\label you meant, and remember that a \
             \\label must come *after* the \\caption inside a figure or table."
        ),
    )
}

fn explain_undefined_citation(error: &QuickError) -> (String, String) {
    let name = quoted_name(&error.message).unwrap_or("a key");
    (
        format!("`{name}` is cited but not in the bibliography"),
        format!(
            "No entry with the key `{name}` was found in this project's .bib files, so the \
             citation printed as [?] in the PDF. Check the key for a typo, and make sure the \
             .bib file holding it is listed in \\bibliography or \\addbibresource."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a log through the whole path and return the diagnostic the named rule produced.
    fn diagnostic_from(log: &str, rule_id: &str) -> Diagnostic {
        let found = diagnostics(log);
        found
            .into_iter()
            .find(|d| d.rule == Some(rule_id))
            .unwrap_or_else(|| panic!("no `{rule_id}` diagnostic in that log"))
    }

    /// Every explanation is shown to a person, so it must read like prose, not like a log line.
    fn assert_reads_like_a_sentence(diagnostic: &Diagnostic) {
        assert!(!diagnostic.title.is_empty(), "a diagnostic must have a title");
        assert!(
            diagnostic.explanation.ends_with('.'),
            "explanations are complete sentences: {:?}",
            diagnostic.explanation
        );
        assert!(
            diagnostic.explanation.split_whitespace().count() >= 12,
            "an explanation of a dozen words or fewer is a log line with punctuation: {:?}",
            diagnostic.explanation
        );
    }

    #[test]
    fn rule_1_undefined_control_sequence_names_the_command() {
        let log = "! Undefined control sequence.\nl.12 \\textbold\n              {Introduction}\n";
        let d = diagnostic_from(log, "undefined-control-sequence");
        assert_eq!(d.title, "\\textbold is not a command TeX knows");
        assert_eq!(d.line, Some(12));
        assert_eq!(d.severity, Severity::Error);
        assert!(d.explanation.contains("\\usepackage"), "{}", d.explanation);
        assert_reads_like_a_sentence(&d);
    }

    #[test]
    fn rule_1_survives_a_log_with_no_usable_context() {
        let log = "! Undefined control sequence.\n";
        let d = diagnostic_from(log, "undefined-control-sequence");
        assert_eq!(d.title, "An unknown command");
        assert_eq!(d.line, None);
        assert_reads_like_a_sentence(&d);
    }

    #[test]
    fn rule_2_missing_dollar_names_the_underscore_the_author_typed() {
        // The case from DESIGN.md §5.2, whose whole point is that TeX's own message names a `$`
        // the author never typed.
        let log = "! Missing $ inserted.\n<inserted text>\n                $\nl.87 The sample size n_\n";
        let d = diagnostic_from(log, "missing-dollar");
        assert_eq!(d.title, "_ used outside maths");
        assert_eq!(d.line, Some(87));
        assert!(d.explanation.contains("subscript"), "{}", d.explanation);
        assert!(!d.explanation.contains("Missing $"), "must not parrot TeX: {}", d.explanation);
        assert_reads_like_a_sentence(&d);
    }

    #[test]
    fn rule_2_recognises_a_superscript_too() {
        let log = "! Missing $ inserted.\nl.9 the 3^\n";
        let d = diagnostic_from(log, "missing-dollar");
        assert_eq!(d.title, "^ used outside maths");
        assert!(d.explanation.contains("superscript"), "{}", d.explanation);
    }

    #[test]
    fn rule_2_offers_the_escape_fix_for_an_underscore() {
        let log = "! Missing $ inserted.\nl.87 The sample size n_\n";
        let d = diagnostic_from(log, "missing-dollar");
        let fix = d.fix.expect("an underscore has an unambiguous escape fix");
        assert_eq!(fix.description, "Escape as \\_");
        assert_eq!(fix.find, "_");
        assert_eq!(fix.replace, "\\_");
    }

    #[test]
    fn rule_2_offers_the_escape_fix_for_a_superscript_too() {
        let log = "! Missing $ inserted.\nl.9 the 3^\n";
        let d = diagnostic_from(log, "missing-dollar");
        let fix = d.fix.expect("a superscript has an unambiguous escape fix too");
        assert_eq!(fix.find, "^");
        assert_eq!(fix.replace, "\\^");
    }

    #[test]
    fn rule_2_offers_no_fix_when_neither_symbol_is_recognised() {
        // The same case `explain_missing_dollar` falls back to its generic wording for: no `_`
        // or `^` in the context line, so there is no symbol to escape.
        let log = "! Missing $ inserted.\nl.3 something else entirely\n";
        let d = diagnostic_from(log, "missing-dollar");
        assert_eq!(d.fix, None);
    }

    #[test]
    fn only_missing_dollar_offers_a_fix_in_this_catalog() {
        // DESIGN.md §5.2's own rule: a fix may only be automatic when the correction cannot be
        // wrong. None of this catalog's other five rules — an unknown command, a brace problem, a
        // missing file, an undefined reference or citation — can be resolved by an edit this
        // crate could describe from the log alone, so every one of them must offer `None`.
        for (log, rule_id) in [
            ("! Undefined control sequence.\nl.12 \\textbold\n", "undefined-control-sequence"),
            ("! Missing } inserted.\nl.5 x\n", "unbalanced-braces"),
            ("! LaTeX Error: File `nosuch.sty' not found.\n", "file-not-found"),
            ("LaTeX Warning: Reference `x' on page 1 undefined on input line 1.\n", "undefined-reference"),
            ("LaTeX Warning: Citation `x' on page 1 undefined on input line 1.\n", "undefined-citation"),
        ] {
            let d = diagnostic_from(log, rule_id);
            assert_eq!(d.fix, None, "{rule_id} should not offer a fix yet");
        }
    }

    #[test]
    fn rule_3_unbalanced_braces_covers_the_wordings_tex_actually_uses() {
        for message in [
            "! Missing } inserted.",
            "! Too many }'s.",
            "! File ended while scanning use of \\emph .",
            "! Paragraph ended before \\textbf was complete.",
        ] {
            let log = format!("{message}\nl.5 x\n");
            let d = diagnostic_from(&log, "unbalanced-braces");
            assert_reads_like_a_sentence(&d);
        }
    }

    #[test]
    fn rule_3_tells_the_two_directions_apart() {
        let too_many = diagnostic_from("! Too many }'s.\nl.5 x\n", "unbalanced-braces");
        let unclosed = diagnostic_from("! Missing } inserted.\nl.5 x\n", "unbalanced-braces");
        assert_ne!(too_many.title, unclosed.title, "a stray closing brace is not the same mistake as an unclosed one");
    }

    #[test]
    fn rule_4_file_not_found_quotes_the_name_and_explains_on_demand_fetching() {
        let log = "! LaTeX Error: File `nosuch.sty' not found.\n";
        let d = diagnostic_from(log, "file-not-found");
        assert_eq!(d.title, "`nosuch.sty` could not be found");
        assert!(d.explanation.contains("downloads packages on demand"), "{}", d.explanation);
        assert_reads_like_a_sentence(&d);
    }

    /// The five rules, each against a log a real Tectonic 0.17.0 run produced. Hand-written log
    /// excerpts are a test of the matcher against my idea of TeX; these are a test against TeX.
    /// Three of the rules above were wrong until these fixtures were captured.
    mod captured_from_a_real_engine {
        use super::*;

        #[test]
        fn undefined_control_sequence() {
            let log = include_str!("../fixtures/undefined-control-sequence/main.log");
            let d = diagnostic_from(log, "undefined-control-sequence");
            assert_eq!(d.title, "\\textbold is not a command TeX knows");
            assert_eq!(d.line, Some(4));
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn missing_dollar() {
            // Captured in S1.11; its `l.5` context is wrapped at 79 columns and truncated with a
            // leading `...`, and the rule must still name the underscore.
            let log = include_str!("../fixtures/broken-underscore/main.log");
            let d = diagnostic_from(log, "missing-dollar");
            assert_eq!(d.title, "_ used outside maths");
            assert_eq!(d.line, Some(5));
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn unbalanced_braces() {
            // XeTeX prints `Runaway argument?` with no `!` and only then the real error, which
            // carries no `l.NN` marker at all — hence no line number, and hence an explanation
            // that leans on the command name instead of pretending to know where to look.
            let log = include_str!("../fixtures/unbalanced-braces/main.log");
            let d = diagnostic_from(log, "unbalanced-braces");
            assert_eq!(d.title, "\\emph is missing its closing brace");
            assert_eq!(d.line, None);
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn file_not_found() {
            // Tectonic doubles the bang on this one: `! ! LaTeX Error: File ... not found..`
            let log = include_str!("../fixtures/missing-package/main.log");
            let d = diagnostic_from(log, "file-not-found");
            assert_eq!(d.title, "`nosuchpackage.sty` could not be found");
            assert!(d.explanation.contains("downloads packages on demand"), "{}", d.explanation);
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn undefined_reference_and_citation() {
            let log = include_str!("../fixtures/undefined-reference/main.log");
            let reference = diagnostic_from(log, "undefined-reference");
            assert_eq!(reference.title, "`fig:setup` is referenced but never defined");
            assert_eq!(reference.line, Some(4));

            let citation = diagnostic_from(log, "undefined-citation");
            assert_eq!(citation.title, "`knuth1984` is cited but not in the bibliography");
            assert_eq!(citation.line, Some(4));

            // This log also holds `There were undefined references.` and an `Empty
            // 'thebibliography' environment` warning. Neither is a thing the author can act on
            // separately, and repeating the same problem three times is noise.
            assert_eq!(diagnostics(log).len(), 2, "{:#?}", diagnostics(log));
        }
    }

    #[test]
    fn rule_4_treats_a_missing_chapter_differently_from_a_missing_package() {
        let log = "! LaTeX Error: File `chapters/intro.tex' not found.\n";
        let d = diagnostic_from(log, "file-not-found");
        assert!(d.explanation.contains("relative to the folder"), "{}", d.explanation);
        assert!(!d.explanation.contains("package"), "a .tex include is not a package: {}", d.explanation);
    }

    #[test]
    fn rule_5_undefined_reference_is_a_warning_with_its_own_line_number() {
        let log = "LaTeX Warning: Reference `fig:setup' on page 3 undefined on input line 42.\n";
        let d = diagnostic_from(log, "undefined-reference");
        assert_eq!(d.title, "`fig:setup` is referenced but never defined");
        assert_eq!(d.line, Some(42));
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.explanation.contains("??"), "say what the author will see in the PDF: {}", d.explanation);
        assert_reads_like_a_sentence(&d);
    }

    #[test]
    fn rule_5_undefined_citation_points_at_the_bib_file() {
        let log = "LaTeX Warning: Citation `knuth1984' on page 1 undefined on input line 7.\n";
        let d = diagnostic_from(log, "undefined-citation");
        assert_eq!(d.title, "`knuth1984` is cited but not in the bibliography");
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.explanation.contains("addbibresource"), "{}", d.explanation);
    }

    #[test]
    fn other_latex_warnings_are_not_swept_up() {
        let log = "LaTeX Warning: Label(s) may have changed. Rerun to get cross-references right.\n\
                   LaTeX Font Warning: Font shape `OT1/cmr/bx/sc' undefined on input line 5.\n";
        assert!(diagnostics(log).is_empty(), "only undefined refs and cites, for now");
    }

    #[test]
    fn an_unmatched_error_still_reaches_the_author() {
        let log = "! Dimension too large.\nl.30 \\hspace{99999pt}\n";
        let found = diagnostics(log);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, None);
        assert_eq!(found[0].line, Some(30));
        // Never silently dropped, and never presented as if we understood it.
        assert!(found[0].explanation.contains("Dimension too large"), "{}", found[0].explanation);
        assert!(found[0].explanation.contains("does not have an explanation"), "{}", found[0].explanation);
    }

    #[test]
    fn errors_and_warnings_from_one_log_come_back_together() {
        let log = "! Undefined control sequence.\nl.12 \\textbold\n\
                   LaTeX Warning: Citation `smith2020' on page 1 undefined on input line 30.\n";
        let found = diagnostics(log);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[1].severity, Severity::Warning);
    }

}
