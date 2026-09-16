//! The fixture harness (S5.3, SPRINTS.md): runs [`texlog::diagnostics`] against every fixture
//! directory that carries an `expected.json` and checks the result matches. Every individual
//! `#[test]` function below this module's own code comes from `build.rs`, generated fresh for
//! whatever fixture directories exist on disk right now — see its own doc comment. That is what
//! "a test per fixture" means here: this file, and the crate, never lists fixtures by name.
//!
//! This is an integration test (`crates/texlog/tests/`, not `src/`), because it exercises
//! [`texlog::diagnostics`] as a caller outside the crate would — the same boundary the crate's
//! own `lib.rs` promises to hold (text in, data out) is the one this file tests across.

use std::fs;
use std::path::Path;

/// Parse `fixtures/<name>/main.log` through the rule catalog and compare it, as JSON, against
/// `fixtures/<name>/expected.json`.
///
/// Comparing parsed [`serde_json::Value`]s, rather than the raw text of both sides, means key
/// order and whitespace in a hand-edited `expected.json` never matter, only the data — and a
/// mismatch's panic message pretty-prints what the parser actually produced, so fixing a wrong
/// `expected.json` (or writing a new one for S5.4) is a copy-paste, not a hand transcription.
fn run_fixture(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name);

    let log = fs::read_to_string(dir.join("main.log"))
        .unwrap_or_else(|e| panic!("{name}: could not read main.log: {e}"));
    let expected_text = fs::read_to_string(dir.join("expected.json"))
        .unwrap_or_else(|e| panic!("{name}: could not read expected.json: {e}"));
    let expected: serde_json::Value = serde_json::from_str(&expected_text)
        .unwrap_or_else(|e| panic!("{name}: expected.json is not valid JSON: {e}"));

    let found = texlog::diagnostics(&log);
    let found_json = serde_json::to_value(&found).expect("Diagnostic always serialises");

    assert_eq!(
        found_json,
        expected,
        "{name}: parsed diagnostics did not match expected.json.\n\
         If this is a new or intentionally changed fixture, this is the expected.json to write:\n{}",
        serde_json::to_string_pretty(&found_json).unwrap()
    );
}

include!(concat!(env!("OUT_DIR"), "/fixture_tests.rs"));
