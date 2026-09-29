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
//! shows. Timing is measured by `warm_build_timings` and, since S9.5, asserted there too: a
//! document whose warm p95 breaches its [`THRESHOLD_MS`] entry fails the build, the same "no
//! warning, only pass or fail" rule the broken document's diagnostics already get.

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
    ("minted", Expect::Unsupported { reason: "minted needs shell-escape, off unless the person at the machine allows it (S9.8); see minted_builds_once_shell_escape_is_allowed" }),
    ("non-latin", Expect::Builds { min_pages: 1 }),
    ("broken", Expect::Broken),
    ("pathological-preamble", Expect::Builds { min_pages: 1 }),
];

/// S9.5's performance gate, one warm p95 ceiling per buildable document, in milliseconds.
///
/// DESIGN.md §7's exit criterion asked for one number — 1.2 s on the thesis — for every warm
/// build. S9.3 and S9.7 found the thesis cannot reach it on the bundled engine (its ~1.9 s
/// preamble is the floor; §7 now says so). Measuring the rest of the corpus for this gate found
/// a second reason one number cannot work: `tikz-figures`, a two-page document, warms slower
/// than the sixty-page thesis (3-D `pgfplots` is the cost, not page count), so "1.2 s except the
/// thesis" would have let a TikZ regression through unnoticed. Each document gets its own
/// ceiling instead, so the gate catches a regression *in that document's own shape of cost*.
///
/// Every number here is this machine's measured p95 (`ABSTRACT_TEX_BENCH_RUNS=5`, 29 Sep 2026)
/// with roughly 1.8× headroom, rounded — a guess at how much slower a CI runner is, not a
/// measurement of one. **Provisional** until S9.5's own done-when is met: a real green run on
/// CI, and a red one from a commit that deliberately slows a document, both recorded in
/// SPRINTS.md. Tighten from there, not from a second guess made here.
const THRESHOLD_MS: &[(&str, u64)] = &[
    ("conference", 1_200), // 595 ms measured — also DESIGN.md §7's original number, kept legible
    ("thesis", 6_000),     // 3_338 ms measured, full warm build (not the S9.7/S9.9 chapter draft)
    ("beamer", 3_000),     // 1_650 ms measured
    ("tikz-figures", 6_500), // 3_704 ms measured — slower than the thesis; see the module doc above
    ("non-latin", 1_500),  // 833 ms measured
    ("pathological-preamble", 4_500), // 2_513 ms measured — heavy by design (S9.1)
];

fn threshold_ms(document: &str) -> u64 {
    THRESHOLD_MS
        .iter()
        .find(|(name, _)| *name == document)
        .map(|(_, ms)| *ms)
        .unwrap_or_else(|| panic!("{document} has no THRESHOLD_MS entry; the performance gate must cover every buildable corpus document"))
}

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

/// S9.5: a corpus document that builds but has no `THRESHOLD_MS` entry would make the ignored
/// timing test panic a long way from here, after a real (slow) build. This runs with no engine,
/// so a document added without a ceiling is caught on every `cargo test`, not only in CI.
#[test]
fn every_buildable_document_has_a_performance_ceiling() {
    for (name, expect) in &CORPUS {
        if matches!(expect, Expect::Builds { .. }) {
            threshold_ms(name); // panics with a clear message if `name` is missing
        }
    }
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
            shell_escape: false,
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
            shell_escape: false,
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
            // S9.5: fail, do not warn, the same rule the broken document's diagnostics get. Both
            // edit kinds on the thesis are checked against the one `thesis` entry — a regression
            // in either shows up as one document breaching its ceiling.
            let ceiling = threshold_ms(name);
            assert!(
                p95 <= ceiling,
                "{name} ({kind} edit): warm p95 {p95} ms breached its {ceiling} ms ceiling (THRESHOLD_MS in this file)"
            );
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

/// S9.7's done-when, on the document it is for: after a full build of the thesis and an edit to
/// chapter 3, a draft of the chapter the include graph names builds with every reference
/// resolved, numbers the chapter exactly as the full build then does, leaves the full build's
/// folder byte-for-byte alone, and is faster than the warm full pass.
#[tokio::test]
#[ignore]
async fn a_thesis_chapter_drafts_faster_with_the_full_builds_numbering() {
    use abstract_tex_engine::draft::DraftJob;

    let engine = Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root()).expect("run `pnpm fetch-engine`"));
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&corpus_dir().join("thesis"), tmp.path());
    let job = BuildJob {
        project_dir: tmp.path().to_path_buf(),
        root_file: PathBuf::from("main.tex"),
        out_dir: tmp.path().join(".abstract-tex/build"),
        synctex: true,
        shell_escape: false,
    };
    assert!(engine.build(&job, CancellationToken::new(), None).await.unwrap().success);

    let edited = "chapters/03-method.tex";
    let mut text = fs::read_to_string(tmp.path().join(edited)).unwrap();
    text.push_str("\nOne more sentence of method, written between builds.\n");
    fs::write(tmp.path().join(edited), text).unwrap();

    let graph = abstract_tex_includes::build_graph(tmp.path(), Path::new("main.tex"));
    let chapter = graph.chapter_of(edited).expect("chapter 3 is an \\include").argument.clone();
    assert_eq!(chapter, "chapters/03-method");
    let draft = DraftJob { chapter, dir: tmp.path().join(".abstract-tex/draft") };

    let full_folder_before = folder_bytes(&job.out_dir);
    let layout = abstract_tex_engine::draft::prepare(&job, &draft).unwrap().expect("a warm thesis has a draft");
    let drafted = engine.build_draft(&job, &layout, CancellationToken::new()).await.unwrap().expect("Tectonic drafts");
    assert!(drafted.success, "{}", drafted.stderr);
    assert!(full_folder_before == folder_bytes(&job.out_dir), "a draft must not touch the full build's folder");
    let draft_log = fs::read_to_string(drafted.log.as_ref().unwrap()).unwrap();
    assert_eq!(draft_log.matches("undefined").count(), 0, "a draft borrows every reference from the full build");
    assert!(drafted.pdf.is_some(), "a draft that leaves no PDF is no draft");

    let full = engine.build(&job, CancellationToken::new(), None).await.unwrap();
    assert!(full.success);
    let chapter_numbering = |folder: &Path| numbering(&fs::read_to_string(folder.join("chapters/03-method.aux")).unwrap());
    let drafted_numbering = chapter_numbering(&draft.dir.join("build"));
    assert!(drafted_numbering.len() > 50, "{drafted_numbering:?}");
    assert_eq!(drafted_numbering, chapter_numbering(&job.out_dir), "the draft numbers the chapter as the full build does");
    let pages = |log: &str| pages_written(log).unwrap();
    let full_log = fs::read_to_string(full.log.as_ref().unwrap()).unwrap();
    println!(
        "draft {} ms, {} pages | full warm pass {} ms, {} pages",
        drafted.duration.as_millis(),
        pages(&draft_log),
        full.duration.as_millis(),
        pages(&full_log)
    );
    assert!(pages(&draft_log) < pages(&full_log));
    assert!(drafted.duration < full.duration, "a draft slower than the full pass has no reason to exist");
}

/// S9.10 against the real engine: a warm thesis build cancelled part-way through its pass — as a
/// save that lands mid-build cancels it — leaves the build folder able to start warm, so the
/// build after it is single passes with every reference resolved, not the ~12 s full build.
/// The cancel lands at 2.5 s of a ~3.3 s pass on a 12-core machine. Measured while writing this
/// test: Tectonic keeps a pass's intermediates in memory and writes them out as the pass ends, so
/// a kill even at 3.3 s left every `.aux` untouched. What a cancel really cost was the warm
/// marker, and that is what this test catches going missing: without S9.10 it fails, full build.
#[tokio::test]
#[ignore]
async fn a_cancelled_thesis_build_leaves_the_next_one_warm() {
    let engine = Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root()).expect("run `pnpm fetch-engine`"));
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&corpus_dir().join("thesis"), tmp.path());
    let job = BuildJob {
        project_dir: tmp.path().to_path_buf(),
        root_file: PathBuf::from("main.tex"),
        out_dir: tmp.path().join(".abstract-tex/build"),
        synctex: true,
        shell_escape: false,
    };
    assert!(engine.build(&job, CancellationToken::new(), None).await.unwrap().success);

    let edited = tmp.path().join("chapters/03-method.tex");
    let mut text = fs::read_to_string(&edited).unwrap();
    text.push_str("\nA sentence saved while the build was still running.\n");
    fs::write(&edited, text).unwrap();

    let cancel = CancellationToken::new();
    let cancel_later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        cancel_later.cancel();
    });
    let cancelled = engine.build(&job, cancel, None).await;
    assert!(matches!(cancelled, Err(EngineError::Cancelled)), "the pass must still be running at 2.5 s: {cancelled:?}");

    let started = Instant::now();
    let next = engine.build(&job, CancellationToken::new(), None).await.unwrap();
    assert!(next.success, "{}", next.stderr);
    assert!(!next.steps.full, "the build after a cancel must start warm, not full: {:?}", next.steps);
    let log = fs::read_to_string(next.log.as_ref().unwrap()).unwrap();
    assert_eq!(log.matches("undefined").count(), 0, "a restored folder resolves every reference");
    println!("after a cancel: {:?} in {} ms", next.steps, started.elapsed().as_millis());
}

/// S9.8 against the real engine: the corpus's `minted` document, which fails without shell
/// escape (above), builds with it — and nothing it runs writes into the source tree: minted's
/// cache lands in the build folder, where the next build finds it. Needs `pygmentize` on `PATH`.
#[tokio::test]
#[ignore]
async fn minted_builds_once_shell_escape_is_allowed() {
    let engine = Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root()).expect("run `pnpm fetch-engine`"));
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&corpus_dir().join("minted"), tmp.path());
    let source_tree_before = source_tree(tmp.path());
    let job = BuildJob {
        project_dir: tmp.path().to_path_buf(),
        root_file: PathBuf::from("main.tex"),
        out_dir: tmp.path().join(".abstract-tex/build"),
        synctex: true,
        shell_escape: true,
    };

    let cold = engine.build(&job, CancellationToken::new(), None).await.unwrap();
    assert!(cold.success, "needs pygmentize on PATH:\n{}", cold.stderr);
    assert!(cold.pdf.is_some());
    assert_eq!(source_tree(tmp.path()), source_tree_before, "shell commands must not write into the source tree");
    assert!(job.out_dir.join("_minted-main").is_dir() || job.out_dir.join("_minted").is_dir(), "minted's cache stays in the build folder");

    let warm = engine.build(&job, CancellationToken::new(), None).await.unwrap();
    assert!(warm.success, "{}", warm.stderr);
    assert!(!warm.steps.full, "a warm minted build is single passes too: {:?}", warm.steps);
    assert_eq!(source_tree(tmp.path()), source_tree_before);
}

/// Every path under `dir` outside `.abstract-tex/`: what the author sees as their project.
fn source_tree(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|name| name == ".abstract-tex") {
            continue;
        }
        if path.is_dir() {
            paths.extend(source_tree(&path));
        }
        paths.push(path);
    }
    paths.sort();
    paths
}

/// What an author reads off a chapter's `.aux`: each label's number and page, and every counter
/// the chapter hands on to the next one. Not the raw bytes: hyperref names its link targets from
/// a document-wide caption count that `\\include` does not checkpoint, so a draft's anchors are
/// `figure.caption.4` where the full build's are `figure.caption.8` — consistent inside each
/// PDF, never shown, and not numbering.
fn numbering(aux: &str) -> Vec<String> {
    let mut kept = Vec::new();
    for line in aux.lines() {
        if line.starts_with("\\setcounter{") {
            kept.push(line.to_string());
        } else if let Some(rest) = line.strip_prefix("\\newlabel{") {
            // `name}{{number}{page}{title}{anchor}{}}`: keep up to the end of the page group.
            let groups: Vec<&str> = rest.splitn(4, '}').collect();
            kept.push(groups[..3.min(groups.len())].join("}"));
        }
    }
    kept
}

/// Every file under `dir` with its bytes, in a stable order: "untouched" means equal to this.
fn folder_bytes(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(folder_bytes(&path));
        } else {
            files.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    files.sort();
    files
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
