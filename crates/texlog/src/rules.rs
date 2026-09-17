//! The rule catalog (DESIGN.md §5.2): TeX's message in, a sentence a person can act on out.
//!
//! Owns: recognising a small set of known failures and explaining them in complete sentences.
//! It must never read a file, spawn anything, or know about the editor — text in, data out —
//! so this crate can be extracted as a standalone MIT crate at S6.5.
//!
//! **Six rule ids of about forty (S2.6's five errors, one split into two ids), landed early in
//! sprint 2 so the v0.1 demo could show an explanation instead of a raw log (SPRINTS.md §2)
//! rather than waiting for the sprint the rule catalog otherwise belongs to.** [`Rule`] is the
//! matcher-plus-explanation-plus-optional-fix shape every entry in [`CATALOG`] implements
//! (S5.5); [`FnRule`] is the one implementation this crate needs today, wrapping the plain
//! functions each of these six rules already had. A future rule with no logic at all — a fixed
//! prefix and a fixed sentence, no dynamic content — would implement [`Rule`] directly instead,
//! as pure data with no `fn`. *Applying* a fix (S6.2), rather than only describing one, is still
//! not here.
//!
//! **S5.6: [`diagnostics`] is rebuilt on [`crate::tokenizer`] and [`crate::resolver`], not
//! [`crate::quick_errors`].** `located_errors`/`located_warnings` below walk
//! [`crate::tokenizer::tokenize`]'s unwrapped, classified lines the same way `quick_errors` and
//! the old `undefined_reference_warnings` used to walk raw ones, but each also asks
//! [`crate::resolver::open_files`] which file was open at that line — which is what lets
//! [`Diagnostic::file`] be a real answer instead of always `None`. `crate::quick_errors` itself
//! is untouched, still the plain raw scan `lib.rs`'s own module doc describes.
//!
//! A rule never guesses. If no rule matches, the diagnostic still appears with TeX's own words
//! rather than being dropped, because an unexplained error the author can see beats a silent
//! one — but it is marked `rule: None` so the drawer can tell the two apart.

use crate::resolver::{file_at, open_files};
use crate::tokenizer::{tokenize, LineKind, LogLine};
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
    /// The file open when this diagnostic was printed, resolved by [`crate::resolver::open_files`]
    /// against the log's own `(`/`)` trail — `None` only for a log with no file open at all at
    /// that point (the very first few lines, or a diagnostic after everything has closed, as
    /// `emergency-stop`'s fixture captures). An extensionless `\input` still resolves to its
    /// *parent* file rather than itself; `resolver.rs`'s own module doc names this as a real,
    /// irreducible limit of text-only resolution, not a bug in this field.
    pub file: Option<String>,
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
    // S6.1: rules 7–15 of DESIGN.md §5.2's "about forty" — common structural mistakes with no
    // package involved. Package-specific rules (babel/hyperref/tikz/xcolor/fontspec) and the two
    // box-warning rules follow in their own loops. Each ships with a fixture captured from a real
    // Tectonic 0.17.0 run, the same discipline S2.6/S5.2/S5.4 already leaned on;
    // `fixtures/README.md` names what each exercises.
    &FnRule {
        id: "misplaced-alignment-tab",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Misplaced alignment tab character"),
        explain: explain_misplaced_alignment_tab,
        fix: None,
    },
    &FnRule {
        id: "extra-alignment-tab",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Extra alignment tab has been changed to"),
        explain: explain_extra_alignment_tab,
        fix: None,
    },
    &FnRule {
        id: "undefined-environment",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: Environment") && e.message.contains("undefined"),
        explain: explain_undefined_environment,
        fix: None,
    },
    &FnRule {
        id: "mismatched-environment",
        severity: Severity::Error,
        // Checked before `missing-begin-document` below only by convention, not necessity — the
        // two prefixes (`\begin{` vs `Missing \begin{document}`) never both match one message.
        matches: |e| e.message.starts_with("LaTeX Error: \\begin{") && e.message.contains("ended by \\end{"),
        explain: explain_mismatched_environment,
        fix: None,
    },
    &FnRule {
        id: "missing-begin-document",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: Missing \\begin{document}"),
        explain: explain_missing_begin_document,
        fix: None,
    },
    &FnRule {
        id: "illegal-unit-of-measure",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Illegal unit of measure"),
        explain: explain_illegal_unit_of_measure,
        fix: None,
    },
    &FnRule {
        id: "missing-number",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Missing number, treated as zero"),
        explain: explain_missing_number,
        fix: None,
    },
    &FnRule {
        id: "double-subscript",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Double subscript"),
        explain: explain_double_script,
        fix: None,
    },
    &FnRule {
        id: "double-superscript",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Double superscript"),
        explain: explain_double_script,
        fix: None,
    },
    // S6.1: rules 16–20, the package-specific failures DESIGN.md §5.2 names — "the ten most
    // common package-specific failures from babel, biblatex, hyperref and tikz". `biblatex`
    // itself needs `biber`, a second binary this project does not yet bundle or invoke
    // (DESIGN.md §4.1 only names Tectonic's own bundled engine); its rules are left for whichever
    // loop wires biber in, rather than guessed at without a real capture to check against.
    &FnRule {
        id: "pgfkeys-unknown-key",
        severity: Severity::Error,
        // tikz's own option parser. DESIGN.md §5.2 names tikz explicitly as one of the four
        // packages this catalog's package-specific rules should cover.
        matches: |e| e.message.starts_with("Package pgfkeys Error: I do not know the key"),
        explain: explain_pgfkeys_unknown_key,
        fix: None,
    },
    &FnRule {
        id: "unknown-key-value-option",
        severity: Severity::Error,
        // `kvsetkeys` is the key-value parser `\hypersetup` (hyperref) and several other packages
        // share, so the message itself never names hyperref — the explanation says so rather than
        // guessing which package's options the author meant.
        matches: |e| e.message.starts_with("Package kvsetkeys Error: Undefined key"),
        explain: explain_unknown_key_value_option,
        fix: None,
    },
    &FnRule {
        id: "babel-unknown-language",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Package babel Error: Unknown option"),
        explain: explain_babel_unknown_language,
        fix: None,
    },
    &FnRule {
        id: "undefined-color",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("Package xcolor Error: Undefined color"),
        explain: explain_undefined_color,
        fix: None,
    },
    &FnRule {
        id: "font-not-found",
        severity: Severity::Error,
        // fontspec is XeTeX/LuaTeX-only, but this project's bundled engine (`preamble-engine`,
        // DESIGN.md §4.1) is XeTeX, so this is a real, reachable failure here, not a hypothetical.
        matches: |e| e.message.starts_with("Package fontspec Error: The font "),
        explain: explain_font_not_found,
        fix: None,
    },
    // S6.1: rules 21–22, DESIGN.md §5.2's "overfull boxes" — see `box_warnings` below for why
    // these need their own scan rather than `located_errors`/`located_warnings`.
    &FnRule {
        id: "overfull-box",
        severity: Severity::Warning,
        matches: |e| e.message.starts_with("Overfull \\hbox"),
        explain: explain_box,
        fix: None,
    },
    &FnRule {
        id: "underfull-box",
        severity: Severity::Warning,
        matches: |e| e.message.starts_with("Underfull \\hbox"),
        explain: explain_box,
        fix: None,
    },
    // S6.1: rules 23–27, general mistakes common enough to be worth a rule on their own — none of
    // them package-specific, and none needing a package this catalog did not already load in an
    // earlier fixture. `misplaced-alignment-tab` on down were common structural mistakes; these are
    // a second pass at the same category, found by testing more real documents against the bundled
    // engine rather than assuming the first pass's list was exhaustive.
    &FnRule {
        id: "command-already-defined",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: Command") && e.message.contains("already defined"),
        explain: explain_command_already_defined,
        fix: None,
    },
    &FnRule {
        id: "missing-item",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: Something's wrong--perhaps a missing \\item"),
        explain: explain_missing_item,
        fix: None,
    },
    &FnRule {
        id: "invalid-column-type",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: Illegal character in array arg"),
        explain: explain_invalid_column_type,
        fix: None,
    },
    &FnRule {
        id: "caption-outside-float",
        severity: Severity::Error,
        matches: |e| e.message.starts_with("LaTeX Error: \\caption outside float"),
        explain: explain_caption_outside_float,
        fix: None,
    },
    &FnRule {
        id: "fragile-command-in-moving-argument",
        severity: Severity::Error,
        // The message names an internal command (`\@sect`, `\@caption`, ...) that the author never
        // typed, not the fragile command that actually caused it — `trailing_command` on the
        // `l.NN` context line is what recovers something the author will recognise, the same
        // technique `explain_unbalanced_braces` already uses for a different message shape.
        matches: |e| e.message.starts_with("Argument of") && e.message.contains("has an extra }"),
        explain: explain_fragile_command_in_moving_argument,
        fix: None,
    },
];

/// Explain everything in a log: the `!` errors, plus the warnings that mean the PDF is wrong.
///
/// This is the only entry point the app should call. `tokenize` and `open_files` each run once
/// and are shared between the error scan and the warning scan below, rather than each repeating
/// its own pass over the log.
pub fn diagnostics(log: &str) -> Vec<Diagnostic> {
    let lines = tokenize(log);
    let stacks = open_files(&lines);
    let mut found: Vec<Diagnostic> = located_errors(&lines, &stacks).iter().map(explain).collect();
    found.extend(located_warnings(&lines, &stacks).iter().map(explain));
    found.extend(box_warnings(&lines, &stacks).iter().map(explain));
    found
}

/// Walks `lines` for `LineKind::Error`, the same shape `crate::quick_errors` scans for on raw
/// text, but resolves each one's `file` against `stacks` (index-aligned with `lines`, from
/// [`open_files`]) and reads the `l.NN` marker off an already-unwrapped logical line instead of a
/// raw physical one — which is what fixes a message like `tikz-unknown-key`'s own capture, whose
/// raw `quick_errors` scan truncated mid-word at the 79-column wrap (SPRINTS.md's S5.4 outcome
/// names this exact fixture as the one waiting on this loop).
fn located_errors(lines: &[LogLine], stacks: &[Vec<String>]) -> Vec<QuickError> {
    let mut errors = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let LineKind::Error(message) = &lines[i].kind else {
            i += 1;
            continue;
        };
        let mut error = QuickError {
            message: message.trim().to_string(),
            line: None,
            context: None,
            file: file_at(stacks, i).map(str::to_string),
        };

        // TeX prints the `l.NN` marker within the next few lines; stop looking at the next `!`
        // or after a generous window, whichever comes first — mirrors `crate::quick_errors`'s
        // own search, just over tokenized lines instead of raw ones.
        let mut looked = 0;
        let mut j = i + 1;
        while j < lines.len() && looked <= 12 {
            if matches!(lines[j].kind, LineKind::Error(_)) {
                break;
            }
            if let LineKind::LineMarker { line, context } = &lines[j].kind {
                error.line = Some(*line);
                let ctx = context.trim();
                if !ctx.is_empty() {
                    error.context = Some(ctx.to_string());
                }
                j += 1;
                break;
            }
            j += 1;
            looked += 1;
        }
        errors.push(error);
        i = j;
    }
    errors
}

/// LaTeX reports undefined `\ref`s and `\cite`s as warnings, not `!` errors, so [`located_errors`]
/// never sees them — but a paper full of bold `??` marks is exactly the kind of thing an author
/// wants told to their face. Deliberately narrow, the same way the scan this replaced was: only
/// `Reference `…undefined` and `Citation `…undefined`, whichever warning banner carried them.
fn located_warnings(lines: &[LogLine], stacks: &[Vec<String>]) -> Vec<QuickError> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            let LineKind::Warning { message } = &line.kind else { return None };
            if !message.contains("undefined") {
                return None;
            }
            if !message.starts_with("Reference ") && !message.starts_with("Citation ") {
                return None;
            }
            Some(QuickError {
                message: message.trim().to_string(),
                line: input_line_number(message),
                context: None,
                file: file_at(stacks, i).map(str::to_string),
            })
        })
        .collect()
}

/// `Overfull \hbox (...)`/`Underfull \hbox (...)` — TeX's own layout warnings, printed with
/// neither a leading `!` nor a `Warning: ` banner (DESIGN.md §5.2 names "overfull boxes" as one
/// of this catalog's rules, but the tokenizer's `LineKind::Warning` can never see one: `classify`
/// only recognises `Warning: ` prefixes, and this is a wholly different, older diagnostic
/// category TeX prints on its own `Text` lines). Only `\hbox`; `\vbox` is real but rarer and not
/// covered here — a fixture-driven addition for whoever hits one, the same way this catalog's
/// other rules were each grown from a real capture rather than guessed ahead of one.
fn box_warnings(lines: &[LogLine], stacks: &[Vec<String>]) -> Vec<QuickError> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            if !matches!(line.kind, LineKind::Text) {
                return None;
            }
            if !line.text.starts_with("Overfull \\hbox") && !line.text.starts_with("Underfull \\hbox") {
                return None;
            }
            Some(QuickError {
                message: line.text.clone(),
                line: box_line_number(&line.text),
                context: None,
                file: file_at(stacks, i).map(str::to_string),
            })
        })
        .collect()
}

/// `... in paragraph at lines 3--4` or `... detected at line 3` — the two shapes a box warning's
/// own line reference takes, captured from two real logs (`overfull-hbox`'s own paragraph case,
/// `underfull-hbox`'s single-line `detected at` case). The *first* number in either shape is
/// where the paragraph or box that triggered the warning starts.
fn box_line_number(text: &str) -> Option<u32> {
    let after = text.split("at line").nth(1)?;
    let digits: String = after.trim_start_matches('s').trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
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
                file: error.file.clone(),
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
        file: error.file.clone(),
        severity: Severity::Error,
        rule: None,
        raw_message: error.message.clone(),
        fix: None,
    }
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

/// The quoted name in a message like `Unknown option 'nosuchlanguage'` — a *matching* pair of
/// straight apostrophes, the shape LaTeX3-based error macros (`babel`, `pgfkeys`) use, distinct
/// from `quoted_name`'s backtick-then-apostrophe TeX convention above. Kept as its own function
/// rather than a shared one parameterised over the two quote characters: with only three real
/// captured callers between them, the two are one obvious line apart, not worth a shared helper.
fn single_quoted_name(text: &str) -> Option<&str> {
    let start = text.find('\'')? + 1;
    let end = text[start..].find('\'')? + start;
    Some(&text[start..end])
}

/// The quoted name in a message like `The font "NoSuchFont" cannot be found` — `fontspec`'s own
/// quoting, a matching pair of double quotes.
fn double_quoted_name(text: &str) -> Option<&str> {
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
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

fn explain_misplaced_alignment_tab(_error: &QuickError) -> (String, String) {
    (
        "`&` used outside a table".to_string(),
        "`&` is reserved for separating columns inside `tabular`, `align` and similar \
         environments; TeX ran into one outside any of those. If you meant a literal ampersand, \
         write `\\&` instead."
            .to_string(),
    )
}

fn explain_extra_alignment_tab(_error: &QuickError) -> (String, String) {
    (
        "More columns than this row declared".to_string(),
        "This row has more `&`-separated entries than the environment's column specification \
         allows, so TeX gave up on the row early rather than guess where it should have ended. \
         Count the `&`s in this row against the `{...}` column spec (or `\\begin{tabular}`'s own \
         argument), and check the row above it for a missing `\\\\` that would make two rows read \
         as one."
            .to_string(),
    )
}

/// `LaTeX Error: Environment nosuchenv undefined.` → `Some("nosuchenv")`.
fn environment_name(message: &str) -> Option<&str> {
    let start = message.find("Environment ")? + "Environment ".len();
    let rest = &message[start..];
    let end = rest.find(" undefined")?;
    Some(&rest[..end])
}

fn explain_undefined_environment(error: &QuickError) -> (String, String) {
    let name = environment_name(&error.message).unwrap_or("that environment");
    (
        format!("`{name}` is not an environment TeX knows"),
        format!(
            "There is no `\\begin{{{name}}}`/`\\end{{{name}}}` pair defined anywhere this \
             document loads. Check the spelling, or check whether it comes from a package this \
             document does not `\\usepackage` yet."
        ),
    )
}

/// `\begin{itemize} on input line 3 ended by \end{enumerate}.` →
/// `Some(("itemize", "enumerate"))`.
fn mismatched_environment_names(message: &str) -> Option<(&str, &str)> {
    let open_start = message.find("\\begin{")? + "\\begin{".len();
    let open_end = message[open_start..].find('}')? + open_start;
    let close_start = message.find("\\end{")? + "\\end{".len();
    let close_end = message[close_start..].find('}')? + close_start;
    Some((&message[open_start..open_end], &message[close_start..close_end]))
}

fn explain_mismatched_environment(error: &QuickError) -> (String, String) {
    match mismatched_environment_names(&error.message) {
        Some((open, close)) => {
            let opened_at = input_line_number(&error.message)
                .map(|n| format!(" on line {n}"))
                .unwrap_or_default();
            (
                format!("`{open}` was closed with `\\end{{{close}}}`"),
                format!(
                    "`\\begin{{{open}}}`{opened_at} was still open when TeX reached \
                     `\\end{{{close}}}`, so the two do not match. Either `\\end{{{open}}}` is \
                     missing before this point, or this `\\end{{{close}}}` should read \
                     `\\end{{{open}}}`."
                ),
            )
        }
        None => (
            "An environment was closed with the wrong name".to_string(),
            "A `\\begin{...}` was still open when TeX reached an `\\end{...}` that did not match \
             it. Check that every environment in between is closed in the order it was opened."
                .to_string(),
        ),
    }
}

fn explain_missing_begin_document(_error: &QuickError) -> (String, String) {
    (
        "Content appears before `\\begin{document}`".to_string(),
        "TeX reached ordinary text or a command that only makes sense inside the document body \
         before it saw `\\begin{document}`. Check that `\\begin{document}` is present and that \
         nothing meant for the body — a stray word, a `\\section`, a package's own output — sits \
         above it in the preamble."
            .to_string(),
    )
}

fn explain_illegal_unit_of_measure(_error: &QuickError) -> (String, String) {
    (
        "A length is missing its unit".to_string(),
        "TeX expected a length here — something like `1pt`, `2cm` or `0.5\\baselineskip` — and \
         found a plain number with no unit, so it assumed `pt` and kept going. Add a unit to the \
         value if `pt` is not what you meant."
            .to_string(),
    )
}

fn explain_missing_number(_error: &QuickError) -> (String, String) {
    (
        "A number was expected but the value is empty".to_string(),
        "TeX expected a number or a length here and found nothing usable, so it used 0. This is \
         usually an empty argument to a command that needs a value, such as `\\vspace{}` or a \
         counter set with no digits after it."
            .to_string(),
    )
}

fn explain_double_script(error: &QuickError) -> (String, String) {
    let (word, symbol) = if error.message.starts_with("Double superscript") {
        ("superscript", "^")
    } else {
        ("subscript", "_")
    };
    (
        format!("Two {word}s on the same base"),
        format!(
            "TeX only allows one `{symbol}` directly on a base, and this line has two — \
             `x{symbol}1{symbol}2` does not say whether the second `{symbol}` attaches to `x` or \
             to `1`. If both parts belong in one {word} together, group them: \
             `x{symbol}{{1,2}}`. If a `{symbol}` was meant as ordinary text rather than a script, \
             wrap it in `\\text{{}}` instead."
        ),
    )
}

fn explain_pgfkeys_unknown_key(error: &QuickError) -> (String, String) {
    let key = single_quoted_name(&error.message).unwrap_or("that key");
    (
        format!("`{key}` is not a tikz/pgf option TeX knows"),
        format!(
            "tikz did not recognise the option `{key}`. Check the spelling against tikz's own \
             documentation, and check whether it comes from a tikz library — \
             `\\usetikzlibrary{{...}}` — this document does not load yet."
        ),
    )
}

fn explain_unknown_key_value_option(error: &QuickError) -> (String, String) {
    let key = quoted_name(&error.message).unwrap_or("that key");
    (
        format!("`{key}` is not an option TeX knows"),
        format!(
            "An option named `{key}` was passed to `\\hypersetup`, or to another package's own \
             key-value configuration, that the package does not recognise — `kvsetkeys` is the \
             shared parser several packages use for their own options, so this message does not \
             say which one. Check the spelling against that package's documentation."
        ),
    )
}

fn explain_babel_unknown_language(error: &QuickError) -> (String, String) {
    let language = single_quoted_name(&error.message).unwrap_or("that language");
    (
        format!("`{language}` is not a language babel knows"),
        format!(
            "babel was asked to load the language `{language}`, and either the name is \
             misspelled or this installation has no language file for it. Check the spelling \
             against babel's own list of supported language names."
        ),
    )
}

fn explain_undefined_color(error: &QuickError) -> (String, String) {
    let name = quoted_name(&error.message).unwrap_or("that colour");
    (
        format!("`{name}` is not a colour xcolor knows"),
        format!(
            "No colour named `{name}` has been defined, so xcolor could not use it. Check the \
             spelling, define it first with `\\definecolor{{{name}}}{{...}}{{...}}`, or load a \
             colour set that already defines it, such as `\\usepackage[dvipsnames]{{xcolor}}`."
        ),
    )
}

fn explain_font_not_found(error: &QuickError) -> (String, String) {
    let name = double_quoted_name(&error.message).unwrap_or("that font");
    (
        format!("`{name}` is not a font this engine can find"),
        format!(
            "fontspec asked for a font named `{name}`, and the engine could not find it under \
             that name. Check the spelling against the font's exact name (not its filename), and \
             make sure it is actually installed for this engine to see rather than only present \
             in another application."
        ),
    )
}

fn explain_box(error: &QuickError) -> (String, String) {
    if error.message.starts_with("Overfull") {
        (
            "A line runs past the margin".to_string(),
            "TeX could not break this line without stretching the spacing too far, so it let the \
             line run past the right margin instead. Often harmless for a single long word — a \
             URL, an identifier — but check the PDF at this line: rephrasing, a manual hyphen \
             (`\\-`), or `\\sloppy` nearby usually fixes it if it is visible."
                .to_string(),
        )
    } else {
        (
            "A line is stretched looser than usual".to_string(),
            "TeX had too little material to fill this line at its normal word spacing, so the \
             words are spread out more than usual to reach the margin. Common in a narrow column \
             or right after a forced line break; check the PDF at this line if the spacing looks \
             odd."
                .to_string(),
        )
    }
}

/// `LaTeX Error: Command \maketitle already defined.` → `Some("\maketitle")`.
fn already_defined_command(message: &str) -> Option<&str> {
    let start = message.find("Command ")? + "Command ".len();
    let rest = &message[start..];
    let end = rest.find(" already defined")?;
    Some(&rest[..end])
}

fn explain_command_already_defined(error: &QuickError) -> (String, String) {
    let name = already_defined_command(&error.message).unwrap_or("This command");
    (
        format!("{name} is already defined"),
        format!(
            "`\\newcommand` only ever defines a name once, and {name} was already defined — by \
             this document class, by a package it loads, or by an earlier `\\newcommand` of your \
             own. Use `\\renewcommand` instead if you meant to change what {name} does, or pick a \
             name that is not already taken."
        ),
    )
}

fn explain_missing_item(_error: &QuickError) -> (String, String) {
    (
        "Content appears before the first \\item".to_string(),
        "Inside `itemize`, `enumerate` and `description`, everything must belong to an `\\item` — \
         TeX found something (text, or a command) before the environment's first `\\item`. Add \
         one before it, or check for a missing `\\item` higher up in the list."
            .to_string(),
    )
}

fn explain_invalid_column_type(_error: &QuickError) -> (String, String) {
    (
        "An unknown column type in a table spec".to_string(),
        "The `{...}` column specification for this `tabular` (or `array`) has a character TeX \
         does not recognise as a column type. The built-in ones are `l`, `c`, `r`, `p{width}` and \
         `|` for a vertical rule; anything else — a typo, or a column type from a package this \
         document does not load — triggers this."
            .to_string(),
    )
}

fn explain_caption_outside_float(_error: &QuickError) -> (String, String) {
    (
        "`\\caption` used outside a figure or table".to_string(),
        "`\\caption` only works inside a float environment such as `figure` or `table` (or \
         another environment built to support it). If this is a custom box or a subfigure, wrap \
         it in `figure`/`table`, or use `\\captionof` from the `caption` package if it genuinely \
         cannot be a float."
            .to_string(),
    )
}

fn explain_fragile_command_in_moving_argument(error: &QuickError) -> (String, String) {
    match trailing_command(error.context.as_ref()) {
        Some(command) => (
            format!("{command} cannot be used here directly"),
            format!(
                "This line is inside a \"moving argument\" — one LaTeX also writes somewhere else, \
                 such as a section title into the table of contents, or a caption into the list of \
                 figures — and {command} does something LaTeX cannot safely redo in that second \
                 place. Either write `\\protect{command}` right before it, or give the outer \
                 command (`\\section`, `\\caption`, ...) a plain-text short version in its optional \
                 argument: `\\section[short title]{{full title with {command}{{...}}}}`."
            ),
        ),
        None => (
            "A command here cannot be used inside this argument".to_string(),
            "This line is inside a \"moving argument\" — one LaTeX also writes somewhere else, \
             such as a section title into the table of contents, or a caption into the list of \
             figures — and something in it does not work safely there. Try `\\protect` right \
             before whichever command is causing this."
                .to_string(),
        ),
    }
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

    /// S6.1: rules 7–15, each against a fixture captured from a real Tectonic 0.17.0 run the same
    /// way as the six rules above — `fixtures/README.md` names what each document does to trigger
    /// its error.
    mod s6_1_captured_from_a_real_engine {
        use super::*;

        #[test]
        fn misplaced_alignment_tab() {
            let log = include_str!("../fixtures/misplaced-alignment-tab/main.log");
            let d = diagnostic_from(log, "misplaced-alignment-tab");
            assert_eq!(d.line, Some(3));
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn extra_alignment_tab() {
            let log = include_str!("../fixtures/extra-alignment-tab/main.log");
            let d = diagnostic_from(log, "extra-alignment-tab");
            assert_eq!(d.line, Some(4));
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn undefined_environment() {
            let log = include_str!("../fixtures/undefined-environment/main.log");
            let d = diagnostic_from(log, "undefined-environment");
            assert_eq!(d.title, "`nosuchenv` is not an environment TeX knows");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn mismatched_environment_names_the_opener_and_the_closer() {
            let log = include_str!("../fixtures/mismatched-environment/main.log");
            let d = diagnostic_from(log, "mismatched-environment");
            assert_eq!(d.title, "`itemize` was closed with `\\end{enumerate}`");
            // The `l.NN` marker points at the mismatched `\end`, not the `\begin` that opened it —
            // the explanation names both, since only one of the two is where the marker landed.
            assert_eq!(d.line, Some(5));
            assert!(d.explanation.contains("line 3"), "{}", d.explanation);
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn missing_begin_document() {
            let log = include_str!("../fixtures/missing-begin-document/main.log");
            let d = diagnostic_from(log, "missing-begin-document");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn illegal_unit_of_measure() {
            let log = include_str!("../fixtures/illegal-unit-of-measure/main.log");
            let d = diagnostic_from(log, "illegal-unit-of-measure");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn missing_number_treated_as_zero() {
            let log = include_str!("../fixtures/missing-number-treated-as-zero/main.log");
            let d = diagnostic_from(log, "missing-number");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn double_subscript_and_superscript_are_told_apart() {
            let subscript = diagnostic_from(
                include_str!("../fixtures/double-subscript/main.log"),
                "double-subscript",
            );
            assert!(subscript.explanation.contains('_'), "{}", subscript.explanation);
            let superscript = diagnostic_from(
                include_str!("../fixtures/double-superscript/main.log"),
                "double-superscript",
            );
            assert!(superscript.explanation.contains('^'), "{}", superscript.explanation);
            assert_ne!(subscript.explanation, superscript.explanation);
        }

        #[test]
        fn pgfkeys_unknown_key_names_the_tikz_option() {
            let log = include_str!("../fixtures/tikz-unknown-key/main.log");
            let d = diagnostic_from(log, "pgfkeys-unknown-key");
            assert_eq!(d.title, "`/tikz/nosuchoption` is not a tikz/pgf option TeX knows");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn unknown_key_value_option_does_not_claim_it_is_hyperrefs_fault_specifically() {
            // The real message comes from `kvsetkeys`, the shared parser `\hypersetup` and other
            // packages' own option handling both use — it never names hyperref itself, so neither
            // should the explanation.
            let log = include_str!("../fixtures/hyperref-unknown-key/main.log");
            let d = diagnostic_from(log, "unknown-key-value-option");
            assert_eq!(d.title, "`nosuchoption` is not an option TeX knows");
            assert!(d.explanation.contains("hypersetup"), "{}", d.explanation);
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn babel_unknown_language() {
            let log = include_str!("../fixtures/babel-unknown-language/main.log");
            let d = diagnostic_from(log, "babel-unknown-language");
            assert_eq!(d.title, "`nosuchlanguage` is not a language babel knows");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn undefined_color() {
            let log = include_str!("../fixtures/undefined-color/main.log");
            let d = diagnostic_from(log, "undefined-color");
            assert_eq!(d.title, "`nosuchcolor` is not a colour xcolor knows");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn font_not_found() {
            let log = include_str!("../fixtures/font-not-found/main.log");
            let d = diagnostic_from(log, "font-not-found");
            assert_eq!(d.title, "`ThisFontDoesNotExistAnywhere` is not a font this engine can find");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn overfull_and_underfull_boxes_are_warnings_not_errors() {
            let overfull = diagnostic_from(include_str!("../fixtures/overfull-hbox/main.log"), "overfull-box");
            assert_eq!(overfull.severity, Severity::Warning);
            assert_eq!(overfull.line, Some(3));
            assert_reads_like_a_sentence(&overfull);

            // This capture holds both directions from one real log: a fixed-width `\hbox` around
            // a single character is underfull itself, and also overfull once TeX tries to typeset
            // the rest of the paragraph around it.
            let log = include_str!("../fixtures/underfull-hbox/main.log");
            let underfull = diagnostic_from(log, "underfull-box");
            assert_eq!(underfull.severity, Severity::Warning);
            assert_eq!(underfull.line, Some(3));
            assert_reads_like_a_sentence(&underfull);
            assert_eq!(diagnostics(log).len(), 2, "{:#?}", diagnostics(log));
        }

        #[test]
        fn command_already_defined_names_the_command() {
            let log = include_str!("../fixtures/command-already-defined/main.log");
            let d = diagnostic_from(log, "command-already-defined");
            assert_eq!(d.title, "\\maketitle is already defined");
            assert!(d.explanation.contains("\\renewcommand"), "{}", d.explanation);
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn missing_item() {
            let log = include_str!("../fixtures/missing-item/main.log");
            let d = diagnostic_from(log, "missing-item");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn invalid_column_type() {
            let log = include_str!("../fixtures/invalid-column-type/main.log");
            let d = diagnostic_from(log, "invalid-column-type");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn caption_outside_float() {
            let log = include_str!("../fixtures/caption-outside-float/main.log");
            let d = diagnostic_from(log, "caption-outside-float");
            assert_reads_like_a_sentence(&d);
        }

        #[test]
        fn fragile_command_in_moving_argument_names_the_footnote_not_the_internal_sect_command() {
            // The real message says `\@sect`, LaTeX's own internal sectioning command — never
            // typed by the author, and not what `trailing_command`'s `l.NN` context read finds.
            let log = include_str!("../fixtures/footnote-in-moving-arg/main.log");
            let d = diagnostic_from(log, "fragile-command-in-moving-argument");
            assert_eq!(d.title, "\\footnote cannot be used here directly");
            assert!(!d.explanation.contains("@sect"), "{}", d.explanation);
            assert!(d.explanation.contains("\\protect\\footnote"), "{}", d.explanation);
            assert_reads_like_a_sentence(&d);
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

    /// S5.6: `diagnostics` now resolves `file` for real, on the fixture built for exactly this —
    /// three files deep, with an unrecognised extensionless wrapper (`outer`) in the middle that
    /// must not stop resolution of the real file nested inside it. `resolver.rs`'s own test
    /// already proved `file_at` gets this right at the `open_files` layer; this proves it survives
    /// all the way through the rule catalog to a `Diagnostic`.
    #[test]
    fn a_diagnostic_three_files_deep_resolves_to_its_own_file_not_a_wrapper_or_the_root() {
        let log = include_str!("../fixtures/nested-include/main.log");
        let d = diagnostic_from(log, "undefined-control-sequence");
        assert_eq!(d.file.as_deref(), Some("chapters/inner"));
    }

    /// The documented limit, not a bug: a diagnostic inside a bare extensionless `\input` resolves
    /// to its *parent* file, because nothing in the log's own text distinguishes `(plainchapter`
    /// from an incidental parenthetical (`resolver.rs`'s own module doc). The real
    /// `bare-input-no-extension` fixture captures the shape but not an error inside it, so this
    /// combines two real captured shapes — a bare extensionless open and a real error message —
    /// into one hand-built log, the same way several rules above already do for a case no single
    /// real capture exercises in full.
    #[test]
    fn a_diagnostic_inside_a_bare_extensionless_input_resolves_to_its_parent() {
        let log = "(main.tex\n(plainchapter\n! Undefined control sequence.\nl.1 \\undefinedcmd\n)\n)\n";
        let d = diagnostic_from(log, "undefined-control-sequence");
        assert_eq!(d.file.as_deref(), Some("main.tex"));
    }

    /// A log with no file ever open at the point a diagnostic fires — the real shape
    /// `emergency-stop`'s fixture captures (every open file closes cleanly before the fatal
    /// error prints) — must not panic and must report `file: None` rather than guess.
    #[test]
    fn a_diagnostic_with_nothing_open_on_the_stack_reports_no_file_rather_than_guessing() {
        let log = include_str!("../fixtures/emergency-stop/main.log");
        let found = diagnostics(log);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, None);
    }
}
