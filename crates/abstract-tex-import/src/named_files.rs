//! The files a document names, when they are beside it: what has to travel with a `.tex` that is taken out
//! of Downloads.
//!
//! **What it must never do:** follow a name out of the folder, or a link anywhere; or change the originals.
//! The names are whatever a file from a stranger says, and what they name is copied.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::staged::Staged;

/// Why a copy was not made.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("{name} could not be read")]
    Read { name: String, source: io::Error },
    #[error("the copy could not be written")]
    Write(#[source] io::Error),
    #[error("it was stopped")]
    Stopped,
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

/// Every command that names a file: which of its arguments does, and the endings it may leave out. Not
/// `scan_includes`, which is for what a document is made of and takes code for one piece: here a file that
/// may be used is one that travels.
fn names_a_file(command: &str) -> Option<(usize, &'static [&'static str])> {
    Some(match command {
        "input" | "include" | "subfile" | "subfileinclude" | "InputIfFileExists" => (0, &["tex"]),
        "includestandalone" => (0, &["tex", "pdf"]),
        "includegraphics" | "includesvg" | OVERPIC => (0, PICTURES),
        "includepdf" => (0, &["pdf"]),
        "bibliography" | "addbibresource" | "addglobalbib" | "addsectionbib" => (0, &["bib"]),
        "bibliographystyle" => (0, &["bst"]),
        "documentclass" | "LoadClass" | "LoadClassWithOptions" => (0, &["cls"]),
        "usepackage" | "RequirePackage" | "RequirePackageWithOptions" => (0, &["sty"]),
        "usetheme" | "usecolortheme" | "usefonttheme" | "useinnertheme" | "useoutertheme" => (0, &["sty"]),
        "lstinputlisting" | "verbatiminput" | "VerbatimInput" => (0, &["tex"]),
        "inputminted" | "import" | "subimport" => (1, &["tex"]),
        _ => return None,
    })
}

/// The one environment that names a file. It is taken for a command of this name.
const OVERPIC: &str = "overpic";

/// What beamer puts in front of a theme's name to have its file's.
fn theme_prefix(command: &str) -> &'static str {
    match command {
        "usetheme" => "beamertheme",
        "usecolortheme" => "beamercolortheme",
        "usefonttheme" => "beamerfonttheme",
        "useinnertheme" => "beamerinnertheme",
        "useoutertheme" => "beameroutertheme",
        _ => "",
    }
}

/// Every name a text has, whether or not there is such a file.
fn names_in(text: &str) -> Vec<Wanted> {
    let mut wanted = Vec::new();
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
        // A list of names, or one with a comma in it.
        let names = argument
            .split(',')
            .chain(argument.contains(',').then_some(*argument));
        for name in names {
            let name = name.replace(['"', '{', '}'], "");
            let path = Path::new(folder).join(format!("{}{}", theme_prefix(command), name.trim()));
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
        let command = match rest.strip_prefix("{overpic}") {
            Some(behind) if command == "begin" => {
                rest = behind;
                OVERPIC
            }
            _ => command,
        };
        let Some((which, _)) = names_a_file(command) else {
            continue;
        };
        rest = rest.strip_prefix('*').unwrap_or(rest);
        // `<2->` in beamer: on which slides.
        if let Some(end) = closing(rest, '<', '>') {
            rest = &rest[end + 1..];
        }
        let mut arguments = Vec::new();
        // Only up to the one that is the name: what follows it is looked through like the rest.
        while arguments.len() <= which {
            let open = rest.trim_start();
            if let Some(end) = closing(open, '{', '}') {
                arguments.push(&open[1..end]);
                rest = &open[end + 1..];
            } else if let Some(end) = closing(open, '[', ']') {
                rest = &open[end + 1..];
            } else {
                break;
            }
        }
        if command == "input" && arguments.is_empty() {
            arguments.extend(name_without_braces(rest.trim_start()));
        }
        found.push((command, arguments));
    }
    found
}

/// The name in `\input intro` or `\input "my intro"`.
fn name_without_braces(text: &str) -> Option<&str> {
    let quoted = text.strip_prefix('"');
    let text = quoted.unwrap_or(text);
    let ends_it = |character: char| match quoted {
        Some(_) => character == '"',
        None => character.is_whitespace() || matches!(character, '\\' | '{' | '}'),
    };
    let mut characters = text.char_indices().take(LONGEST_ARGUMENT);
    match characters.find(|(_, character)| ends_it(*character)) {
        Some((end, _)) => Some(&text[..end]),
        // It goes to the end of the text; or on and on, and is no name.
        None => (text.len() < LONGEST_ARGUMENT).then_some(text),
    }
}

/// How far an argument may go. One that is never closed is not looked for to the end of the file: a file
/// with thousands of them would never be done with.
const LONGEST_ARGUMENT: usize = 2_000;

/// Where the bracket that opens `text` closes, if `text` starts with one and it is closed.
fn closing(text: &str, open: char, close: char) -> Option<usize> {
    if !text.starts_with(open) {
        return None;
    }
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

/// Copy `file` and the files it names into a staging folder in `beside`. `stopped` is asked now and then,
/// and ends it.
pub fn copy_document(file: &Path, beside: &Path, stopped: &dyn Fn() -> bool) -> Result<Staged, CopyError> {
    let staged = Staged::begin(beside).map_err(CopyError::Write)?;
    let mut files = files_named_by(file);
    files.insert(0, file.to_path_buf());
    let mut buffer = vec![0; 64 * 1024];
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
        // Nor `io::copy`, which cannot be stopped: a file can be gigabytes.
        loop {
            if stopped() {
                return Err(CopyError::Stopped);
            }
            let read = from.read(&mut buffer).map_err(unread)?;
            if read == 0 {
                break;
            }
            to.write_all(&buffer[..read]).map_err(CopyError::Write)?;
        }
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
            ("\\includegraphics<1>{a}", "a.png"),
            ("\\includegraphics<2->[width=3cm]{a}", "a.png"),
            ("\\input{\"my chapter\"}", "my chapter.tex"),
            ("\\input \"my chapter\"", "my chapter.tex"),
            ("\\include{a,b}", "a,b.tex"),
            ("{\\input a}", "a.tex"),
            ("\\input a\\relax", "a.tex"),
            ("\\input\n  a", "a.tex"),
            ("\\subfileinclude{a}", "a.tex"),
            ("\\includestandalone[width=1cm]{a}", "a.tex"),
            ("\\includestandalone{a}", "a.pdf"),
            ("\\begin{overpic}[width=1cm]{a}", "a.png"),
            ("\\usetheme[progress]{Mine}", "beamerthemeMine.sty"),
            ("\\usecolortheme{Mine}", "beamercolorthemeMine.sty"),
            ("\\usefonttheme{Mine}", "beamerfontthemeMine.sty"),
            ("\\useinnertheme{Mine}", "beamerinnerthemeMine.sty"),
            ("\\useoutertheme{Mine}", "beamerouterthemeMine.sty"),
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
    fn what_follows_a_files_name_is_looked_through_too() {
        for text in [
            "\\includegraphics[width=2cm]{a}\n\n{\\Large University \\includegraphics{b}}",
            "\\bibliography{x}\n{\\small \\includegraphics{a}\\includegraphics{b}}",
            "\\InputIfFileExists{x}{\\includegraphics{a}}{\\includegraphics{b}}",
            "\\InputIfFileExists{x}{\\input{a.pdf}}{\\input{b.pdf}}",
            "\\includegraphics{a}[\\includegraphics{b}]",
            "\\begin{figure}[h]\n{\\includegraphics{a}}\\input b.pdf",
        ] {
            let found = named(&[("paper.tex", text), ("a.pdf", ""), ("b.pdf", "")]);
            assert_eq!(found, ["a.pdf", "b.pdf"], "{text}");
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
            "\\input ",
            "\\input \"",
            "\\includegraphics<",
            "\\InputIfFileExists{",
            "\\import{",
            "\\begin{overpic}[",
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
        copy_document(
            &place.path().join("paper.tex"),
            destination.parent().unwrap(),
            &|| false,
        )
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
        copy_document(&paper, place.path(), &|| false)
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
    fn a_stopped_copy_leaves_nothing_behind() {
        let big = "x".repeat(300_000);
        let place = folder(&[("paper.tex", "\\input{a.txt}"), ("a.txt", &big)]);
        let beside = tempfile::tempdir().unwrap();
        // Stopped at the first question, at the second, ...: before anything, between files, in a file.
        for questions in 0..5 {
            let asked = std::cell::Cell::new(0);
            let answer = copy_document(&place.path().join("paper.tex"), beside.path(), &|| {
                asked.set(asked.get() + 1);
                asked.get() > questions
            });
            assert!(matches!(answer, Err(CopyError::Stopped)), "{questions}");
            assert_eq!(fs::read_dir(beside.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn a_copy_that_fails_leaves_nothing_behind() {
        let place = folder(&[("a file", "")]);
        let beside = tempfile::tempdir().unwrap();
        let answer = copy_document(&place.path().join("gone.tex"), beside.path(), &|| false);
        assert!(matches!(answer, Err(CopyError::Read { name, .. }) if name == "gone.tex"));
        assert_eq!(fs::read_dir(beside.path()).unwrap().count(), 0);

        let paper = place.path().join("a file");
        let nowhere = place.path().join("a file/inside");
        assert!(matches!(
            copy_document(&paper, &nowhere, &|| false),
            Err(CopyError::Write(_))
        ));
    }
}
