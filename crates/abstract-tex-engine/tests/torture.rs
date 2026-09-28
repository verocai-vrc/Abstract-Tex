//! The twenty-error torture document: the v0.3 exit criterion (DESIGN.md §7, S6.4).
//!
//! `fixtures/torture/` is one document with twenty chapter files, each holding exactly one
//! deliberate mistake on a known line. This test walks the document the way an author would:
//! build, meet the first error, fix it, build again — twenty-one builds in all, the last one
//! clean. It exists because this engine halts at the first `!` error (see
//! `crates/texlog/fixtures/README.md`), so "twenty errors" can never mean twenty in one log;
//! it means twenty in sequence, and every one of them must resolve to its own file and line
//! with a plain-language explanation.
//!
//! "Fixing" a chapter here means replacing it with a one-line comment, not applying the real
//! correction — the point is that the *next* mistake is then the first live one, exactly as it
//! would be after a real fix, and that the chapters before it still take part in the build.
//!
//! Two tests share one checker. `the_recorded_walk_still_resolves_every_mistake` reads the
//! twenty-one logs committed under `fixtures/torture/captures/` and runs every time — it is the
//! recorded exit demo, and it keeps the rule catalog honest against this document on every
//! `pnpm verify`. `the_real_engine_walk_resolves_every_mistake` repeats the walk with the real
//! Tectonic (ignored by default, like the engine's own real-build test), and re-records the
//! captures when `ABSTRACT_TEX_RECORD_TORTURE` is set, so the two can never quietly drift apart.
//!
//! This file must never assert on TeX's own wording: the drawer never shows it, so neither
//! should the exit criterion depend on it.

use std::fs;
use std::path::{Path, PathBuf};

use abstract_tex_engine::tectonic::Tectonic;
use abstract_tex_engine::{BuildJob, Engine, EngineError};
use texlog::{Diagnostic, Severity};
use tokio_util::sync::CancellationToken;

/// One deliberate mistake: which chapter it is in, the line the drawer must name, the rule
/// that must explain it, and the one-click fix it must offer, if the catalog has one.
struct Mistake {
    chapter: &'static str,
    line: u32,
    rule: &'static str,
    severity: Severity,
    fix: Option<&'static str>,
}

/// The twenty, in the order `main.tex` inputs them. The `line` is the one TeX names, which
/// for most rules is the mistake's own line; `missing-item` is the exception the rule's own
/// explanation accounts for (TeX only notices at the `\end{itemize}` that no `\item` came).
const MISTAKES: [Mistake; 20] = [
    Mistake { chapter: "01-undefined-control-sequence", line: 3, rule: "undefined-control-sequence", severity: Severity::Error, fix: None },
    Mistake { chapter: "02-missing-dollar", line: 3, rule: "missing-dollar", severity: Severity::Error, fix: Some("Escape as \\_") },
    Mistake { chapter: "03-unbalanced-braces", line: 3, rule: "unbalanced-braces", severity: Severity::Error, fix: None },
    Mistake { chapter: "04-undefined-reference", line: 3, rule: "undefined-reference", severity: Severity::Warning, fix: None },
    Mistake { chapter: "05-undefined-citation", line: 3, rule: "undefined-citation", severity: Severity::Warning, fix: None },
    Mistake { chapter: "06-misplaced-alignment-tab", line: 3, rule: "misplaced-alignment-tab", severity: Severity::Error, fix: Some("Escape as \\&") },
    Mistake { chapter: "07-extra-alignment-tab", line: 7, rule: "extra-alignment-tab", severity: Severity::Error, fix: None },
    Mistake { chapter: "08-undefined-environment", line: 3, rule: "undefined-environment", severity: Severity::Error, fix: None },
    Mistake { chapter: "09-mismatched-environment", line: 6, rule: "mismatched-environment", severity: Severity::Error, fix: None },
    Mistake { chapter: "10-missing-item", line: 5, rule: "missing-item", severity: Severity::Error, fix: None },
    Mistake { chapter: "11-illegal-unit-of-measure", line: 4, rule: "illegal-unit-of-measure", severity: Severity::Error, fix: Some("Add pt") },
    // No fix, and the walk is what showed why: the title is longer than TeX's `half_error_line`
    // (50), so the printed context is `...ote{Which never works without protection.}}` and the
    // `\footnote` the fix would need is cut off. Logged in `bugs-issues-fixes.md` (Open, S6.4).
    Mistake { chapter: "12-fragile-command-in-moving-argument", line: 1, rule: "fragile-command-in-moving-argument", severity: Severity::Error, fix: None },
    Mistake { chapter: "13-caption-outside-float", line: 3, rule: "caption-outside-float", severity: Severity::Error, fix: None },
    Mistake { chapter: "14-display-math-wrong-delimiter", line: 4, rule: "display-math-wrong-delimiter", severity: Severity::Error, fix: Some("Close with \\]") },
    Mistake { chapter: "15-verb-unterminated", line: 3, rule: "verb-unterminated", severity: Severity::Error, fix: Some("Close with |") },
    Mistake { chapter: "16-undefined-color", line: 3, rule: "undefined-color", severity: Severity::Error, fix: None },
    Mistake { chapter: "17-image-not-found", line: 5, rule: "image-not-found", severity: Severity::Error, fix: None },
    Mistake { chapter: "18-file-not-found", line: 3, rule: "file-not-found", severity: Severity::Error, fix: None },
    Mistake { chapter: "19-preamble-only-command", line: 3, rule: "preamble-only-command", severity: Severity::Error, fix: None },
    Mistake { chapter: "20-include-cannot-be-nested", line: 3, rule: "include-cannot-be-nested", severity: Severity::Error, fix: Some("Change to \\input") },
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn torture_dir() -> PathBuf {
    repo_root().join("fixtures/torture")
}

/// The log for the build in which mistakes `1..step` are fixed and mistake `step` is the
/// first live one; `step == 20` is the clean build after all twenty are gone.
fn capture_path(step: usize) -> PathBuf {
    let name = if step == MISTAKES.len() { "clean".to_string() } else { format!("{:02}", step + 1) };
    torture_dir().join("captures").join(format!("{name}.log"))
}

/// The exit criterion, for one build: mistake `step` resolves to its chapter and line, is
/// explained by its rule in plain language, and offers its fix if it has one. Everything
/// before `step` is fixed by now, so nothing may still be reported from those chapters.
fn check_step(step: usize, diagnostics: &[Diagnostic]) {
    let mistake = &MISTAKES[step];
    let file = format!("sections/{}.tex", mistake.chapter);

    let found = diagnostics
        .iter()
        .find(|d| is_chapter(d, mistake.chapter) && d.line == Some(mistake.line))
        .unwrap_or_else(|| {
            panic!(
                "step {}: nothing resolved to {file}:{}; the log produced:\n{:#?}",
                step + 1,
                mistake.line,
                diagnostics
            )
        });

    assert_eq!(found.rule, Some(mistake.rule), "step {}: wrong rule for {file}", step + 1);
    assert_eq!(found.severity, mistake.severity, "step {}: wrong severity for {file}", step + 1);
    assert_eq!(
        found.fix.as_ref().map(|f| f.description.as_str()),
        mistake.fix,
        "step {}: wrong fix offered for {file}",
        step + 1
    );
    // "Plain-language explanation": whole sentences of our own, never TeX's message echoed.
    assert!(found.explanation.ends_with('.'), "step {}: explanation is not a sentence", step + 1);
    assert!(
        !found.explanation.contains(&found.raw_message),
        "step {}: explanation just repeats TeX's own words",
        step + 1
    );

    for earlier in &MISTAKES[..step] {
        assert!(
            !diagnostics.iter().any(|d| is_chapter(d, earlier.chapter)),
            "step {}: {} was fixed but is still reported",
            step + 1,
            earlier.chapter
        );
    }
}

/// Whether a diagnostic's resolved `file` is this chapter. Two spellings are correct, because
/// `Diagnostic.file` is the name as TeX printed it: `\input{sections/foo}` — the spelling this
/// document uses — is echoed with no `.tex`, while `\include` (chapter 20) opens `sections/foo.tex`
/// and says so. `texlog` never reads the file tree, so it cannot complete the first form; the
/// drawer's `diagnosticTarget` does, against the include graph, and `drawer.test.ts` pins that.
/// The walk's own first step is what found the gap (`bugs-issues-fixes.md`, S6.4).
fn is_chapter(diagnostic: &Diagnostic, chapter: &str) -> bool {
    let bare = format!("sections/{chapter}");
    let with_extension = format!("{bare}.tex");
    matches!(diagnostic.file.as_deref(), Some(file) if file == bare || file == with_extension)
}

/// Runs every build, on real logs recorded by the ignored test below. No engine needed.
#[test]
fn the_recorded_walk_still_resolves_every_mistake() {
    for step in 0..MISTAKES.len() {
        let log = fs::read_to_string(capture_path(step)).unwrap_or_else(|e| {
            panic!("step {}: no capture ({e}); run the ignored test with ABSTRACT_TEX_RECORD_TORTURE=1", step + 1)
        });
        check_step(step, &texlog::diagnostics(&log));
    }

    let clean = fs::read_to_string(capture_path(MISTAKES.len())).unwrap();
    let leftovers = texlog::diagnostics(&clean);
    assert!(leftovers.is_empty(), "clean build still reports:\n{leftovers:#?}");
}

/// The same walk with the real engine: copy the project to a temporary directory, and for
/// each step comment out every chapter before it so the next mistake is the first one TeX
/// meets. Ignored by default for the same reason as the engine's own real-build test: it
/// needs the fetched sidecar and, on a cold cache, the network.
#[tokio::test]
#[ignore]
async fn the_real_engine_walk_resolves_every_mistake() {
    let engine = match Tectonic::locate() {
        Ok(engine) => engine,
        Err(EngineError::NotFound) => {
            let found = abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root())
                .expect("run `pnpm fetch-engine` first");
            Tectonic::at(found)
        }
        Err(e) => panic!("{e}"),
    };
    let record = std::env::var_os("ABSTRACT_TEX_RECORD_TORTURE").is_some();

    let tmp = tempfile::tempdir().unwrap();
    copy_project(&torture_dir(), tmp.path());
    let job = BuildJob {
        project_dir: tmp.path().to_path_buf(),
        root_file: PathBuf::from("main.tex"),
        out_dir: tmp.path().join(".abstract-tex/build"),
        synctex: false,
    };

    // The clean build is the extra twenty-first step, once the last chapter is fixed too.
    for step in 0..=MISTAKES.len() {
        if step > 0 {
            let fixed = &MISTAKES[step - 1];
            let chapter = tmp.path().join("sections").join(format!("{}.tex", fixed.chapter));
            fs::write(chapter, "% Fixed in an earlier step of the torture walk.\n").unwrap();
        }

        let outcome = engine.build(&job, CancellationToken::new(), None).await.unwrap();
        let log_path = outcome.log.clone().unwrap_or_else(|| panic!("step {}: no log kept", step + 1));
        let log = fs::read_to_string(&log_path).unwrap();
        if record {
            fs::create_dir_all(torture_dir().join("captures")).unwrap();
            fs::write(capture_path(step), &log).unwrap();
        }

        if step < MISTAKES.len() {
            check_step(step, &texlog::diagnostics(&log));
        } else {
            assert!(outcome.success, "clean build failed:\n{}", outcome.stderr);
            assert!(outcome.pdf.is_some(), "clean build left no PDF");
        }
    }
}

/// Copies `main.tex`, `preamble.tex` and `sections/` — the document, not the captures.
fn copy_project(from: &Path, to: &Path) {
    for name in ["main.tex", "preamble.tex"] {
        fs::copy(from.join(name), to.join(name)).unwrap();
    }
    let sections = to.join("sections");
    fs::create_dir_all(&sections).unwrap();
    for entry in fs::read_dir(from.join("sections")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), sections.join(entry.file_name())).unwrap();
    }
}
