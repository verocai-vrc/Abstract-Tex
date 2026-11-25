//! The files a document names, when they are beside it: what has to travel with a `.tex` that is taken out
//! of Downloads.
//!
//! **What it must never do:** follow a name out of the folder, or a link anywhere; or change the originals.
//! The names are whatever a file from a stranger says, and what they name is copied.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use abstract_tex_includes::{scan_includes, Directive};

use crate::staged::Staged;

/// Why a copy was not made.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("{name} could not be read")]
    Read { name: String, source: io::Error },
    #[error("the copy could not be written")]
    Write(#[source] io::Error),
}

/// The files `file` names that are beside it, and those that they name. `file` itself is not one.
pub fn files_named_by(file: &Path) -> Vec<PathBuf> {
    let (Some(folder), Some(own_name)) = (file.parent(), file.file_name()) else {
        return Vec::new();
    };
    let beside = ordinary_files_in(folder);
    let mut found = vec![own_name.to_string_lossy().into_owned()];
    // `found` grows while it is read, so this is not a `for`: each file's names are looked up in turn,
    // including those of the files they bring in.
    let mut next = 0;
    while let Some(name) = found.get(next) {
        let path = folder.join(name);
        next += 1;
        if !is_source(&path) {
            continue;
        }
        for wanted in names_in(&text_of(&path)) {
            for answer in matching(&beside, &wanted) {
                if !found.contains(answer) {
                    found.push(answer.clone());
                }
            }
        }
    }
    found.iter().skip(1).map(|name| folder.join(name)).collect()
}

/// A folder's ordinary files: no folder, no link, no pipe. By their names without regard to case, which is
/// how Windows and macOS find them; on Linux more than one can have the same.
fn ordinary_files_in(folder: &Path) -> HashMap<String, Vec<String>> {
    let mut files: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(entries) = fs::read_dir(folder) else {
        return files;
    };
    for entry in entries.flatten() {
        // Of the entry itself, without following it if it is a link.
        let is_ordinary = entry.file_type().is_ok_and(|kind| kind.is_file());
        // A name that is not Unicode is not one a document can have.
        if let (true, Ok(name)) = (is_ordinary, entry.file_name().into_string()) {
            let key = name.to_lowercase();
            // Either of these can say that the folder it is in is part of a repository somewhere else.
            if !matches!(key.as_str(), ".git" | "commondir") {
                files.entry(key).or_default().push(name);
            }
        }
    }
    files
}

/// Whether a file may name others. Whatever else it is, it is not a figure or a bibliography.
fn is_source(path: &Path) -> bool {
    let ending = path
        .extension()
        .map(|ending| ending.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ending.as_str(),
        "tex" | "cls" | "sty" | "def" | "cfg" | "ltx" | "tikz" | "pgf" | "clo" | "bbx" | "cbx"
    )
}

/// How much of a file is looked through for names. A document is a fraction of this.
const MOST_TEXT: u64 = 10 * 1024 * 1024;

/// A file's text, or none. Letters that are not UTF-8 may come out wrong: this is only read for names.
fn text_of(path: &Path) -> String {
    let is_readable = fs::metadata(path).is_ok_and(|found| found.is_file() && found.len() <= MOST_TEXT);
    if !is_readable {
        return String::new();
    }
    let bytes = fs::read(path).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A name as a document has it, and the endings that may be left out of it.
struct Wanted {
    name: String,
    endings: &'static [&'static str],
}

/// What the graphics package looks for when a picture is named without its ending.
const PICTURES: &[&str] = &["pdf", "eps", "ps", "png", "jpg", "jpeg", "mps", "bmp", "svg"];

/// Every command that names a file: which of its arguments does, and the endings it may leave out. The
/// include commands are `scan_includes`'s, except the ones it only recognises.
fn names_a_file(command: &str) -> Option<(usize, &'static [&'static str])> {
    Some(match command {
        "InputIfFileExists" => (0, &["tex"]),
        "includegraphics" | "includesvg" => (0, PICTURES),
        "includepdf" => (0, &["pdf"]),
        "bibliography" | "addbibresource" | "addglobalbib" | "addsectionbib" => (0, &["bib"]),
        "bibliographystyle" => (0, &["bst"]),
        "documentclass" | "LoadClass" | "LoadClassWithOptions" => (0, &["cls"]),
        "usepackage" | "RequirePackage" | "RequirePackageWithOptions" => (0, &["sty"]),
        "lstinputlisting" | "verbatiminput" | "VerbatimInput" => (0, &["tex"]),
        "inputminted" | "import" | "subimport" => (1, &["tex"]),
        _ => return None,
    })
}

/// Every name a text has, whether or not there is such a file.
fn names_in(text: &str) -> Vec<Wanted> {
    let mut wanted = Vec::new();
    for directive in scan_includes(text) {
        let name = match directive {
            Directive::Include { argument, .. } => argument,
            // `\input intro`, without braces. Of the rest nothing is the name of a file.
            Directive::Unparsed { raw, .. } => match raw.split_once(' ') {
                Some((_, word)) => word.to_string(),
                None => continue,
            },
        };
        wanted.push(Wanted {
            name,
            endings: &["tex"],
        });
    }
    for (command, arguments) in commands_in(&without_comments(text)) {
        let Some((which, endings)) = names_a_file(command) else {
            continue;
        };
        let Some(argument) = arguments.get(which) else {
            continue;
        };
        // `\import{folder}{file}`: only from this folder.
        let folder = match command {
            "import" | "subimport" => arguments[0].trim(),
            _ => "",
        };
        for name in argument.split(',') {
            let name = name.replace(['"', '{', '}'], "");
            let path = Path::new(folder).join(name.trim());
            wanted.push(Wanted {
                name: path.to_string_lossy().into_owned(),
                endings,
            });
        }
    }
    wanted
}

/// The files that answer to a name: that one, and without its ending any that has one of its endings.
fn matching<'a>(beside: &'a HashMap<String, Vec<String>>, wanted: &Wanted) -> Vec<&'a String> {
    let Some(name) = name_in_this_folder(&wanted.name) else {
        return Vec::new();
    };
    let mut spellings = vec![name.clone()];
    spellings.extend(wanted.endings.iter().map(|ending| format!("{name}.{ending}")));
    let mut found = Vec::new();
    for spelling in spellings {
        let Some(files) = beside.get(&spelling.to_lowercase()) else {
            continue;
        };
        // As it is spelled, if one is.
        found.extend(files.iter().find(|file| **file == spelling).or(files.first()));
    }
    found
}

/// The one plain name that `name` is, if it is one: `./a` is `a`, and `sub/a`, `../a` and `/a` are none.
fn name_in_this_folder(name: &str) -> Option<String> {
    let mut parts = Path::new(name.trim())
        .components()
        .filter(|part| !matches!(part, Component::CurDir));
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(only)), None) => Some(only.to_string_lossy().into_owned()),
        _ => None,
    }
}

/// A text without its `%` comments. The same few lines as in `abstract-tex-includes` and in the app's
/// `bibliography.rs`: cheaper to repeat than to make public and explain.
fn without_comments(source: &str) -> String {
    let mut cleaned = String::with_capacity(source.len());
    let mut characters = source.chars();
    let mut in_comment = false;
    while let Some(character) = characters.next() {
        match character {
            '\n' => {
                in_comment = false;
                cleaned.push('\n');
            }
            _ if in_comment => {}
            '%' => in_comment = true,
            '\\' => {
                cleaned.push('\\');
                cleaned.extend(characters.next()); // `\%` is a percent sign
            }
            _ => cleaned.push(character),
        }
    }
    cleaned
}

/// Every command in a text that names a file, with its `{...}` arguments. `[...]` options are stepped over.
/// Of any other command only the name is: what is in its arguments is looked through like the rest.
fn commands_in(text: &str) -> Vec<(&str, Vec<&str>)> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        rest = &rest[at + 1..];
        let is_letter = |character: char| character.is_ascii_alphabetic();
        let command = &rest[..rest.find(|character| !is_letter(character)).unwrap_or(rest.len())];
        if command.is_empty() {
            // `\\` or `\%`: past that character too, so that `\\input` is not `\input`.
            let mut characters = rest.chars();
            characters.next();
            rest = characters.as_str();
            continue;
        }
        rest = &rest[command.len()..];
        if names_a_file(command).is_none() {
            continue;
        }
        rest = rest.strip_prefix('*').unwrap_or(rest);
        let mut arguments = Vec::new();
        while let Some(close) = closing(rest.trim_start()) {
            let open = rest.trim_start();
            if open.starts_with('{') {
                arguments.push(&open[1..close]);
            }
            rest = &open[close + 1..];
        }
        found.push((command, arguments));
    }
    found
}

/// How far an argument may go. One that is never closed is not looked for to the end of the file: a file
/// with thousands of them would never be done with.
const LONGEST_ARGUMENT: usize = 2_000;

/// Where the bracket that opens `text` closes, if one does.
fn closing(text: &str) -> Option<usize> {
    let (open, close) = match text.chars().next()? {
        '{' => ('{', '}'),
        '[' => ('[', ']'),
        _ => return None,
    };
    let mut depth = 0;
    for (at, character) in text.char_indices().take(LONGEST_ARGUMENT) {
        if character == open {
            depth += 1;
        } else if character == close {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// Copy `file` and the files it names into a staging folder in `beside`.
pub fn copy_document(file: &Path, beside: &Path) -> Result<Staged, CopyError> {
    let staged = Staged::begin(beside).map_err(CopyError::Write)?;
    let mut files = files_named_by(file);
    files.insert(0, file.to_path_buf());
    for original in files {
        let name = original.file_name().unwrap_or_default();
        let unread = |source| CopyError::Read {
            name: name.to_string_lossy().into_owned(),
            source,
        };
        // Not `fs::copy`, which copies the permissions too: what is read-only where it was found would be
        // a document that cannot be written.
        let mut from = fs::File::open(&original).map_err(unread)?;
        let mut to = fs::File::create_new(staged.document().join(name)).map_err(CopyError::Write)?;
        io::copy(&mut from, &mut to).map_err(CopyError::Write)?;
    }
    Ok(staged)
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

    /// What `paper.tex` names, sorted.
    fn named(files: &[(&str, &str)]) -> Vec<String> {
        let place = folder(files);
        names_in(&files_named_by(&place.path().join("paper.tex")), place.path())
    }

    fn names_in(files: &[PathBuf], folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = files
            .iter()
            .map(|file| {
                assert_eq!(file.parent(), Some(folder), "beside it");
                file.file_name().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_way_of_naming_a_file_travels() {
        for (text, name) in [
            ("\\input{a}", "a.tex"),
            ("\\input{a.tex}", "a.tex"),
            ("\\input a", "a.tex"),
            ("\\input{table.tikz}", "table.tikz"),
            ("\\include{a}", "a.tex"),
            ("\\subfile{a}", "a.tex"),
            ("\\import{./}{a}", "a.tex"),
            ("\\subimport{}{a}", "a.tex"),
            ("\\subimport*{.}{a}", "a.tex"),
            ("\\InputIfFileExists{a}{}{}", "a.tex"),
            ("\\includegraphics{a.png}", "a.png"),
            ("\\includegraphics[width=\\textwidth]{a}", "a.pdf"),
            ("\\includegraphics{a}", "a.JPG"),
            ("\\includegraphics*[scale=2] {./a}", "a.eps"),
            ("\\graphicspath{{figures/}{./}}\\includegraphics{a}", "a.jpeg"),
            ("\\includegraphics{\"my fig\".png}", "my fig.png"),
            ("\\includegraphics{{a.b}.png}", "a.b.png"),
            ("\\includepdf[pages=-]{a}", "a.pdf"),
            ("\\includesvg{a}", "a.svg"),
            ("\\bibliography{x, a ,y}", "a.bib"),
            ("\\bibliography{a.bib}", "a.bib"),
            ("\\addbibresource[glob]{a.bib}", "a.bib"),
            ("\\addglobalbib{a.bib}", "a.bib"),
            ("\\addsectionbib{a.bib}", "a.bib"),
            ("\\bibliographystyle{a}", "a.bst"),
            ("\\documentclass[12pt]{a}", "a.cls"),
            ("\\LoadClass{a}", "a.cls"),
            ("\\LoadClassWithOptions{a}", "a.cls"),
            ("\\usepackage{x,a}", "a.sty"),
            ("\\usepackage[b]\n  {a}", "a.sty"),
            ("\\RequirePackage{a}[2020/01/01]", "a.sty"),
            ("\\RequirePackageWithOptions{a}", "a.sty"),
            ("\\lstinputlisting[language=C]{a.c}", "a.c"),
            ("\\verbatiminput{a.txt}", "a.txt"),
            ("\\VerbatimInput{a.txt}", "a.txt"),
            ("\\inputminted{python}{a.py}", "a.py"),
            ("\\inputminted[linenos]{python}{a.py}", "a.py"),
        ] {
            let found = named(&[
                ("paper.tex", text),
                (name, ""),
                ("other.pdf", ""),
                ("other.tex", ""),
            ]);
            assert_eq!(found, [name], "{text}");
        }
    }

    #[test]
    fn one_file_named_twice_travels_once_and_the_document_itself_never() {
        let text = "\\input{a}\\input{a.tex}\\input{paper}\\includegraphics{f}";
        let found = named(&[
            ("paper.tex", text),
            ("a.tex", "\\input{paper}"),
            ("f.pdf", ""),
            ("f.png", ""),
        ]);
        // Which of two figures the engine would take is its business.
        assert_eq!(found, ["a.tex", "f.pdf", "f.png"]);
    }

    #[test]
    fn what_a_chapter_or_a_class_names_travels_too() {
        let found = named(&[
            ("paper.tex", "\\documentclass{mine}\\include{one}"),
            ("mine.cls", "\\RequirePackage{helper}"),
            ("helper.sty", "\\input{defs.def}"),
            ("defs.def", "\\input{deep}"),
            ("deep.tex", ""),
            ("one.tex", "\\input{two}\\includegraphics{f}"),
            ("two.tex", "\\input{one}"),
            ("f.png", "\\input{unread}"),
            ("unread.tex", ""),
        ]);
        assert_eq!(
            found,
            [
                "deep.tex",
                "defs.def",
                "f.png",
                "helper.sty",
                "mine.cls",
                "one.tex",
                "two.tex"
            ]
        );
    }

    #[test]
    fn only_ordinary_files_in_the_same_folder_travel() {
        let place = folder(&[
            (
                "in/paper.tex",
                "\\input{../x}\\input{sub/x}\\input{folder}\\input{gone}\\input{}\\input{.}\\input{..}\\input{\\mine}\\import{sub}{y}",
            ),
            ("x.tex", ""),
            ("in/sub/x.tex", ""),
            ("in/sub.tex", ""),
            ("in/y.tex", ""),
            ("in/folder/y", ""),
        ]);
        let paper = place.path().join("in/paper.tex");
        let absolute = place.path().join("x.tex");
        let text = fs::read_to_string(&paper).unwrap() + &format!("\\input{{{}}}", absolute.display());
        fs::write(&paper, text).unwrap();
        assert_eq!(files_named_by(&paper), Vec::<PathBuf>::new());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_does_not_travel() {
        let place = folder(&[("paper.tex", "\\input{secret}"), ("in/secret.txt", "")]);
        std::os::unix::fs::symlink(
            place.path().join("in/secret.txt"),
            place.path().join("secret.tex"),
        )
        .unwrap();
        assert_eq!(
            files_named_by(&place.path().join("paper.tex")),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn what_would_make_a_repository_of_the_copy_does_not_travel() {
        let text = "\\verbatiminput{.git}\\verbatiminput{HEAD}\\verbatiminput{commondir}";
        let found = named(&[
            ("paper.tex", text),
            (".git", "gitdir: x"),
            ("HEAD", ""),
            ("CommonDir", "x"),
        ]);
        assert_eq!(found, ["HEAD"]);
    }

    #[test]
    fn a_name_in_a_comment_does_not_travel() {
        let files = [
            (
                "paper.tex",
                "% \\input{a}\n\\% \\input{b} % \\includegraphics{c}\n\\\\includegraphics{c}",
            ),
            ("a.tex", ""),
            ("b.tex", ""),
            ("c.png", ""),
        ];
        assert_eq!(named(&files), ["b.tex"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn of_two_names_that_differ_in_case_the_one_that_was_written_travels() {
        for name in ["a", "A"] {
            let text = format!("\\input{{{name}}}");
            let found = named(&[("paper.tex", &text), ("a.tex", ""), ("A.tex", "")]);
            assert_eq!(found, [format!("{name}.tex")]);
        }
    }

    #[test]
    fn a_file_too_big_to_be_a_document_is_not_looked_through() {
        let place = folder(&[("paper.tex", "\\input{a}"), ("a.tex", "")]);
        let paper = place.path().join("paper.tex");
        assert_eq!(files_named_by(&paper).len(), 1);
        fs::OpenOptions::new()
            .write(true)
            .open(&paper)
            .unwrap()
            .set_len(MOST_TEXT + 1)
            .unwrap();
        assert_eq!(files_named_by(&paper).len(), 0);
    }

    #[test]
    fn a_file_in_an_old_encoding_is_read_for_its_names() {
        let place = folder(&[("fig.png", "")]);
        let paper = place.path().join("paper.tex");
        fs::write(&paper, b"caf\xe9 \\includegraphics{fig}").unwrap();
        assert_eq!(names_in(&files_named_by(&paper), place.path()), ["fig.png"]);
    }

    #[test]
    fn a_file_made_to_take_forever_does_not() {
        let started = std::time::Instant::now();
        for text in [
            "\\a{",
            "\\includegraphics{",
            "\\includegraphics[",
            "\\input{x}",
            "\\input{",
        ] {
            let text = text.repeat(20_000) + "\\includegraphics{a}";
            let others: Vec<String> = (0..500).map(|number| format!("{number}.txt")).collect();
            let mut files: Vec<(&str, &str)> = others.iter().map(|name| (name.as_str(), "")).collect();
            files.extend([("paper.tex", text.as_str()), ("a.pdf", "")]);
            named(&files);
        }
        assert!(started.elapsed().as_secs() < 15, "{:?}", started.elapsed());
        let nested = "\\section{".repeat(20_000) + "\\includegraphics{a}";
        assert_eq!(named(&[("paper.tex", &nested), ("a.pdf", "")]), ["a.pdf"]);
    }

    #[test]
    fn a_name_inside_another_commands_argument_travels() {
        let text = "\\section{One \\includegraphics{a}}\\foreach{\\verbatiminput{b}}";
        let found = named(&[("paper.tex", text), ("a.pdf", ""), ("b", "")]);
        assert_eq!(found, ["a.pdf", "b"]);
    }

    /// Every file in a folder with its bytes, sorted.
    fn contents(folder: &Path) -> Vec<(String, Vec<u8>)> {
        let mut all: Vec<(String, Vec<u8>)> = fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                (name, fs::read(entry.path()).unwrap())
            })
            .collect();
        all.sort();
        all
    }

    #[test]
    fn the_original_is_untouched_and_the_copy_is_whole() {
        let files = [
            (
                "paper.tex",
                "caf\u{e9} \\includegraphics{fig}\\bibliography{refs}",
            ),
            ("fig.png", "\0\u{1}"),
            ("refs.bib", "@book{x}"),
        ];
        let place = folder(&files);
        fs::write(place.path().join("other.txt"), "stays").unwrap();
        let before = contents(place.path());
        let beside = tempfile::tempdir().unwrap();
        let destination = beside.path().join("new/paper");
        copy_document(&place.path().join("paper.tex"), destination.parent().unwrap())
            .unwrap()
            .place(&destination)
            .unwrap();
        assert_eq!(contents(place.path()), before);
        let copied: Vec<_> = before
            .into_iter()
            .filter(|(name, _)| name != "other.txt")
            .collect();
        assert_eq!(contents(&destination), copied);
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_of_what_is_read_only_can_be_written_and_not_run() {
        use std::os::unix::fs::PermissionsExt;
        let place = folder(&[("paper.tex", "")]);
        let paper = place.path().join("paper.tex");
        fs::set_permissions(&paper, fs::Permissions::from_mode(0o555)).unwrap();
        let destination = place.path().join("new");
        copy_document(&paper, place.path())
            .unwrap()
            .place(&destination)
            .unwrap();
        let mode = fs::metadata(destination.join("paper.tex"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o311, 0o200);
    }

    #[test]
    fn a_copy_that_fails_leaves_nothing_behind() {
        let place = folder(&[("a file", "")]);
        let beside = tempfile::tempdir().unwrap();
        let answer = copy_document(&place.path().join("gone.tex"), beside.path());
        assert!(matches!(answer, Err(CopyError::Read { name, .. }) if name == "gone.tex"));
        assert_eq!(fs::read_dir(beside.path()).unwrap().count(), 0);

        let paper = place.path().join("a file");
        let nowhere = place.path().join("a file/inside");
        assert!(matches!(
            copy_document(&paper, &nowhere),
            Err(CopyError::Write(_))
        ));
    }
}
