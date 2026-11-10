//! Test double for `latexdiff` that says which switches it was given (A7), so a test can prove
//! "try again with simpler markup" really reaches the program. `--version` succeeds; otherwise it
//! prints one comment line listing every argument that starts with `--`, which `render_with`
//! writes into the marked-up file where the test reads it back. The two file paths are left out:
//! they are temporary folders and would make the line differ on every run.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        return;
    }
    let switches: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|argument| argument.starts_with("--"))
        .collect();
    println!("% switches: {}", switches.join(" "));
}
