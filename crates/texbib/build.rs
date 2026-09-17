//! Generates one `#[test]` per fixture directory under `fixtures/` that carries both a
//! `main.bib` and an `expected.json` — the same harness `crates/texlog/build.rs` uses, and its
//! doc comment explains what a build script is. Cargo re-runs this script whenever `fixtures/`
//! changes (the `cargo:rerun-if-changed` line below), so adding a new fixture directory with
//! both files is enough to add its test — no Rust file needs editing: a new `.bib` shape from
//! a new tool lands as data, not code.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Cargo always sets this");
    let fixtures_dir = Path::new(&manifest_dir).join("fixtures");
    println!("cargo:rerun-if-changed={}", fixtures_dir.display());

    let mut entries: Vec<_> = fs::read_dir(&fixtures_dir)
        .expect("fixtures/ must exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    // Sorted so the generated file — and therefore the order `cargo test` lists these tests in —
    // does not depend on the filesystem's own directory-listing order, which varies by OS.
    entries.sort_by_key(|entry| entry.file_name());

    let mut generated = String::new();
    for entry in entries {
        let dir_name = entry.file_name().into_string().expect("fixture directory names are ASCII");
        if !entry.path().join("main.bib").is_file() || !entry.path().join("expected.json").is_file() {
            // No expected.json: a .bib kept for a hand-written unit test elsewhere, not for the
            // harness. Skipped rather than treated as an error.
            continue;
        }
        // A directory name like `better-bibtex` is not a valid Rust identifier
        // (hyphens are not allowed in one); the generated test's name swaps them for underscores.
        let test_name = dir_name.replace('-', "_");
        generated.push_str(&format!(
            "#[test]\nfn fixture_{test_name}() {{\n    run_fixture({dir_name:?});\n}}\n\n"
        ));
    }

    let out_dir = env::var("OUT_DIR").expect("Cargo always sets this");
    let dest = Path::new(&out_dir).join("fixture_tests.rs");
    fs::write(dest, generated).expect("failed to write generated fixture tests");
}
