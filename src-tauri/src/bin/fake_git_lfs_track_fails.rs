//! Test double for `git` where Git LFS is installed but tracking itself fails (S11.3c) — `lfs
//! version` and `lfs install` succeed; `lfs track` exits non-zero with a message on stderr, which
//! `lfs.rs`'s own test uses to prove that message reaches the author rather than a bare exit code.

fn main() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    match args.first().map(String::as_str) {
        Some("version" | "install") => {}
        Some("track") => {
            eprintln!("error: could not track - repository is read-only");
            std::process::exit(1);
        }
        _ => std::process::exit(1),
    }
}
