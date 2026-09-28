//! The golden corpus (DESIGN.md §8, S9.1): eight real-shaped documents under `fixtures/corpus/`
//! that must keep compiling after every change, and one deliberately broken document that must
//! keep producing exactly the diagnostics it produced before.
//!
//! Two tests, the same split as `torture.rs`. `the_recorded_broken_log_still_gives_its_diagnostics`
//! reads a log committed under `fixtures/corpus/broken/captures/` and runs on every `cargo test`,
//! so a rule-catalog change that alters what the broken document reports is caught with no
//! engine at all. `every_corpus_document_builds_as_recorded` builds all eight with the real
//! Tectonic (ignored by default: it needs the fetched sidecar and, on a cold cache, the network)
//! and re-records the capture and `expected.json` when `ABSTRACT_TEX_RECORD_CORPUS` is set.
//!
//! "Exactly the diagnostics" means rule, file, line and severity — never the explanation's
//! wording, which a rule is free to improve, and never TeX's own words, which the drawer never
//! shows. Timing is printed, not asserted: measuring it properly is S9.2's harness.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use abstract_tex_engine::tectonic::Tectonic;
use abstract_tex_engine::{BuildJob, Engine, EngineError};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

/// What a corpus document is expected to do on the bundled engine.
enum Expect {
    /// Builds, and the PDF has at least this many pages — the thesis's sixty is part of what the
    /// exit criterion times, so a thesis that quietly shrank would make the gate meaningless.
    Builds { min_pages: u32 },
    /// Fails, with the diagnostics in its `expected.json`.
    Broken,
    /// Cannot build on the bundled engine, for the reason given; built anyway, and required to
    /// fail, so the day it starts working (S9.4's engine switching) this test says so.
    Unsupported { reason: &'static str },
}

const CORPUS: [(&str, Expect); 8] = [
    ("conference", Expect::Builds { min_pages: 2 }),
    ("thesis", Expect::Builds { min_pages: 60 }),
    ("beamer", Expect::Builds { min_pages: 10 }),
    ("tikz-figures", Expect::Builds { min_pages: 2 }),
    ("minted", Expect::Unsupported { reason: "minted needs shell-escape and Pygments; the bundled engine runs without shell-escape" }),
    ("non-latin", Expect::Builds { min_pages: 1 }),
    ("broken", Expect::Broken),
    ("pathological-preamble", Expect::Builds { min_pages: 1 }),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_dir() -> PathBuf {
    repo_root().join("fixtures/corpus")
}

fn broken_capture() -> PathBuf {
    corpus_dir().join("broken/captures/main.log")
}

/// The part of each diagnostic that must not change: which rule, where, how severe.
fn signature(log: &str) -> Value {
    let diagnostics = texlog::diagnostics(log);
    Value::Array(
        diagnostics
            .iter()
            .map(|d| json!({ "rule": d.rule, "file": d.file, "line": d.line, "severity": d.severity }))
            .collect(),
    )
}

fn expected_broken() -> Value {
    let text = fs::read_to_string(corpus_dir().join("broken/expected.json"))
        .expect("no expected.json; run the ignored test with ABSTRACT_TEX_RECORD_CORPUS=1");
    serde_json::from_str(&text).unwrap()
}

/// How many pages the engine says it wrote, from the log's closing `Output written on …` line
/// (`(1 page, …)` or `(62 pages, …)`).
fn pages_written(log: &str) -> Option<u32> {
    let after = log.split("Output written on ").nth(1)?;
    let open = after.find('(')?;
    let digits: String = after[open + 1..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[test]
fn the_recorded_broken_log_still_gives_its_diagnostics() {
    let log = fs::read_to_string(broken_capture())
        .expect("no capture; run the ignored test with ABSTRACT_TEX_RECORD_CORPUS=1");
    assert_eq!(signature(&log), expected_broken());
}

#[test]
fn pages_written_reads_both_spellings() {
    assert_eq!(pages_written("Output written on main.xdv (1 page, 26980 bytes)."), Some(1));
    assert_eq!(pages_written("Output written on main.xdv (62 pages, 1 bytes)."), Some(62));
    assert_eq!(pages_written("No pages of output."), None);
}

#[tokio::test]
#[ignore]
async fn every_corpus_document_builds_as_recorded() {
    let engine = match Tectonic::locate() {
        Ok(engine) => engine,
        Err(EngineError::NotFound) => {
            let found = abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root())
                .expect("run `pnpm fetch-engine` first");
            Tectonic::at(found)
        }
        Err(e) => panic!("{e}"),
    };
    let record = std::env::var_os("ABSTRACT_TEX_RECORD_CORPUS").is_some();

    for (name, expect) in &CORPUS {
        // A copy, so build output never lands in the repository's fixtures.
        let tmp = tempfile::tempdir().unwrap();
        copy_dir(&corpus_dir().join(name), tmp.path());
        let job = BuildJob {
            project_dir: tmp.path().to_path_buf(),
            root_file: PathBuf::from("main.tex"),
            out_dir: tmp.path().join(".abstract-tex/build"),
            synctex: true,
        };

        let started = Instant::now();
        let outcome = engine.build(&job, CancellationToken::new(), None).await.unwrap();
        let log = outcome.log.as_ref().and_then(|path| fs::read_to_string(path).ok()).unwrap_or_default();
        println!("{name}: success={} in {:.1?}, {:?} pages", outcome.success, started.elapsed(), pages_written(&log));

        match expect {
            Expect::Builds { min_pages } => {
                assert!(outcome.success, "{name} failed to build:\n{}", outcome.stderr);
                let pages = pages_written(&log).unwrap_or(0);
                assert!(pages >= *min_pages, "{name}: {pages} pages, expected at least {min_pages}");
            }
            Expect::Broken => {
                assert!(!outcome.success, "{name} is meant to fail");
                if record {
                    fs::create_dir_all(broken_capture().parent().unwrap()).unwrap();
                    fs::write(broken_capture(), &log).unwrap();
                    let pretty = serde_json::to_string_pretty(&signature(&log)).unwrap();
                    fs::write(corpus_dir().join("broken/expected.json"), pretty + "\n").unwrap();
                }
                assert_eq!(signature(&log), expected_broken(), "{name}: diagnostics changed");
            }
            Expect::Unsupported { reason } => {
                assert!(!outcome.success, "{name} now builds — its reason ({reason}) no longer holds; update the corpus");
            }
        }
    }
}

/// The S9.2 harness: for every document that builds, one cold build and then `runs` warm builds
/// after an edit, timed through the same `Engine::build` the app calls, with the steps each one
/// took. Two kinds of edit: a comment appended to `main.tex` (moves nothing: the floor of a warm
/// build), and — for the thesis, the document the exit criterion names — a sentence added to a
/// chapter, which reflows pages the way real writing does. Writes `target/corpus-report.json`
/// and prints a table; asserts nothing, because the gate is S9.5's job once the numbers are known.
#[tokio::test]
#[ignore]
async fn warm_build_timings() {
    let engine = Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root()).expect("run `pnpm fetch-engine`"));
    let runs: usize = std::env::var("ABSTRACT_TEX_BENCH_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(3);
    let mut report = Vec::new();

    for (name, expect) in &CORPUS {
        if !matches!(expect, Expect::Builds { .. }) {
            continue;
        }
        let tmp = tempfile::tempdir().unwrap();
        copy_dir(&corpus_dir().join(name), tmp.path());
        let job = BuildJob {
            project_dir: tmp.path().to_path_buf(),
            root_file: PathBuf::from("main.tex"),
            out_dir: tmp.path().join(".abstract-tex/build"),
            synctex: true,
        };

        let cold = engine.build(&job, CancellationToken::new(), None).await.unwrap();
        assert!(cold.success, "{name}: cold build failed");
        let mut edits: Vec<(&str, PathBuf, &str)> = vec![("comment", tmp.path().join("main.tex"), "\n% warm edit\n")];
        if *name == "thesis" {
            edits.push(("prose", tmp.path().join("chapters/03-method.tex"), "\nOne more sentence of method, written between builds.\n"));
        }
        for (kind, file, addition) in edits {
            let mut millis = Vec::new();
            let mut steps = Vec::new();
            for _ in 0..runs {
                let mut text = fs::read_to_string(&file).unwrap();
                text.push_str(addition);
                fs::write(&file, text).unwrap();
                let warm = engine.build(&job, CancellationToken::new(), None).await.unwrap();
                assert!(warm.success, "{name}: warm build after a {kind} edit failed");
                millis.push(warm.duration.as_millis() as u64);
                steps.push(json!({ "singlePasses": warm.steps.single_passes, "full": warm.steps.full }));
            }
            let (median, p95) = (percentile(&millis, 50), percentile(&millis, 95));
            // "1p" is one single pass; "2p+full" is two passes that then needed the full build.
            let steps_text: Vec<String> = steps
                .iter()
                .map(|s| format!("{}p{}", s["singlePasses"], if s["full"] == true { "+full" } else { "" }))
                .collect();
            println!(
                "{name:<22} cold {:>6} ms | {kind:<7} edit: median {median:>6} ms, p95 {p95:>6} ms, steps {}",
                cold.duration.as_millis(),
                steps_text.join(" ")
            );
            report.push(json!({
                "document": name, "coldMs": cold.duration.as_millis() as u64, "edit": kind,
                "warmMs": millis, "medianMs": median, "p95Ms": p95, "steps": steps,
            }));
        }
    }
    let path = repo_root().join("target/corpus-report.json");
    fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("report: {}", path.display());
}

/// Nearest-rank percentile: the smallest sample with at least `p`% of samples at or below it.
/// With the handful of runs a local harness makes, p95 is simply the slowest; S9.5's CI gate
/// takes enough runs for it to mean more.
fn percentile(samples: &[u64], p: usize) -> u64 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (p * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

#[test]
fn percentile_is_nearest_rank() {
    let samples = [900, 100, 500, 300, 700];
    assert_eq!(percentile(&samples, 50), 500);
    assert_eq!(percentile(&samples, 95), 900);
    assert_eq!(percentile(&[42], 95), 42);
}

/// Copies a corpus document's folder, subfolders included (the thesis has `chapters/`).
fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
