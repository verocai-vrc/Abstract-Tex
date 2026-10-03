//! Test double for `git`, used only by `src-tauri/src/lfs.rs`'s own tests (S11.3c) — never
//! shipped, never run by the app. Answers as a machine with Git LFS installed and working would:
//! `lfs version` and `lfs install --local` succeed with no side effect, and `lfs track <paths>`
//! succeeds and writes the line a real `git-lfs` would, into `.gitattributes` in the current
//! directory — which `Command::current_dir` has already pointed at the project a test built, so
//! staging it afterwards has something real on disk to stage.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(2).collect(); // skip argv[0] and "lfs"
    match args.first().map(String::as_str) {
        Some("version" | "install") => {}
        Some("track") => {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(".gitattributes")
                .unwrap();
            for path in &args[1..] {
                writeln!(file, "{path} filter=lfs diff=lfs merge=lfs -text").unwrap();
            }
        }
        _ => std::process::exit(1),
    }
}
