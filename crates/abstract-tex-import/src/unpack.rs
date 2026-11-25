//! Unpacking a `.zip` from anywhere: Overleaf, a colleague, a stranger.
//!
//! **What it must never do:** write outside its own staging folder, believe a size the archive states,
//! make a link, or bring in a permission, a repository (whatever it is called) or a build from another
//! computer. An archive is taken whole or refused whole, and one that is refused leaves nothing behind.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use zip::result::ZipError;
use zip::{CompressionMethod, ZipArchive};

use crate::staged::Staged;

/// How much an archive may hold.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Files and folders.
    pub entries: usize,
    /// Unpacked.
    pub bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 20_000,
            bytes: 2_000_000_000,
        }
    }
}

/// Why an archive was refused. The names are the archive's own.
#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("not a zip, or a damaged one")]
    NotAZip,
    #[error("it needs a password")]
    Locked,
    #[error("packed in a way that cannot be read")]
    Unsupported,
    #[error("it holds too many files")]
    TooMany,
    #[error("it holds too much")]
    TooBig,
    #[error("{0:?} cannot be unpacked safely")]
    UnsafeName(String),
    #[error("{0:?} and {1:?} would be one file")]
    SameName(String, String),
    #[error("there is no .tex file in it")]
    NoTex,
    #[error("it was stopped")]
    Stopped,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Whose rules a name has to follow. A value and not `cfg!`, so that a test can ask about either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Other,
}

impl Platform {
    pub const HERE: Platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Other
    };
}

/// The longest name an entry may have, in bytes, and how deep it may lie. No disk takes much longer ones,
/// and removing very deep folders again can fail, for want of so many open ones at a time.
const LONGEST_NAME: usize = 1_000;
const DEEPEST: usize = 60;

/// The path inside the folder that an entry's name means, or `None` if it must not be written.
///
/// Not `zip`'s own `enclosed_name`: that quietly turns `/a.tex` into `a.tex`. A name that leads elsewhere
/// is no mistake to correct, it is a reason to refuse the archive.
pub fn safe_path(name: &str, platform: Platform) -> Option<PathBuf> {
    // Old Windows programs write `\` between the parts, whatever the rules say.
    let is_separator = |character| matches!(character, '/' | '\\');
    if name.is_empty() || name.len() > LONGEST_NAME || name.starts_with(is_separator) {
        return None;
    }
    let mut path = PathBuf::new();
    let mut depth = 0;
    for part in name.split(is_separator) {
        if matches!(part, "" | ".") {
            continue;
        }
        if part == ".." || part.contains('\0') || (platform == Platform::Windows && !windows_keeps(part)) {
            return None;
        }
        depth += 1;
        if depth > DEEPEST {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// A name as it is said to the person. A refused one can be as long as a page.
fn said(name: &str) -> String {
    const MOST: usize = 100;
    let mut short: String = name.chars().take(MOST).collect();
    if short.len() < name.len() {
        short.push('…');
    }
    short
}

/// Whether Windows makes a file of exactly this name. It forbids some characters, drops a dot or a space
/// at the end, and takes `CON`, `nul.tex` and their like for devices. `src-tauri/src/git.rs` has the same
/// rule for a folder's name.
fn windows_keeps(name: &str) -> bool {
    let is_forbidden = |character: char| {
        character.is_control() || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
    };
    if name.ends_with(['.', ' ']) || name.contains(is_forbidden) {
        return false;
    }
    let before_the_first_dot = name.split('.').next().unwrap_or(name);
    let device = before_the_first_dot.trim_end().to_uppercase();
    let is_numbered = |port: &str| {
        let number = device.strip_prefix(port).unwrap_or_default();
        number.chars().count() == 1 && number.chars().all(|digit| "0123456789¹²³".contains(digit))
    };
    let is_device = matches!(
        device.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    );
    !is_device && !is_numbered("COM") && !is_numbered("LPT")
}

/// What stays out, at any depth: another computer's repository (its hooks are programs), our own folder
/// (its PDF and its saved versions are not this document's), and what macOS leaves in every zip.
fn stays_out(path: &Path) -> bool {
    path.components().any(|part| {
        let name = folded(Path::new(part.as_os_str()));
        matches!(name.as_str(), ".git" | ".abstract-tex" | "__macosx" | ".ds_store")
    })
}

/// A path as a disk compares it that ignores case: to macOS the long `ſ` is an `s` too, and only going by
/// way of the capital letters says so.
fn folded(path: &Path) -> String {
    path.to_string_lossy().to_uppercase().to_lowercase()
}

/// Leave out every `HEAD` that would make a repository of the folder it is in. Git goes by what is in a
/// folder and not by its name, and one from a stranger can say that the files it keeps are all those in
/// the person's home. Without `HEAD` it is no repository, and the rest are only files.
fn leave_out_hidden_repositories(wanted: &mut Vec<Wanted>, paths: &Paths) {
    let mut suspects = HashSet::new();
    for taken in paths.taken.values() {
        let (Some(folder), Some(name)) = (taken.path.parent(), taken.path.file_name()) else {
            continue;
        };
        // `commondir` is a file that says where the other two are.
        if matches!(folded(Path::new(name)).as_str(), "objects" | "refs" | "commondir") {
            suspects.insert(folded(folder));
        }
    }
    wanted.retain(|entry| {
        let is_head = entry
            .path
            .file_name()
            .is_some_and(|name| folded(Path::new(name)) == "head");
        let is_suspect = entry
            .path
            .parent()
            .is_some_and(|folder| suspects.contains(&folded(folder)));
        entry.is_folder || !is_head || !is_suspect
    });
}

/// The paths of one archive, told apart as Windows and macOS do: without regard to case.
#[derive(Default)]
struct Paths {
    taken: HashMap<String, Taken>,
}

struct Taken {
    /// As the archive has it.
    path: PathBuf,
    is_folder: bool,
}

impl Paths {
    /// Take `path` and the folders it is in. Two entries may share a folder, and nothing else.
    fn take(&mut self, path: &Path, is_folder: bool) -> Result<(), UnpackError> {
        for (steps_up, taken) in path.ancestors().enumerate() {
            if taken.as_os_str().is_empty() {
                break; // the document's own folder
            }
            let is_folder = is_folder || steps_up > 0;
            let key = taken.to_string_lossy().to_lowercase();
            match self.taken.get(&key) {
                Some(other) if is_folder && other.is_folder => break, // and so are those it is in
                Some(other) => {
                    return Err(UnpackError::SameName(shown(&other.path), shown(taken)));
                }
                None => {
                    let path = taken.to_path_buf();
                    self.taken.insert(key, Taken { path, is_folder });
                }
            }
        }
        Ok(())
    }
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// An entry that will be written.
struct Wanted {
    index: usize,
    path: PathBuf,
    is_folder: bool,
}

/// Unpack `zip` into a staging folder in `beside`. `stopped` is asked now and then, and ends it.
pub fn unpack(
    zip: &Path,
    beside: &Path,
    limits: &Limits,
    stopped: &dyn Fn() -> bool,
) -> Result<Staged, UnpackError> {
    let mut archive = ZipArchive::new(io::BufReader::new(fs::File::open(zip)?)).map_err(unreadable)?;
    // Every entry is judged before the first byte is written.
    let wanted = judge(&archive, limits)?;
    if stopped() {
        return Err(UnpackError::Stopped);
    }
    let mut staged = Staged::begin(beside)?;
    let mut room = limits.bytes;
    for Wanted {
        index,
        path,
        is_folder,
    } in wanted
    {
        let target = staged.document().join(path);
        if is_folder {
            fs::create_dir_all(target)?;
            continue;
        }
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder)?;
        }
        let packed = archive.by_index(index).map_err(unreadable)?;
        write(packed, &target, &mut room, stopped)?;
    }
    staged.remove_wrapping_folder()?;
    Ok(staged)
}

/// What is to be written, if all of it may be.
fn judge(archive: &ZipArchive<impl Read + io::Seek>, limits: &Limits) -> Result<Vec<Wanted>, UnpackError> {
    let mut paths = Paths::default();
    let mut wanted = Vec::new();
    let mut has_tex = false;
    for index in 0..archive.len() {
        let entry = archive.by_index_data(index).map_err(unreadable)?;
        let name = entry.name().map_err(unreadable)?;
        let Some(path) = safe_path(&name, Platform::HERE) else {
            return Err(UnpackError::UnsafeName(said(&name)));
        };
        let is_folder = entry.is_dir();
        if stays_out(&path) || (is_folder && path.as_os_str().is_empty()) {
            continue;
        }
        // A link would be a way out of the folder; a pipe or a device has no business in a document.
        let kind = entry.unix_mode().unwrap_or(0) & 0o170_000;
        if path.as_os_str().is_empty() || !matches!(kind, 0 | 0o040_000 | 0o100_000) {
            return Err(UnpackError::UnsafeName(said(&name)));
        }
        if entry.encrypted() {
            return Err(UnpackError::Locked);
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(UnpackError::Unsupported);
        }
        paths.take(&path, is_folder)?;
        if paths.taken.len() > limits.entries {
            return Err(UnpackError::TooMany);
        }
        has_tex |= !is_folder
            && path
                .extension()
                .is_some_and(|ending| ending.eq_ignore_ascii_case("tex"));
        wanted.push(Wanted {
            index,
            path,
            is_folder,
        });
    }
    if !has_tex {
        return Err(UnpackError::NoTex);
    }
    leave_out_hidden_repositories(&mut wanted, &paths);
    Ok(wanted)
}

/// Write one file. What it takes is counted here and nowhere else: the sizes an archive states are
/// claims, and a few bytes can unpack to gigabytes.
fn write(
    mut packed: impl Read,
    target: &Path,
    room: &mut u64,
    stopped: &dyn Fn() -> bool,
) -> Result<(), UnpackError> {
    // `create_new` refuses a file that is there. `Paths` has seen to it, except where a disk takes two
    // names for one that we think differ: macOS does, with accents written in two ways.
    let mut file = match fs::OpenOptions::new().write(true).create_new(true).open(target) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let name = target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            return Err(UnpackError::SameName(name.clone(), name));
        }
        opened => opened?,
    };
    let mut buffer = [0; 64 * 1024];
    loop {
        if stopped() {
            return Err(UnpackError::Stopped);
        }
        let read = packed.read(&mut buffer).map_err(damaged)?;
        if read == 0 {
            return Ok(());
        }
        *room = room.checked_sub(read as u64).ok_or(UnpackError::TooBig)?;
        file.write_all(&buffer[..read])?;
    }
}

fn unreadable(error: ZipError) -> UnpackError {
    match error {
        ZipError::Io(error) => damaged(error),
        ZipError::UnsupportedArchive(reason) if reason == ZipError::PASSWORD_REQUIRED => UnpackError::Locked,
        ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_) => {
            UnpackError::Unsupported
        }
        _ => UnpackError::NotAZip,
    }
}

/// Reading failed: by the archive's fault, or by the disk's.
fn damaged(error: io::Error) -> UnpackError {
    match error.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof => {
            UnpackError::NotAZip
        }
        _ => UnpackError::Io(error),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;

    const TEX: &str = "\\documentclass{article}";

    /// An archive with these files. A name that ends in `/` is a folder.
    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// Change bytes of every entry's two headers: at `local` in the one before its contents, which lacks
    /// some of what the other says, and at `central` in the list at the end.
    fn patched(mut bytes: Vec<u8>, local: Option<usize>, central: usize, value: &[u8]) -> Vec<u8> {
        for (signature, offset) in [(b"PK\x03\x04", local), (b"PK\x01\x02", Some(central))] {
            let Some(offset) = offset else { continue };
            let starts: Vec<usize> = (0..bytes.len() - 4)
                .filter(|at| &bytes[*at..at + 4] == signature)
                .collect();
            for start in starts {
                bytes[start + offset..start + offset + value.len()].copy_from_slice(value);
            }
        }
        bytes
    }

    /// Unpacked where nothing else is. The folder is handed back so that it can be looked at.
    fn unpacked(bytes: &[u8], limits: &Limits) -> (TempDir, Result<Staged, UnpackError>) {
        let place = tempfile::tempdir().unwrap();
        let zip = place.path().join("it.zip");
        fs::write(&zip, bytes).unwrap();
        let beside = place.path().join("beside");
        fs::create_dir(&beside).unwrap();
        let answer = unpack(&zip, &beside, limits, &|| false);
        (place, answer)
    }

    /// Why it was refused; and nothing is left of it.
    fn refused_within(bytes: &[u8], limits: &Limits) -> UnpackError {
        let (place, answer) = unpacked(bytes, limits);
        let error = answer.unwrap_err();
        let left = fs::read_dir(place.path().join("beside")).unwrap().count();
        assert_eq!(left, 0, "left behind after {error:?}");
        error
    }

    fn refused(bytes: &[u8]) -> UnpackError {
        refused_within(bytes, &Limits::default())
    }

    /// Every file an archive became, sorted, with forward slashes.
    fn files_of(bytes: &[u8]) -> Vec<String> {
        let (place, answer) = unpacked(bytes, &Limits::default());
        let destination = place.path().join("beside/document");
        answer.unwrap().place(&destination).unwrap();
        assert_eq!(
            fs::read_dir(place.path().join("beside")).unwrap().count(),
            1,
            "only the document"
        );
        let mut files = Vec::new();
        list(&destination, &destination, &mut files);
        files.sort();
        files
    }

    fn list(root: &Path, folder: &Path, files: &mut Vec<String>) {
        for entry in fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                list(root, &path, files);
            } else {
                let inside = path.strip_prefix(root).unwrap().to_string_lossy();
                files.push(inside.replace('\\', "/"));
            }
        }
    }

    #[test]
    fn a_name_that_leads_elsewhere_is_not_written() {
        for name in [
            "../x.tex",
            "a/../../x.tex",
            "a/../x.tex",
            "/x.tex",
            "\\x.tex",
            "a\\..\\..\\x.tex",
            "a/\0.tex",
            "",
        ] {
            assert_eq!(safe_path(name, Platform::Windows), None, "{name:?}");
            assert_eq!(safe_path(name, Platform::Other), None, "{name:?}");
        }
        for name in [
            "C:/x.tex",
            "C:x.tex",
            "CON.tex",
            "a/nul",
            "a/Com1.tex/b",
            "lpt9",
            "COM¹.tex",
            "conin$",
            "nul .tex",
            "x.tex ",
            "x.",
            "a:b.tex",
            "a?.tex",
            "a\t.tex",
            "a|b",
            "a<b",
            "a>b",
            "a*b",
            "a\"b",
        ] {
            assert_eq!(safe_path(name, Platform::Windows), None, "{name:?}");
            assert!(safe_path(name, Platform::Other).is_some(), "{name:?}");
        }
    }

    #[test]
    fn a_name_that_is_too_long_or_too_deep_is_not_written() {
        for platform in [Platform::Windows, Platform::Other] {
            assert!(safe_path(&"a/".repeat(60), platform).is_some());
            assert_eq!(safe_path(&"a/".repeat(61), platform), None);
            assert!(safe_path(&"a".repeat(1_000), platform).is_some());
            assert_eq!(safe_path(&"a".repeat(1_001), platform), None);
        }
        // And refused at once: every folder it is in would be kept in memory with its whole path.
        let bytes = archive(&[("main.tex", TEX), (&("a/".repeat(32_000) + "x"), "")]);
        match refused(&bytes) {
            UnpackError::UnsafeName(said) => assert!(said.chars().count() <= 101, "{}", said.len()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_ordinary_name_is_kept() {
        for platform in [Platform::Windows, Platform::Other] {
            for (name, path) in [
                ("a\\b.tex", "a/b.tex"),
                ("./a//b.tex", "a/b.tex"),
                ("chapters/", "chapters"),
                ("./", ""),
                ("console.tex", "console.tex"),
                ("com10", "com10"),
                (".latexmkrc", ".latexmkrc"),
                ("três coelhos.tex", "três coelhos.tex"),
            ] {
                assert_eq!(safe_path(name, platform), Some(PathBuf::from(path)), "{name:?}");
            }
        }
        // Only Windows forbids it.
        assert_eq!(
            safe_path("fig:one.png", Platform::Other),
            Some(PathBuf::from("fig:one.png"))
        );
    }

    #[test]
    fn what_was_packed_is_unpacked() {
        let bytes = archive(&[
            ("main.tex", TEX),
            ("chapters/intro.tex", "hello"),
            ("fig.png", "\0"),
        ]);
        assert_eq!(files_of(&bytes), ["chapters/intro.tex", "fig.png", "main.tex"]);
        let (place, answer) = unpacked(&bytes, &Limits::default());
        answer.unwrap().place(&place.path().join("out")).unwrap();
        assert_eq!(
            fs::read_to_string(place.path().join("out/chapters/intro.tex")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn the_folder_to_unpack_beside_is_made_if_it_is_not_there() {
        let place = tempfile::tempdir().unwrap();
        fs::write(place.path().join("it.zip"), archive(&[("main.tex", TEX)])).unwrap();
        let beside = place.path().join("Documents/Abstract-Tex");
        let staged = unpack(&place.path().join("it.zip"), &beside, &Limits::default(), &|| {
            false
        });
        staged.unwrap().place(&beside.join("it")).unwrap();
        assert!(beside.join("it/main.tex").is_file());
    }

    #[test]
    fn a_hostile_name_is_refused_before_anything_is_written() {
        for name in ["../x.tex", "/x.tex", "a/../../x.tex"] {
            let bytes = archive(&[("main.tex", TEX), (name, "")]);
            assert!(
                matches!(refused(&bytes), UnpackError::UnsafeName(said) if said == name),
                "{name}"
            );
        }
    }

    #[test]
    fn the_folder_itself_may_be_named() {
        let bytes = archive(&[("./", ""), ("./main.tex", TEX)]);
        assert_eq!(files_of(&bytes), ["main.tex"]);
        let bytes = archive(&[("main.tex", TEX), (".", "a file")]);
        assert!(matches!(refused(&bytes), UnpackError::UnsafeName(name) if name == "."));
    }

    #[test]
    fn a_file_that_is_there_is_never_written_over() {
        let place = tempfile::tempdir().unwrap();
        let target = place.path().join("main.tex");
        fs::write(&target, "one").unwrap();
        let answer = write("two".as_bytes(), &target, &mut 100, &|| false);
        assert!(matches!(answer, Err(UnpackError::SameName(..))));
        assert_eq!(fs::read_to_string(&target).unwrap(), "one");
    }

    #[test]
    fn a_link_is_refused() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink("link.tex", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(matches!(refused(&bytes), UnpackError::UnsafeName(name) if name == "link.tex"));
    }

    #[test]
    fn what_is_neither_a_file_nor_a_folder_is_refused() {
        // The last two bytes of the list's "external attributes" are Unix's: 0o010644 is a pipe.
        let mode = 0o010644u16.to_le_bytes();
        let bytes = archive(&[("pipe.tex", TEX)]);
        let bytes = patched(bytes, None, 40, &mode);
        assert!(matches!(refused(&bytes), UnpackError::UnsafeName(_)));
    }

    #[test]
    fn a_password_is_said_to_be_one() {
        let bytes = patched(archive(&[("main.tex", TEX)]), Some(6), 8, &[1]);
        assert!(matches!(refused(&bytes), UnpackError::Locked));
        // Encrypted ones are often packed in a way we cannot read too. The password is what to say.
        let bytes = patched(bytes, Some(8), 10, &[12]);
        assert!(matches!(refused(&bytes), UnpackError::Locked));
    }

    #[test]
    fn a_method_we_cannot_read_is_said_to_be_one() {
        let bytes = patched(archive(&[("main.tex", TEX)]), Some(8), 10, &[12]);
        assert!(matches!(refused(&bytes), UnpackError::Unsupported));
    }

    #[test]
    fn too_many_entries() {
        let limits = Limits {
            entries: 2,
            ..Limits::default()
        };
        let three = archive(&[("a.tex", ""), ("b.tex", ""), ("c.tex", "")]);
        assert!(matches!(refused_within(&three, &limits), UnpackError::TooMany));
        // The folders a name asks for are counted too.
        let deep = archive(&[("a/b/c.tex", "")]);
        assert!(matches!(refused_within(&deep, &limits), UnpackError::TooMany));
        let (_place, two) = unpacked(&archive(&[("a.tex", ""), ("b.tex", "")]), &limits);
        assert!(two.is_ok());
    }

    #[test]
    fn the_size_is_counted_while_writing() {
        let limits = Limits {
            bytes: 10,
            ..Limits::default()
        };
        let eleven = archive(&[("a.tex", "123456"), ("b.tex", "12345")]);
        assert!(matches!(refused_within(&eleven, &limits), UnpackError::TooBig));
        let long = "x".repeat(300_000);
        let liar = patched(archive(&[("a.tex", &long)]), Some(22), 24, &1u32.to_le_bytes());
        // `zip` itself stops at the size that was stated, and calls the rest damage.
        assert!(matches!(refused(&liar), UnpackError::NotAZip));
        let honest = archive(&[("a.tex", &long)]);
        assert!(matches!(refused_within(&honest, &limits), UnpackError::TooBig));
        let (_place, ten) = unpacked(&archive(&[("a.tex", "123456"), ("b.tex", "1234")]), &limits);
        assert!(ten.is_ok());
    }

    #[test]
    fn two_names_differing_in_case_are_refused() {
        let bytes = archive(&[("Main.tex", TEX), ("main.tex", TEX)]);
        assert!(
            matches!(refused(&bytes), UnpackError::SameName(one, two) if one == "Main.tex" && two == "main.tex")
        );
        let bytes = archive(&[("a/b.tex", TEX), ("a\\b.tex", TEX)]);
        assert!(matches!(refused(&bytes), UnpackError::SameName(..)));
        let bytes = archive(&[("a", TEX), ("a/b.tex", TEX)]);
        assert!(
            matches!(refused(&bytes), UnpackError::SameName(..)),
            "a file and a folder"
        );
        let bytes = archive(&[("A/b.tex", TEX), ("a", TEX)]);
        assert!(
            matches!(refused(&bytes), UnpackError::SameName(..)),
            "a folder and a file"
        );
    }

    #[test]
    fn one_folder_named_twice_is_one_folder() {
        let bytes = archive(&[
            ("a/", ""),
            ("a/x.tex", TEX),
            ("a/y.tex", TEX),
            ("b/", ""),
            ("b.tex", ""),
        ]);
        assert_eq!(files_of(&bytes), ["a/x.tex", "a/y.tex", "b.tex"]);
    }

    #[test]
    fn what_is_not_a_zip_says_so() {
        assert!(matches!(refused(b"hello"), UnpackError::NotAZip));
        assert!(matches!(refused(b""), UnpackError::NotAZip));
        let whole = archive(&[("main.tex", &TEX.repeat(100))]);
        assert!(matches!(refused(&whole[..whole.len() / 2]), UnpackError::NotAZip));
    }

    #[test]
    fn a_damaged_one_says_so() {
        let mut bytes = archive(&[("main.tex", &"many different words 1234567890".repeat(9))]);
        bytes[45] ^= 0xff; // in the packed text
        assert!(matches!(refused(&bytes), UnpackError::NotAZip));
    }

    #[test]
    fn one_that_is_not_there_is_the_disks_matter() {
        let place = tempfile::tempdir().unwrap();
        let answer = unpack(
            &place.path().join("no.zip"),
            place.path(),
            &Limits::default(),
            &|| false,
        );
        assert!(matches!(answer, Err(UnpackError::Io(error)) if error.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn a_zip_with_no_tex_is_refused() {
        assert!(matches!(
            refused(&archive(&[("notes.txt", "")])),
            UnpackError::NoTex
        ));
        assert!(matches!(refused(&archive(&[("x.tex/", "")])), UnpackError::NoTex));
        assert!(matches!(refused(&archive(&[])), UnpackError::NoTex));
        // The one in our own folder stays out, so it does not count.
        let hidden = archive(&[(".abstract-tex/x.tex", TEX)]);
        assert!(matches!(refused(&hidden), UnpackError::NoTex));
        assert_eq!(files_of(&archive(&[("MAIN.TEX", "")])), ["MAIN.TEX"]);
    }

    #[test]
    fn a_stopped_archive_leaves_nothing_behind() {
        let place = tempfile::tempdir().unwrap();
        let zip = place.path().join("it.zip");
        fs::write(&zip, archive(&[("a.tex", "1"), ("b.tex", &"x".repeat(300_000))])).unwrap();
        let beside = place.path().join("beside");
        // Stopped at the first question, at the second, ...: at the start, between files, in a file.
        for questions in 0..4 {
            let asked = std::cell::Cell::new(0);
            let answer = unpack(&zip, &beside, &Limits::default(), &|| {
                asked.set(asked.get() + 1);
                asked.get() > questions
            });
            assert!(matches!(answer, Err(UnpackError::Stopped)), "{questions}");
            let left = fs::read_dir(&beside).map(|found| found.count()).unwrap_or(0);
            assert_eq!(left, 0);
        }
    }

    #[test]
    fn one_wrapping_folder_is_removed_and_two_are_not() {
        let one = archive(&[("thesis/", ""), ("thesis/main.tex", TEX), ("thesis/ch/a.tex", "")]);
        assert_eq!(files_of(&one), ["ch/a.tex", "main.tex"]);
        let two = archive(&[("a/b/main.tex", TEX)]);
        assert_eq!(files_of(&two), ["b/main.tex"]);
        let beside = archive(&[("a/main.tex", TEX), ("b/x.tex", "")]);
        assert_eq!(files_of(&beside), ["a/main.tex", "b/x.tex"]);
        // What stays out does not stand beside it.
        let mac = archive(&[("thesis/main.tex", TEX), ("__MACOSX/thesis/._main.tex", "")]);
        assert_eq!(files_of(&mac), ["main.tex"]);
    }

    #[test]
    fn git_and_our_own_folder_and_mac_leftovers_stay_out() {
        let bytes = archive(&[
            ("main.tex", TEX),
            (".git/config", ""),
            ("sub/.GIT/hooks/post-checkout", ""),
            (".abstract-tex/build/main.pdf", ""),
            ("sub/.abstract-tex/x", ""),
            ("__MACOSX/._main.tex", ""),
            ("sub/.DS_Store", ""),
            (".gitignore", ""),
            ("sub/x.tex", ""),
        ]);
        assert_eq!(files_of(&bytes), [".gitignore", "main.tex", "sub/x.tex"]);
    }

    #[test]
    fn a_repository_by_another_name_stays_out() {
        // A folder is a repository when it has a file `HEAD`, and `objects` and `refs` or a file that says
        // where those are; what it is called is nothing to Git. Without its `HEAD` it is only a folder.
        let hidden = |folder: &str| {
            vec![
                (format!("{folder}HEAD"), "ref: refs/heads/main"),
                (format!("{folder}config"), "[core]\nworktree = ../../.."),
                (format!("{folder}objects/x"), ""),
                (format!("{folder}refs/x"), ""),
            ]
        };
        for folder in ["", "sub/", "deep/er/"] {
            for wrap in ["", "thesis/"] {
                let mut all: Vec<_> = hidden(folder)
                    .into_iter()
                    .map(|(name, text)| (format!("{wrap}{name}"), text))
                    .collect();
                // In any order: `HEAD` may come before what makes it one.
                for _ in 0..all.len() {
                    all.rotate_left(1);
                    let mut files: Vec<(&str, &str)> =
                        all.iter().map(|(name, text)| (name.as_str(), *text)).collect();
                    let main = format!("{wrap}main.tex");
                    files.push((&main, TEX));
                    let found = files_of(&archive(&files));
                    assert!(found.iter().all(|file| !file.ends_with("HEAD")), "{found:?}");
                    assert_eq!(found.len(), 4, "the rest is only files: {found:?}");
                }
            }
        }
        for files in [
            ["Head", "OBJECTS/", "Refs/"],
            ["sub\\HEAD", "sub/objects/", "sub/refs/"],
            ["HEAD", "commondir", "config"],
            ["HEAD", "CommonDir", "gitdir"],
            // On macOS the long s is an s.
            ["HEAD", "object\u{17f}/x", "ref\u{17f}/x"],
        ] {
            let found = files_of(&archive(&[
                ("main.tex", TEX),
                (files[0], ""),
                (files[1], ""),
                (files[2], ""),
            ]));
            let has_head = found.iter().any(|file| file.to_lowercase().ends_with("head"));
            assert!(!has_head, "{found:?}");
        }
        // A `.git` that is a file says where the repository is.
        let elsewhere = archive(&[
            ("main.tex", TEX),
            (".git", "gitdir: ../../.."),
            ("a/.Git", "gitdir: x"),
        ]);
        assert_eq!(files_of(&elsewhere), ["main.tex"]);
    }

    #[test]
    fn a_file_that_is_only_called_head_is_kept() {
        let bytes = archive(&[
            ("main.tex", TEX),
            ("HEAD", ""),
            ("a/head", ""),
            ("b/refs/x", ""),
            ("c/HEAD/objects", ""),
        ]);
        assert_eq!(
            files_of(&bytes),
            ["HEAD", "a/head", "b/refs/x", "c/HEAD/objects", "main.tex"]
        );
    }

    #[test]
    fn a_name_from_an_old_zip_program_keeps_its_accents() {
        let mut bytes = archive(&[("trXs.tex", TEX)]);
        for at in 0..bytes.len() - 8 {
            if &bytes[at..at + 8] == b"trXs.tex" {
                bytes[at + 2] = 0x88;
            }
        }
        assert_eq!(files_of(&bytes), ["três.tex"]);
    }

    #[cfg(unix)]
    #[test]
    fn what_is_unpacked_cannot_be_run() {
        use std::os::unix::fs::PermissionsExt;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().unix_permissions(0o777);
        writer.start_file("main.tex", options).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let (place, answer) = unpacked(&bytes, &Limits::default());
        let destination = place.path().join("out");
        answer.unwrap().place(&destination).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode(&destination.join("main.tex")) & 0o111, 0);
        // As any folder the person makes, not the staging folder's own "only for me".
        let ordinary = place.path().join("ordinary");
        fs::create_dir(&ordinary).unwrap();
        assert_eq!(mode(&destination), mode(&ordinary));
    }
}
