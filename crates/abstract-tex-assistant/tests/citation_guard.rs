//! S12.2: the citation-fabrication guard, attacked (DESIGN.md §5.5, "Hard constraint — not
//! negotiable").
//!
//! Written before the guard was. Every string in the tables below is something a model could
//! plausibly emit, or an adversary could ask it to, and the question for each is the same: would
//! a citation the project's `.bib` does not hold reach the buffer? The tables are the guard's
//! specification; when a new trick is found it is added here first.
//!
//! Three kinds of test, in order of how much they prove:
//!
//! 1. **Attacks** — each must be refused, with the right reason.
//! 2. **Benign edits** — each must pass. A guard that refuses everything is not a guard but a
//!    broken feature, and the assistant's value depends on it saying yes to real rewrites.
//! 3. **Properties** — random attacks and random benign text, and "never panics on anything".

use abstract_tex_assistant::guard::{citation_macros, FindingKind, Guard};
use proptest::prelude::*;

const KNOWN: [&str; 3] = ["smith2020", "lee2019", "garcia-lopez:2015"];

fn guard() -> Guard {
    Guard::new(KNOWN.map(String::from))
}

/// The kinds of finding for an edit that turns nothing into `result`.
fn kinds(result: &str) -> Vec<FindingKind> {
    guard()
        .check("", result)
        .findings
        .iter()
        .map(|finding| finding.kind)
        .collect()
}

fn refused(result: &str) -> bool {
    !guard().check("", result).is_clean()
}

// ---------------------------------------------------------------------------------------------
// 1. Attacks: a key the .bib does not hold
// ---------------------------------------------------------------------------------------------

/// Every cite-family command, in the spellings the real packages use. All carry the key `fake`.
#[test]
fn an_unknown_key_is_refused_in_every_cite_command() {
    let commands = [
        "cite",
        "citep",
        "citet",
        "citealp",
        "citealt",
        "citeauthor",
        "citeyear",
        "citeyearpar",
        "nocite",
        "parencite",
        "textcite",
        "autocite",
        "footcite",
        "smartcite",
        "supercite",
        "fullcite",
        "footfullcite",
        "citetitle",
        "citeurl",
        "volcite",
        "cites",
        "parencites",
        "textcites",
        "autocites",
        "footcites",
        "Cite",
        "Citep",
        "Citet",
        "Citeauthor",
        "Parencite",
        "Textcite",
        "Autocite",
        "CITE",
        "mycite",
        "@cite",
        "@citex",
        "nptextcite",
        "shortcite",
        "notecite",
        "pnotecite",
    ];
    for command in commands {
        for form in [format!("\\{command}{{fake}}"), format!("\\{command}*{{fake}}")] {
            assert_eq!(kinds(&form), vec![FindingKind::UnknownKey], "{form}");
        }
    }
}

#[test]
fn an_unknown_key_hidden_among_known_ones_is_found() {
    for result in [
        r"\citep{smith2020,fake2021}",
        r"\citep{fake2021,smith2020}",
        r"\citep{smith2020, lee2019, fake2021}",
        r"\cites{smith2020}{fake2021}",
        r"\cites{fake2021}{smith2020}",
        r"\parencites(see)(p.~3)[a][b]{smith2020}[c]{fake2021}",
        r"\parencite[see][12]{lee2019,fake2021}",
        r"\citet[p.~4]{fake2021}",
        r"\citet[see \cite{fake2021}]{smith2020}",
        r"\parencites(see \cite{fake2021})(p.~3){smith2020}",
    ] {
        let verdict = guard().check("", result);
        assert_eq!(verdict.unknown_keys(), vec!["fake2021".to_string()], "{result}");
    }
}

/// TeX reads through spaces, newlines and comments between a command and its argument, so the
/// guard must too — `\cite%\n{key}` is `\cite{key}`.
#[test]
fn whitespace_and_comments_do_not_hide_a_key() {
    for result in [
        "\\cite {fake}",
        "\\cite\n{fake}",
        "\\cite\n\n   {fake}",
        "\\cite%\n{fake}",
        "\\cite % a comment\n  {fake}",
        "\\cite{ smith2020 ,\n   fake }",
        "\\cite{smith2020,%\nfake}",
        "\\citep [p.~3]\n[chapter 2] {fake}",
        "\\cite\t{fake}",
    ] {
        assert!(
            guard()
                .check("", result)
                .unknown_keys()
                .iter()
                .any(|k| k.contains("fake")),
            "{result:?}"
        );
    }
}

#[test]
fn a_citation_nested_inside_something_else_is_still_a_citation() {
    for result in [
        r"\footnote{See \cite{fake}.}",
        r"\begin{quote}Text \cite{fake}\end{quote}",
        r"$x \text{ by \cite{fake}}$",
        r"\textbf{\emph{\cite{fake}}}",
        r"\caption{A plot, after \citet{fake}.}",
        r"\section{On \cite{fake}}",
        "\\\\\\cite{fake}", // a line break, then a real \cite
        r"% as \cite{fake} shows",
        r"\verb|\cite{fake}|",
        r"\begin{verbatim}\cite{fake}\end{verbatim}",
    ] {
        assert!(refused(result), "{result}");
    }
}

/// A key that merely *looks* like a known one is not that key: the .bib is compared exactly.
#[test]
fn near_misses_are_not_matches() {
    for result in [
        r"\cite{Smith2020}",          // case
        r"\cite{smith2020 }x",        // fine after trim; the x is outside
        r"\cite{smith}",              // a prefix
        r"\cite{smith2020a}",         // a suffix
        r"\cite{smith2021}",          // a year off
        "\\cite{smith\u{200B}2020}",  // a zero-width space in the middle
        "\\cite{sm\u{0456}th2020}",   // a Cyrillic і for the Latin i
        "\\cite{smith2020\u{00A0}}x", // a no-break space is not a space to TeX
        "\\cite{smith 2020}",         // a space inside
        r"\cite{{smith2020}}",        // an extra brace pair
        r"\cite{smith2020.}",         // trailing punctuation
        r"\cite{smith2020\relax}",
        r"\cite{\detokenize{smith2020}}",
        "\\cite{smith2020\u{FEFF}}",
        "\\cite{\u{FF53}mith2020}", // a full-width s
    ] {
        let verdict = guard().check("", result);
        // Only the ordinary space is trimmed into the real key; every other spelling is refused.
        let trimmed_to_real_key = result == r"\cite{smith2020 }x";
        assert_eq!(verdict.is_clean(), trimmed_to_real_key, "{result:?}");
    }
}

#[test]
fn an_unclosed_group_still_gives_up_its_key() {
    assert!(refused(r"\cite{fake"));
    assert!(refused(r"\cite{smith2020, fake"));
    assert!(refused("\\cite{fake\n\nNew paragraph"));
}

#[test]
fn every_unknown_key_in_one_edit_is_reported_once() {
    let verdict = guard().check(
        "",
        r"\cite{a1} and \cite{b2} and \citep{a1,smith2020} and \cite{a1}",
    );
    assert_eq!(verdict.unknown_keys(), vec!["a1".to_string(), "b2".to_string()]);
}

// ---------------------------------------------------------------------------------------------
// 1b. Attacks: the author's own citation macros
// ---------------------------------------------------------------------------------------------

#[test]
fn a_macro_that_wraps_cite_is_a_citation_command_once_the_guard_knows_it() {
    let preamble = r"
\newcommand{\see}[1]{(see \cite{#1})}
\newcommand{\bold}[1]{\textbf{#1}}
\providecommand\ref@{\ref}
\def\vgl#1{\citep[vgl.][]{#1}}
\DeclareRobustCommand{\src}[1]{\textcite{#1}}
\NewDocumentCommand{\pcite}{m}{\parencite{#1}}
";
    let mut found = citation_macros(preamble);
    found.sort();
    assert_eq!(found, vec!["pcite", "see", "src", "vgl"]);

    let aware = guard().with_citation_commands(found);
    for result in [
        r"\see{fake}",
        r"\vgl{fake}",
        r"\src{fake}",
        r"\see{smith2020,fake}",
    ] {
        assert!(!aware.check("", result).is_clean(), "{result}");
    }
    assert!(aware.check("", r"\see{smith2020} and \bold{x}").is_clean());
    // Without that knowledge a wrapper is invisible — which is why the app passes what it finds.
    assert!(guard().check("", r"\see{fake}").is_clean());
}

// ---------------------------------------------------------------------------------------------
// 1c. Attacks: ways round the scan
// ---------------------------------------------------------------------------------------------

#[test]
fn defining_or_obscuring_a_command_is_refused() {
    for result in [
        r"\def\c{\cite}\c{fake}",
        r"\gdef\c#1{\cite{#1}}",
        r"\edef\c{\noexpand\cite}",
        r"\xdef\c{x}",
        r"\let\c\cite \c{fake}",
        r"\futurelet\c\cite",
        r"\newcommand{\c}[1]{\cite{#1}}",
        r"\renewcommand{\emph}[1]{\cite{#1}}",
        r"\providecommand\c{x}",
        r"\DeclareRobustCommand{\c}{\cite}",
        r"\NewDocumentCommand{\c}{m}{\cite{#1}}",
        r"\RenewDocumentCommand{\c}{m}{x}",
        r"\csname cite\endcsname{fake}",
        r"\csname ci\endcsname",
        r"\catcode`\@=11 \@cite",
        r"\expandafter\cite\expandafter{fake}",
        r"\scantokens{\cite{fake}}",
        r"\AtBeginDocument{\cite{fake}}",
        r"\AtEndDocument{x}",
        r"\input{other-chapter}",
        r"\include{other-chapter}",
        r"\InputIfFileExists{notes}{}{}",
        r"\immediate\write18{echo}",
        r"\directlua{tex.print('\\cite{fake}')}",
        r"\newenvironment{c}{\cite}{}",
    ] {
        assert!(
            kinds(result).contains(&FindingKind::HiddenCommand),
            "{result}: {:?}",
            kinds(result)
        );
    }
}

/// `^^5c` is a backslash to TeX, so `^^5ccite{fake}` is `\cite{fake}` and no scan for a literal
/// backslash sees it.
#[test]
fn tex_character_notation_is_refused() {
    for result in [
        "^^5ccite{fake}",
        "^^5Ccite{fake}",
        "\\cite{^^66ake}",
        "text ^^M more",
        "^^7f",
    ] {
        assert!(kinds(result).contains(&FindingKind::HiddenCommand), "{result}");
    }
    // A single caret, a power and a chain of them are maths.
    for result in ["$x^2$", "$x^{2}$", "a ^ b", "$x^y^z$"] {
        assert!(guard().check("", result).is_clean(), "{result}");
    }
}

// ---------------------------------------------------------------------------------------------
// 1d. Attacks: a reference list of its own
// ---------------------------------------------------------------------------------------------

#[test]
fn a_reference_list_written_into_the_text_is_refused() {
    for result in [
        r"\bibitem{x} A. Author, A Book, 2020.",
        r"\begin{thebibliography}{9}\bibitem{x} Y.\end{thebibliography}",
        "\\begin{filecontents}{extra.bib}\n@book{fake, title={X}}\n\\end{filecontents}",
        "\\begin{filecontents*}{extra.bib}\n@book{fake, title={X}}\n\\end{filecontents*}",
        r"\addbibresource{extra.bib}",
        r"\addglobalbib{extra.bib}",
        r"\addsectionbib{extra.bib}",
        r"\bibliography{extra}",
    ] {
        assert!(kinds(result).contains(&FindingKind::OwnReferenceList), "{result}");
    }
}

// ---------------------------------------------------------------------------------------------
// 1e. Attacks: a citation with no \cite at all
// ---------------------------------------------------------------------------------------------

#[test]
fn a_reference_written_in_plain_text_is_refused() {
    for result in [
        "as shown before (Smith et al., 2019).",
        "as shown before (Smith and Jones, 2018).",
        "as shown before (Smith & Jones 2018).",
        "as shown before (see Smith, 2017, p. 4).",
        "as shown before (García-López, 2015a).",
        "as shown before (Müller, 2020).",
        "as shown before (Zhang et al. 2021; Lee 2019).",
        "Smith et al. (2019) showed that",
        "Smith et al., 2019, showed that",
        "as in [Smith et al., 2019]",
        "(O'Brien, 2012)",
        "(Nakamura 1999b)",
    ] {
        assert!(
            kinds(result).contains(&FindingKind::PlainTextCitation),
            "{result}: {:?}",
            kinds(result)
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. Benign edits: the guard must say yes
// ---------------------------------------------------------------------------------------------

#[test]
fn citations_of_entries_the_bib_holds_pass() {
    for result in [
        r"\cite{smith2020}",
        r"\citep[p.~3]{smith2020,lee2019}",
        r"\cites{smith2020}{lee2019}",
        r"\Citep{lee2019}",
        r"\parencites(see)(p.~3)[a][b]{smith2020}[c]{lee2019}",
        r"\textcite[Ch.~2]{garcia-lopez:2015} argues",
        "\\cite{ smith2020 ,\n lee2019 }",
        r"\nocite{*}",
        "\\cite{smith2020,% the second one\n lee2019}", // a comment inside the key list is TeX's to drop
        r"\cite*{smith2020}",
        r"\footnote{See \cite{smith2020}.}",
        "As \\citet{smith2020} and \\citet{lee2019} both note, the rate fell.",
    ] {
        assert!(guard().check("", result).is_clean(), "{result}");
    }
}

#[test]
fn ordinary_prose_and_markup_pass() {
    for result in [
        "",
        "We cite this result without proof.",
        r"\section{Introduction}\label{sec:intro} See Section~\ref{sec:method}.",
        r"\label{cite:figure}\ref{fake}",
        r"The \emph{rate} fell by 4\% in 2019, and again in 2020.",
        "In 1998 (see below) the company was founded.",
        "The study ran for six months (January 2020 to June 2020).",
        "(Figure 3, 2019)",
        "(Table 2 and Section 4, 2021)",
        "(in March 2019)",
        "$a \\\\ b$ and $x_1^2 + y^{3}$",
        "a\\\\cite{x}",
        r"\textit{cite} is a word",
        "Zażółć gęślą jaźń — 日本語のテキスト — Ünïcödé.",
        r"\begin{equation} E = mc^2 \end{equation}",
        "Dr. Smith (2019) said so", // narrative form with no "et al.": left to the author
    ] {
        let verdict = guard().check("", result);
        assert!(verdict.is_clean(), "{result:?}: {:?}", verdict.findings);
    }
}

/// An unknown key the author already had is theirs, not the model's: tightening a paragraph that
/// contains a typo'd `\cite` must not be refused for it — and must not be refused for keeping it.
#[test]
fn what_the_original_already_contained_is_not_introduced_by_the_edit() {
    let original = r"Prior work \cite{typo2020} (Smith et al., 2019) is clear. \input{intro} \def\x{y}";
    let reworded =
        r"Earlier work \cite{typo2020}, as in (Smith et al., 2019), is plain. \input{intro} \def\x{y}";
    assert!(guard().check(original, reworded).is_clean());

    // But a second, different unknown key is the model's.
    let extra = r"Earlier work \cite{typo2020,typo2021} is plain.";
    assert_eq!(
        guard().check(original, extra).unknown_keys(),
        vec!["typo2021".to_string()]
    );

    // And a repeated old key is still old.
    assert!(guard()
        .check(original, r"\cite{typo2020} \cite{typo2020}")
        .is_clean());
}

#[test]
fn removing_citations_is_always_allowed() {
    let original = r"\cite{fake} and \cite{smith2020} and \bibitem{x} and \def\a{b}";
    assert!(guard().check(original, "nothing is cited any more").is_clean());
}

/// A construct that was already in the text is not new just because the edit kept it; one *more*
/// of it is.
#[test]
fn a_construct_counts_as_new_only_when_there_are_more_of_it() {
    assert!(guard().check(r"\input{a}", r"\input{a} reworded").is_clean());
    assert!(!guard().check(r"\input{a}", r"\input{a} \input{b}").is_clean());
}

// ---------------------------------------------------------------------------------------------
// Hunks
// ---------------------------------------------------------------------------------------------

/// S12.3 decides per hunk; the guard says which byte ranges of the result are not allowed.
#[test]
fn a_finding_blocks_exactly_the_hunks_that_touch_it() {
    let result = "Intro here. \\cite{fake} Outro there.";
    let verdict = guard().check("", result);
    let start = result.find("\\cite").unwrap();
    let end = start + "\\cite{fake}".len();

    assert!(verdict.blocks(&(start..end)));
    assert!(
        verdict.blocks(&(start + 3..start + 4)),
        "a hunk inside the command"
    );
    assert!(verdict.blocks(&(0..start + 1)), "a hunk that ends inside it");
    assert!(
        verdict.blocks(&(end - 1..result.len())),
        "a hunk that begins inside it"
    );
    assert!(!verdict.blocks(&(0..start)), "the text before");
    assert!(!verdict.blocks(&(end..result.len())), "the text after");
    assert!(!verdict.blocks(&(0..0)), "an empty range touches nothing");
}

/// A hunk that only inserts the key between an unchanged `\cite{` and `}` must still be caught:
/// the guard looks at the whole resulting text, not at the inserted fragment.
#[test]
fn a_key_inserted_between_unchanged_braces_is_caught_in_context() {
    let original = r"See \cite{} for details.";
    let result = r"See \cite{fake} for details.";
    let verdict = guard().check(original, result);
    let inserted_at = result.find("fake").unwrap();
    assert!(verdict.blocks(&(inserted_at..inserted_at + 4)));
}

#[test]
fn findings_come_in_text_order_with_a_sentence_each() {
    let verdict = guard().check("", r"\cite{b} \bibitem{x} \cite{a}");
    let positions: Vec<usize> = verdict
        .findings
        .iter()
        .map(|finding| finding.span.start)
        .collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted);
    for finding in &verdict.findings {
        let sentence = finding.sentence();
        assert!(sentence.ends_with('.') && sentence.len() > 20, "{sentence}");
    }
    let unknown = verdict
        .findings
        .iter()
        .find(|f| f.kind == FindingKind::UnknownKey)
        .unwrap();
    assert!(unknown.sentence().contains('b') && unknown.sentence().contains(".bib"));
}

// ---------------------------------------------------------------------------------------------
// 3. Properties
// ---------------------------------------------------------------------------------------------

fn command_name() -> impl Strategy<Value = String> {
    let names = prop::sample::select(vec![
        "cite",
        "citep",
        "citet",
        "citeauthor",
        "parencite",
        "textcite",
        "autocite",
        "footcite",
        "Cite",
        "Citep",
        "Parencite",
        "nocite",
        "cites",
        "supercite",
        "fullcite",
        "volcite",
    ]);
    // Flip the case of each letter at random: the predicate is case-insensitive.
    (names, prop::collection::vec(any::<bool>(), 12)).prop_map(|(name, flips)| {
        name.chars()
            .zip(flips.into_iter().cycle())
            .map(|(c, flip)| if flip { c.to_ascii_uppercase() } else { c })
            .collect()
    })
}

fn between_command_and_argument() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "",
        " ",
        "\n",
        "  \n  ",
        "%c\n",
        " % note\n ",
        "\t",
        "[p.~3]",
        "[see][p.~3]",
        "*",
        "(see)",
        "(a)(b)[c]",
    ])
    .prop_map(String::from)
}

fn unknown_key() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9:_-]{0,11}".prop_filter("not a real key", |key| !KNOWN.contains(&key.as_str()))
}

fn surrounding_text() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "",
        "Some prose. ",
        "\\textbf{",
        "$x$ and ",
        "\n\n",
        "\\footnote{",
        "% ",
        "(see ",
    ])
    .prop_map(String::from)
}

proptest! {
    /// Whatever the command's case, whatever sits between it and its argument, whatever surrounds
    /// it, and whether the unknown key is alone or among known ones, it is reported.
    #[test]
    fn an_unknown_key_is_always_found(
        name in command_name(),
        gap in between_command_and_argument(),
        key in unknown_key(),
        known_before in any::<bool>(),
        before in surrounding_text(),
    ) {
        let list = if known_before { format!("smith2020,{key}") } else { key.clone() };
        let result = format!("{before}\\{name}{gap}{{{list}}} after");
        let verdict = guard().check("", &result);
        prop_assert!(
            verdict.unknown_keys().contains(&key),
            "{:?} -> {:?}", result, verdict.findings
        );
    }

    /// Only known keys, in any arrangement the scan understands: never refused.
    #[test]
    fn a_known_key_is_never_refused(
        name in command_name(),
        gap in prop::sample::select(vec!["", " ", "\n", "%c\n", "[p.~3]", "[see][p.~3]", "*"]),
        picks in prop::collection::vec(0usize..3, 1..4),
    ) {
        let list: Vec<&str> = picks.iter().map(|&i| KNOWN[i]).collect();
        let result = format!("Text \\{name}{gap}{{{}}} more.", list.join(", "));
        let verdict = guard().check("", &result);
        prop_assert!(verdict.is_clean(), "{:?} -> {:?}", result, verdict.findings);
    }

    /// Nothing the guard is given — arbitrary Unicode, stray backslashes, half-open groups — may
    /// panic it, and every span it returns is a valid range in the text.
    #[test]
    fn it_never_panics_and_its_spans_are_always_in_range(original in ".{0,200}", result in "\\PC{0,200}") {
        let verdict = guard().check(&original, &result);
        for finding in &verdict.findings {
            prop_assert!(finding.span.start <= finding.span.end);
            prop_assert!(finding.span.end <= result.len());
            prop_assert!(result.is_char_boundary(finding.span.start) && result.is_char_boundary(finding.span.end));
        }
    }

    /// An edit that adds nothing — the same text back — is never refused, whatever the text.
    #[test]
    fn handing_back_the_same_text_is_always_clean(text in "\\PC{0,200}") {
        prop_assert!(guard().check(&text, &text).is_clean());
    }
}
