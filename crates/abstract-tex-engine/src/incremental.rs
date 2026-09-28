//! Warm builds in as few TeX passes as the document allows (DESIGN.md §5.1 rung 2, S9.2).
//!
//! Tectonic's default build reruns BibTeX, and TeX after it, on every build — even one where
//! nothing changed — because it starts each run with no memory of the last one. On the corpus
//! thesis that is four TeX passes and seven BibTeX runs, about 20 s, for a one-word edit. It can
//! instead run a single TeX pass (`--pass tex`) that reads the previous build's `.aux` and `.bbl`
//! back from the build folder (`-Z search-path`). This module decides when that is safe and
//! when it has converged; `tectonic.rs` runs the processes.
//!
//! The rule, measured against the corpus rather than guessed:
//! - a warm build starts with one single pass, but only if the last build succeeded;
//! - if the `.aux` files come out identical, the build is done;
//! - if they changed but nothing BibTeX reads did (labels moved, pages shifted), another single
//!   pass, up to [`MAX_SINGLE_PASSES`];
//! - if a citation, database or style changed, one full default build, BibTeX included.
//!
//! It must never run a process or read anything outside the build folder.

use std::fs;
use std::path::{Path, PathBuf};

/// Written into the build folder after a successful build and removed as the next one starts,
/// so a build that failed, crashed or was cancelled can never leave the next one trusting a
/// half-written `.aux`. Deleting `.abstract-tex/` removes it too: the next build is then full.
pub const WARM_MARKER: &str = ".abstract-tex-warm";

/// The most single passes one warm build makes before handing over to a full build. Three is
/// what LaTeX itself needs in the worst common case (a table of contents that moves the pages it
/// lists); a document still moving after that is better served by the engine's own loop.
pub const MAX_SINGLE_PASSES: u32 = 3;

/// Whether the next build may start with a single pass: the last one succeeded, and left the
/// root file's `.aux` behind for the pass to read.
pub fn can_start_warm(out_dir: &Path, stem: &str) -> bool {
    out_dir.join(WARM_MARKER).is_file() && out_dir.join(format!("{stem}.aux")).is_file()
}

/// Every `.aux` file under the build folder, with its contents. `\include` gives each chapter its
/// own `.aux` in a subfolder, so the whole tree is read. The files are small (kilobytes), so they
/// are compared whole rather than hashed: equal text is the one test that cannot be wrong.
#[derive(Debug, PartialEq, Eq)]
pub struct AuxSnapshot {
    files: Vec<(PathBuf, String)>,
}

impl AuxSnapshot {
    pub fn read(out_dir: &Path) -> Self {
        let mut files = Vec::new();
        collect_aux(out_dir, &mut files);
        files.sort();
        Self { files }
    }

    /// Whether anything BibTeX reads changed between `self` and `earlier`: a citation, the
    /// database list, the style. Not the page a citation landed on (`\abx@aux@page`), which moves
    /// with every edit that reflows text and never changes the `.bbl`.
    pub fn bibliography_changed(&self, earlier: &AuxSnapshot) -> bool {
        self.bibliography_lines() != earlier.bibliography_lines()
    }

    fn bibliography_lines(&self) -> Vec<&str> {
        self.files
            .iter()
            .flat_map(|(_, text)| text.lines())
            .filter(|line| BIBLIOGRAPHY_PREFIXES.iter().any(|prefix| line.starts_with(prefix)))
            .collect()
    }
}

/// `.aux` lines whose change means the `.bbl` must be rebuilt. `\citation`/`\bibdata`/`\bibstyle`
/// are BibTeX's own; biblatex on its BibTeX backend adds `\abx@aux@cite`.
const BIBLIOGRAPHY_PREFIXES: &[&str] = &["\\citation{", "\\bibdata{", "\\bibstyle{", "\\abx@aux@cite{"];

fn collect_aux(dir: &Path, files: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_aux(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "aux") {
            // A file that vanished or is not UTF-8 counts as empty rather than aborting the
            // comparison; the worst case is one extra pass, never a wrong PDF.
            let text = fs::read_to_string(&path).unwrap_or_default();
            files.push((path, text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn a_warm_start_needs_both_the_marker_and_the_root_aux() {
        let neither = build_folder(&[]);
        assert!(!can_start_warm(neither.path(), "main"));
        let only_aux = build_folder(&[("main.aux", "\\relax\n")]);
        assert!(!can_start_warm(only_aux.path(), "main"));
        let both = build_folder(&[("main.aux", "\\relax\n"), (WARM_MARKER, "")]);
        assert!(can_start_warm(both.path(), "main"));
        assert!(!can_start_warm(both.path(), "thesis"), "the marker is not enough for another root");
    }

    #[test]
    fn the_snapshot_reads_chapter_aux_files_and_nothing_else() {
        let dir = build_folder(&[
            ("main.aux", "\\@input{chapters/one.aux}\n"),
            ("chapters/one.aux", "\\newlabel{a}{{1}{1}}\n"),
            ("main.log", "not an aux file"),
        ]);
        let snapshot = AuxSnapshot::read(dir.path());
        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.iter().all(|(path, _)| path.extension().unwrap() == "aux"));
    }

    #[test]
    fn a_moved_label_or_page_is_not_a_bibliography_change() {
        let before = build_folder(&[("main.aux", "\\citation{knuth}\n\\newlabel{a}{{1}{1}}\n\\abx@aux@page{1}{3}\n")]);
        let after = build_folder(&[("main.aux", "\\citation{knuth}\n\\newlabel{a}{{1}{2}}\n\\abx@aux@page{1}{4}\n")]);
        let (before, after) = (AuxSnapshot::read(before.path()), AuxSnapshot::read(after.path()));
        assert_ne!(before, after, "the files did change");
        assert!(!after.bibliography_changed(&before));
    }

    #[test]
    fn a_new_citation_or_database_is_a_bibliography_change() {
        let before = build_folder(&[("main.aux", "\\citation{knuth}\n\\bibdata{refs}\n")]);
        let new_cite = build_folder(&[("main.aux", "\\citation{knuth}\n\\citation{lamport}\n\\bibdata{refs}\n")]);
        let new_data = build_folder(&[("main.aux", "\\citation{knuth}\n\\bibdata{refs,more}\n")]);
        let before = AuxSnapshot::read(before.path());
        assert!(AuxSnapshot::read(new_cite.path()).bibliography_changed(&before));
        assert!(AuxSnapshot::read(new_data.path()).bibliography_changed(&before));
    }

    #[test]
    fn a_citation_in_a_chapter_aux_counts_too() {
        let before = build_folder(&[("main.aux", ""), ("chapters/one.aux", "")]);
        let after = build_folder(&[("main.aux", ""), ("chapters/one.aux", "\\citation{knuth}\n")]);
        assert!(AuxSnapshot::read(after.path()).bibliography_changed(&AuxSnapshot::read(before.path())));
    }
}
