//! Test double for `git` on a machine with no Git LFS at all — every `lfs` subcommand fails, the
//! way a real `git` actually answers ("'lfs' is not a git command"). `src-tauri/src/lfs.rs`'s own
//! test of the "not installed" path (S11.3c) runs against this rather than against whatever this
//! build machine happens to have — GitHub-hosted CI runners ship Git LFS by default, so a test
//! that asked the real `git` would pass here and fail there.

fn main() {
    eprintln!("git: 'lfs' is not a git command. See 'git --help'.");
    std::process::exit(1);
}
