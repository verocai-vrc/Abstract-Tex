//! Test double for `latexdiff` installed but refusing the two files it was given — `--version`
//! succeeds (so this is not the "not installed" path); `--flatten` exits non-zero with a message
//! on stderr, which S11.4b's own test uses to prove that message reaches the final error rather
//! than a bare exit code.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        return;
    }
    eprintln!("latexdiff: unbalanced braces in old/main.tex");
    std::process::exit(1);
}
