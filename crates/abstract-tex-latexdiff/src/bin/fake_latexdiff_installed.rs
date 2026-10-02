//! Test double for `latexdiff`, used only by this crate's own tests (S11.4b) — never shipped,
//! never run by the app. `--version` succeeds; `--flatten <old> <new>` succeeds and writes a
//! fixed, recognisable string to stdout rather than a real diff, which is all a test needs to
//! prove the string landed in the right file afterwards.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        return;
    }
    if args.first().map(String::as_str) == Some("--flatten") {
        println!("% a latexdiff would have gone here");
        return;
    }
    std::process::exit(1);
}
