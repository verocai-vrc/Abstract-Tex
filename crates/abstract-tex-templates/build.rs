//! Tells Cargo to rebuild this crate when anything under `templates/` changes.
//!
//! `include_dir!` reads the folder at compile time, but on stable Rust it cannot tell Cargo which
//! files it read, so without this an edited template would keep shipping its old text until
//! something else forced a rebuild. Cargo watches a directory named here recursively.

fn main() {
    println!("cargo:rerun-if-changed=../../templates");
}
