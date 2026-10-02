//! Test double for a machine with no `latexdiff` at all — every invocation fails, the way a
//! shell actually answers a command that is not on `PATH` (S11.4b's own test of that path, kept
//! deterministic rather than asked of whatever this build machine happens to have installed).

fn main() {
    std::process::exit(127); // the shell's own "command not found" exit code
}
