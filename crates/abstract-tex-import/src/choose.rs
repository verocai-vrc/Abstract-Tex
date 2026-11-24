//! What a chosen file is, and which folder its document is in.

use std::fs;
use std::path::{Path, PathBuf};

use abstract_tex_includes::build_graph;

/// What a file is, by its ending, which is how the person who picked it knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Tex,
    Zip,
    /// Any word processor's.
    Word,
    Pdf,
    /// An archive that is not a zip.
    OtherArchive,
    Other,
}

pub fn kind_of(file: &Path) -> FileKind {
    let ending = file
        .extension()
        .map(|ending| ending.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ending.as_str() {
        "tex" => FileKind::Tex,
        "zip" => FileKind::Zip,
        "doc" | "docx" | "docm" | "dot" | "dotx" | "odt" | "rtf" | "pages" => FileKind::Word,
        "pdf" => FileKind::Pdf,
        "tar" | "gz" | "tgz" | "bz2" | "tbz" | "xz" | "txz" | "7z" | "rar" | "zst" => FileKind::OtherArchive,
        _ => FileKind::Other,
    }
}

/// How many folders above its own a file's document is looked for in.
const LEVELS_UP: usize = 3;

/// The folder of the document `file` belongs to. From its own folder upwards, stopping before a crowded
/// one: the first that has settings, or whose main file is another file that includes this one. Else its
/// own folder. `None` when that is crowded: the file is there among other things.
///
/// `root_of` answers a folder's main file, as a path inside it. It is handed in because finding one is
/// the app's business, and so that this crate needs nothing of the app's.
pub fn document_folder(
    file: &Path,
    crowded: &dyn Fn(&Path) -> bool,
    root_of: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let own = file.parent()?;
    if crowded(own) {
        return None;
    }
    let found = own
        .ancestors()
        .take(LEVELS_UP + 1)
        .take_while(|folder| !crowded(folder))
        .find(|folder| folder.join("abstract-tex.toml").is_file() || includes(folder, file, root_of));
    Some(found.unwrap_or(own).to_path_buf())
}

/// Whether `folder` is a document with `file` as one of its parts, rather than as its main file.
fn includes(folder: &Path, file: &Path, root_of: &dyn Fn(&Path) -> Option<PathBuf>) -> bool {
    let (Some(root), Ok(inside)) = (root_of(folder), file.strip_prefix(folder)) else {
        return false;
    };
    if root == inside {
        return false;
    }
    let inside = inside.to_string_lossy().replace('\\', "/");
    build_graph(folder, &root)
        .nodes
        .iter()
        .any(|node| node.path == inside)
}

/// How deep, and through how many files, [`has_tex`] looks.
const TEX_DEPTH: usize = 4;
const TEX_ENTRIES: usize = 5_000;

/// Whether there is any `.tex` file in a folder. A folder too big to look through is taken to have one:
/// this only decides whether a question is asked.
pub fn has_tex(folder: &Path) -> bool {
    let mut folders = vec![(folder.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((next, depth)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > TEX_ENTRIES {
                return true;
            }
            // `file_type` does not follow a link, so links that point at each other end nowhere.
            let Ok(kind) = entry.file_type() else { continue };
            let is_hidden = entry.file_name().to_string_lossy().starts_with('.');
            if kind.is_dir() && depth < TEX_DEPTH && !is_hidden {
                folders.push((entry.path(), depth + 1));
            } else if !kind.is_dir() && kind_of(&entry.path()) == FileKind::Tex {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with these files in it.
    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let place = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = place.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        place
    }

    /// Stand-in for the app's own rule: `main.tex`, else the only `.tex` that has a `\documentclass`.
    fn root_of(folder: &Path) -> Option<PathBuf> {
        if folder.join("main.tex").is_file() {
            return Some("main.tex".into());
        }
        fs::read_dir(folder).ok()?.flatten().find_map(|entry| {
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            text.contains("\\documentclass").then(|| entry.file_name().into())
        })
    }

    fn found(place: &tempfile::TempDir, file: &str, crowded: &[&str]) -> Option<String> {
        let is_crowded = |folder: &Path| crowded.iter().any(|name| folder == place.path().join(name));
        let answer = document_folder(&place.path().join(file), &is_crowded, &root_of)?;
        let inside = answer
            .strip_prefix(place.path())
            .expect("never above the test's own folder");
        Some(inside.to_string_lossy().into_owned())
    }

    const THESIS: &str = "\\documentclass{book}\\include{chapters/intro}";

    #[test]
    fn a_chapter_opens_its_thesis() {
        let place = folder(&[("thesis/main.tex", THESIS), ("thesis/chapters/intro.tex", "text")]);
        assert_eq!(
            found(&place, "thesis/chapters/intro.tex", &[]).as_deref(),
            Some("thesis")
        );
    }

    #[test]
    fn a_chapter_that_is_also_its_own_document_still_opens_its_thesis() {
        let place = folder(&[
            (
                "thesis/main.tex",
                "\\documentclass{book}\\usepackage{subfiles}\\subfile{chapters/intro}",
            ),
            ("thesis/chapters/intro.tex", "\\documentclass[../main]{subfiles}"),
        ]);
        assert_eq!(
            found(&place, "thesis/chapters/intro.tex", &[]).as_deref(),
            Some("thesis")
        );
    }

    #[test]
    fn a_folder_with_settings_is_the_document() {
        let place = folder(&[("thesis/abstract-tex.toml", ""), ("thesis/a/b/notes.tex", "text")]);
        assert_eq!(
            found(&place, "thesis/a/b/notes.tex", &[]).as_deref(),
            Some("thesis")
        );
    }

    #[test]
    fn a_document_of_its_own_opens_its_own_folder() {
        let place = folder(&[
            ("thesis/main.tex", THESIS),
            ("thesis/letters/bees.tex", "\\documentclass{letter}"),
        ]);
        assert_eq!(
            found(&place, "thesis/letters/bees.tex", &[]).as_deref(),
            Some("thesis/letters")
        );
        assert_eq!(found(&place, "thesis/main.tex", &[]).as_deref(), Some("thesis"));
    }

    #[test]
    fn the_search_stops_at_three_levels_and_at_a_crowded_folder() {
        let deep = folder(&[
            (
                "t/main.tex",
                "\\documentclass{book}\\input{a/b/c/x}\\input{a/b/c/d/x}",
            ),
            ("t/a/b/c/x.tex", "three up"),
            ("t/a/b/c/d/x.tex", "four up"),
        ]);
        assert_eq!(found(&deep, "t/a/b/c/x.tex", &[]).as_deref(), Some("t"));
        assert_eq!(found(&deep, "t/a/b/c/d/x.tex", &[]).as_deref(), Some("t/a/b/c/d"));

        let above = folder(&[
            ("t/main.tex", "\\documentclass{book}\\input{Downloads/new/x}"),
            ("t/Downloads/new/x.tex", "text"),
        ]);
        assert_eq!(
            found(&above, "t/Downloads/new/x.tex", &["t/Downloads"]).as_deref(),
            Some("t/Downloads/new")
        );
    }

    #[test]
    fn a_lone_file_in_a_crowded_folder_has_no_folder() {
        let place = folder(&[("Downloads/paper.tex", "\\documentclass{article}")]);
        assert_eq!(found(&place, "Downloads/paper.tex", &["Downloads"]), None);
    }

    #[test]
    fn a_file_is_known_by_its_ending() {
        for (name, kind) in [
            ("a.tex", FileKind::Tex),
            ("A.TEX", FileKind::Tex),
            ("a.zip", FileKind::Zip),
            ("a.docx", FileKind::Word),
            ("a.odt", FileKind::Word),
            ("a.pdf", FileKind::Pdf),
            ("a.tar.gz", FileKind::OtherArchive),
            ("a.txt", FileKind::Other),
            ("tex", FileKind::Other),
        ] {
            assert_eq!(kind_of(Path::new(name)), kind, "{name}");
        }
    }

    #[test]
    fn a_folder_has_tex_when_a_file_in_it_or_below_is_one() {
        assert!(has_tex(folder(&[("a/b/x.tex", "")]).path()));
        assert!(!has_tex(folder(&[("a/x.txt", ""), (".hidden/x.tex", "")]).path()));
        assert!(!has_tex(Path::new("/nowhere/at/all")));
        assert!(
            !has_tex(folder(&[("a/b/c/d/e/x.tex", "")]).path()),
            "five down is not looked at"
        );
        let many: Vec<String> = (0..=TEX_ENTRIES).map(|n| format!("{n}.txt")).collect();
        let many: Vec<(&str, &str)> = many.iter().map(|name| (name.as_str(), "")).collect();
        assert!(has_tex(folder(&many).path()), "too big to say");
    }
}
