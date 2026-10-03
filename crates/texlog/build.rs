//! Generates one `#[test]` per fixture directory under `fixtures/` that carries both a
//! `main.log` and an `expected.json` (S5.3, SPRINTS.md). Cargo re-runs this script whenever
//! `fixtures/` changes (the `cargo:rerun-if-changed` line below), so adding a new fixture
//! directory with both files is enough to add its test — no Rust file needs editing, which is
//! the whole point: S5.4's twenty fixtures should be able to land as data, not code.
//!
//! This is a *build script*: a small program Cargo compiles and runs before it compiles the
//! crate itself, with its own `main`, unrelated to `lib.rs`. Its only job here is to write a
//! generated Rust source file into `OUT_DIR` — a directory Cargo picks per build, outside the
//! source tree, whose path it hands to the build script and to the crate's own code as the
//! `OUT_DIR` environment variable. `tests/fixtures.rs` pulls that generated file in with
//! `include!`, the same way any other Rust file is included, just chosen at build time instead
//! of being written by hand.

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
        let dir_name = entry
            .file_name()
            .into_string()
            .expect("fixture directory names are ASCII");
        if !entry.path().join("main.log").is_file() || !entry.path().join("expected.json").is_file() {
            // No expected.json: this fixture belongs to tokenizer.rs or resolver.rs instead
            // (crates/texlog/fixtures/README.md says which), not the rule catalog this harness
            // checks. Skipped rather than treated as an error.
            continue;
        }
        // A directory name like `undefined-control-sequence` is not a valid Rust identifier
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
