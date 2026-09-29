//! Warm builds in as few TeX passes as the document allows (DESIGN.md §5.1 rung 2, S9.2).
//!
//! Tectonic's default build reruns BibTeX, and TeX after it, on every build — even one where
//! nothing changed — because it starts each run with no memory of the last one. On the corpus
//! thesis that is four TeX passes and seven BibTeX runs, about 20 s, for a one-word edit. It can
//! instead run a single TeX pass (`--reruns 0`) that reads the previous build's `.aux` and `.bbl`
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
//! A warm build that is cancelled — every save that lands while a build runs — puts the folder
//! back the way the last successful build left it ([`Checkpoint`], S9.10), so the build after it
//! can still start warm.
//!
//! It must never run a process or read anything outside the build folder.

use std::fs;
use std::io;
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

/// Extensions of the files a pass writes from scratch rather than reads back: its outputs. A
/// checkpoint leaves them alone, and a draft does not inherit them (`draft.rs`). `gz` is the
/// `.synctex.gz`; `blg` is BibTeX's own log.
pub const OUTPUT_EXTENSIONS: &[&str] = &["pdf", "xdv", "log", "gz", "blg"];

/// Whether `path` is one of a pass's outputs ([`OUTPUT_EXTENSIONS`]).
pub fn is_output(path: &Path) -> bool {
    path.extension().is_some_and(|ext| OUTPUT_EXTENSIONS.iter().any(|output| ext == *output))
}

/// Every intermediate file in a build folder — the `.aux`, `.bbl`, `.toc`, `.out`, … a pass reads
/// back — held in memory, taken while the folder is still as a successful build left it (S9.10).
///
/// A pass that is killed can leave any of them half-written, which is why the warm marker is
/// removed as each build starts. With a checkpoint, a cancelled build does not have to cost the
/// next one its warm start: [`restore`](Checkpoint::restore) puts every intermediate back and
/// then the marker, so the marker still vouches for exactly what it always did.
///
/// Measured on the corpus thesis (S9.10): Tectonic holds a pass's intermediates in memory and
/// writes them out as the pass ends, so most kills leave the folder untouched and this restore
/// changes nothing but the marker. It is kept anyway because it makes the marker's promise hold
/// without relying on that: a kill during the write-out, or between two passes, is put right too.
#[derive(Debug)]
pub struct Checkpoint {
    out_dir: PathBuf,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl Checkpoint {
    /// Read the intermediates of `out_dir`. Tens of kilobytes on the corpus thesis.
    pub fn take(out_dir: &Path) -> io::Result<Self> {
        let mut paths = Vec::new();
        collect_intermediates(out_dir, &mut paths)?;
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = fs::read(&path)?;
            files.push((path, bytes));
        }
        Ok(Self { out_dir: out_dir.to_path_buf(), files })
    }

    /// Put the folder back: remove intermediates created since [`take`](Checkpoint::take), write
    /// every checkpointed one back, and write the warm marker last — so a restore that fails
    /// part-way leaves no marker, and the next build is full, as it would have been without one.
    /// Outputs are not touched; the next build rewrites them.
    pub fn restore(&self) -> io::Result<()> {
        let mut now = Vec::new();
        collect_intermediates(&self.out_dir, &mut now)?;
        for path in now {
            if !self.files.iter().any(|(kept, _)| *kept == path) {
                fs::remove_file(&path)?;
            }
        }
        for (path, bytes) in &self.files {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, bytes)?;
        }
        fs::write(self.out_dir.join(WARM_MARKER), b"")
    }
}

/// Every file under `dir` but the outputs and the marker, whose presence is the checkpoint's
/// to decide rather than something to copy.
fn collect_intermediates(dir: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_intermediates(&path, paths)?;
        } else if !is_output(&path) && path.file_name().is_some_and(|name| name != WARM_MARKER) {
            paths.push(path);
        }
    }
    Ok(())
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

    #[test]
    fn a_restored_checkpoint_is_the_folder_as_it_was_with_the_marker_last() {
        let dir = build_folder(&[
            ("main.aux", "\\relax\n\\@input{chapters/one.aux}\n"),
            ("chapters/one.aux", "\\newlabel{a}{{1}{2}}\n"),
            ("main.bbl", "\\begin{thebibliography}{1}\\end{thebibliography}\n"),
            ("main.pdf", "%PDF old"),
            (WARM_MARKER, ""),
        ]);
        let checkpoint = Checkpoint::take(dir.path()).unwrap();

        // What a killed pass leaves: its marker gone, an .aux cut short, a file it had just
        // created, and a PDF half-written.
        fs::remove_file(dir.path().join(WARM_MARKER)).unwrap();
        fs::write(dir.path().join("main.aux"), "\\relax\n\\@in").unwrap();
        fs::write(dir.path().join("chapters/two.aux"), "").unwrap();
        fs::write(dir.path().join("main.pdf"), "%PDF half").unwrap();

        checkpoint.restore().unwrap();
        let read = |name: &str| fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("main.aux"), "\\relax\n\\@input{chapters/one.aux}\n");
        assert_eq!(read("chapters/one.aux"), "\\newlabel{a}{{1}{2}}\n");
        assert!(!dir.path().join("chapters/two.aux").exists(), "created by the killed pass, so removed");
        assert_eq!(read("main.pdf"), "%PDF half", "outputs are the next build's to rewrite");
        assert!(can_start_warm(dir.path(), "main"));
    }

    #[test]
    fn outputs_are_recognised_by_extension() {
        assert!(is_output(Path::new("b/main.synctex.gz")));
        assert!(is_output(Path::new("b/main.pdf")));
        assert!(!is_output(Path::new("b/main.aux")));
        assert!(!is_output(Path::new("b/main.run.xml")));
    }
}
