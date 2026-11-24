//! The documents used last, for the start screen (S17.3): which folder, which main file, what it is
//! called, and when.
//!
//! Kept in the app's own config folder (`recent-documents.toml`), never in a document: the list belongs
//! to this computer. It must never be a reason that something else fails. A file that cannot be read is
//! an empty list, and taking a document off the list deletes nothing but its line.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::project::{detect_root, to_forward_slashes, write_atomically};

/// How many are kept.
const KEPT: usize = 30;

/// Held around every read-change-write of the file, so that of two changes at once neither is lost.
/// A static, not a field, for the reason `assistant.rs` gives: copies of [`RecentDocuments`] made for
/// another thread all point at the same file.
static CHANGE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentDocument {
    pub folder: String,
    /// Relative to the folder, with forward slashes.
    pub root_file: String,
    pub title: String,
    /// Seconds since 1970.
    pub used_at: i64,
}

/// The file's shape: `[[document]]` sections, newest first.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentsFile {
    #[serde(default, rename = "document")]
    documents: Vec<RecentDocument>,
}

/// Where the list is kept. One per app; handed to commands as Tauri state.
#[derive(Debug, Clone)]
pub struct RecentDocuments {
    file: PathBuf,
}

impl RecentDocuments {
    pub fn at(file: PathBuf) -> Self {
        Self { file }
    }

    /// Newest first. Empty when there is no list, or none that can be read.
    pub fn list(&self) -> Vec<RecentDocument> {
        self.read().documents
    }

    /// Put a document first, and answer the title it is now shown under: the one given, else the one
    /// it already had (a letter has no `\title`; it was typed when the letter was made), else the name
    /// of its folder.
    pub fn record(&self, folder: &Path, root_file: &Path, title: Option<&str>, now: i64) -> Result<String> {
        let folder = folder.to_string_lossy().into_owned();
        self.change(|documents| {
            let known = position_of(documents, &folder).map(|at| documents.remove(at));
            let title = title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .or(known.map(|document| document.title))
                .unwrap_or_else(|| folder_name(Path::new(&folder)));
            documents.insert(
                0,
                RecentDocument {
                    folder,
                    root_file: to_forward_slashes(root_file),
                    title: title.clone(),
                    used_at: now,
                },
            );
            documents.truncate(KEPT);
            title
        })
    }

    /// A missing document was found somewhere else. When the new place is already listed, because
    /// opening it listed it, the old line goes and the new one takes over its title if it had only its
    /// folder's name.
    pub fn moved(&self, from: &Path, to: &Path) -> Result<()> {
        let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
        self.change(|documents| {
            let Some(old_at) = position_of(documents, &from) else {
                return;
            };
            if same_folder(&from, &to, cfg!(windows)) {
                return;
            }
            match position_of(documents, &to) {
                Some(new_at) => {
                    let old_title = documents[old_at].title.clone();
                    let found = &mut documents[new_at];
                    if found.title == folder_name(Path::new(&found.folder)) {
                        found.title = old_title;
                    }
                    documents.remove(old_at);
                }
                None => documents[old_at].folder = to.into_owned(),
            }
        })
    }

    /// Take one off the list. Nothing else is touched.
    pub fn forget(&self, folder: &Path) -> Result<()> {
        let folder = folder.to_string_lossy();
        self.change(|documents| {
            if let Some(at) = position_of(documents, &folder) {
                documents.remove(at);
            }
        })
    }

    pub fn clear(&self) -> Result<()> {
        self.change(Vec::clear)
    }

    fn change<T>(&self, edit: impl FnOnce(&mut Vec<RecentDocument>) -> T) -> Result<T> {
        // Poisoned: another thread panicked while holding it. The file is still whole, so carry on.
        let _one_change_at_a_time = CHANGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = self.read();
        let answer = edit(&mut file.documents);
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder).with_context(|| format!("could not create {}", folder.display()))?;
        }
        write_atomically(&self.file, &toml::to_string(&file)?)
            .with_context(|| format!("could not write {}", self.file.display()))?;
        Ok(answer)
    }

    fn read(&self) -> RecentsFile {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }
}

/// What a document is called when nothing better is known.
pub fn folder_name(folder: &Path) -> String {
    folder
        .file_name()
        .unwrap_or(folder.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn position_of(documents: &[RecentDocument], folder: &str) -> Option<usize> {
    documents
        .iter()
        .position(|document| same_folder(&document.folder, folder, cfg!(windows)))
}

/// Whether two paths name one folder. Windows does not tell `C:\Thesis` from `c:\thesis`.
pub fn same_folder(a: &str, b: &str, ignoring_case: bool) -> bool {
    if ignoring_case {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// Whether the document is still where it was. The folder alone does not say so: a drive that was
/// taken out leaves an empty one at its mount point. Reads a disk that may not answer, so callers
/// run it on a thread of its own with a limit (`start::PresenceChecks`).
pub fn is_present(document: &RecentDocument) -> bool {
    let folder = Path::new(&document.folder);
    folder.join(&document.root_file).is_file() || detect_root(folder).is_some()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Barrier};

    use super::*;

    fn list_in(folder: &tempfile::TempDir) -> RecentDocuments {
        RecentDocuments::at(folder.path().join("app").join("recent-documents.toml"))
    }

    fn record(recents: &RecentDocuments, folder: &str, title: Option<&str>) -> String {
        recents
            .record(Path::new(folder), Path::new("main.tex"), title, 1)
            .unwrap()
    }

    fn folders(recents: &RecentDocuments) -> Vec<String> {
        recents.list().into_iter().map(|d| d.folder).collect()
    }

    #[test]
    fn the_newest_is_first_and_a_document_is_listed_once() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        for folder in ["/a", "/b", "/a"] {
            record(&recents, folder, None);
        }
        assert_eq!(folders(&recents), ["/a", "/b"]);
    }

    #[test]
    fn thirty_are_kept() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        for number in 0..31 {
            record(&recents, &format!("/d/{number}"), None);
        }
        let kept = folders(&recents);
        assert_eq!(kept.len(), 30);
        assert!(!kept.contains(&"/d/0".to_string()));
    }

    #[test]
    fn an_opening_with_no_title_keeps_the_one_it_had() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        assert_eq!(record(&recents, "/p/letter", Some("My letter")), "My letter");
        assert_eq!(record(&recents, "/p/letter", None), "My letter");
        assert_eq!(record(&recents, "/p/letter", Some("  ")), "My letter");
        assert_eq!(recents.list()[0].title, "My letter");
    }

    #[test]
    fn a_document_with_no_title_is_named_after_its_folder() {
        let config = tempfile::tempdir().unwrap();
        assert_eq!(record(&list_in(&config), "/p/lab-report", None), "lab-report");
    }

    #[test]
    fn a_file_that_does_not_parse_is_an_empty_list_and_the_next_record_replaces_it() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        fs::create_dir_all(config.path().join("app")).unwrap();
        fs::write(
            config.path().join("app/recent-documents.toml"),
            "document = \"not a list\"",
        )
        .unwrap();
        assert_eq!(recents.list(), []);
        record(&recents, "/a", None);
        assert_eq!(recents.list().len(), 1);
    }

    #[test]
    fn two_records_at_once_are_both_kept() {
        for _round in 0..20 {
            let config = tempfile::tempdir().unwrap();
            let recents = Arc::new(list_in(&config));
            let barrier = Arc::new(Barrier::new(2));
            let threads: Vec<_> = ["/a", "/b"]
                .into_iter()
                .map(|folder| {
                    let (recents, barrier) = (Arc::clone(&recents), Arc::clone(&barrier));
                    std::thread::spawn(move || {
                        barrier.wait();
                        record(&recents, folder, None);
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
            assert_eq!(recents.list().len(), 2);
        }
    }

    #[test]
    fn taking_one_off_and_clearing() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        for folder in ["/a", "/b", "/c"] {
            record(&recents, folder, None);
        }
        recents.forget(Path::new("/b")).unwrap();
        assert_eq!(folders(&recents), ["/c", "/a"]);
        recents.clear().unwrap();
        assert_eq!(recents.list(), []);
    }

    #[test]
    fn a_document_found_again_keeps_its_place_and_its_title() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        record(&recents, "/a", Some("Thesis"));
        record(&recents, "/b", None);
        recents.moved(Path::new("/a"), Path::new("/usb/a")).unwrap();
        assert_eq!(folders(&recents), ["/b", "/usb/a"]);
        assert_eq!(recents.list()[1].title, "Thesis");
    }

    #[test]
    fn a_document_found_again_after_it_was_opened_is_listed_once_under_the_title_it_had() {
        let config = tempfile::tempdir().unwrap();
        let recents = list_in(&config);
        record(&recents, "/a", Some("My letter"));
        record(&recents, "/usb/letter", None); // opening it recorded it, under its folder's name
        recents.moved(Path::new("/a"), Path::new("/usb/letter")).unwrap();
        assert_eq!(folders(&recents), ["/usb/letter"]);
        assert_eq!(recents.list()[0].title, "My letter");
    }

    #[test]
    fn an_empty_mount_point_is_not_the_document() {
        let folder = tempfile::tempdir().unwrap();
        let document = RecentDocument {
            folder: folder.path().to_string_lossy().into_owned(),
            root_file: "main.tex".into(),
            title: "T".into(),
            used_at: 1,
        };
        assert!(!is_present(&document));
        fs::write(folder.path().join("main.tex"), "").unwrap();
        assert!(is_present(&document));
    }

    #[test]
    fn a_document_whose_root_file_was_renamed_is_still_there() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join("paper.tex"), "\\documentclass{article}").unwrap();
        assert!(is_present(&RecentDocument {
            folder: folder.path().to_string_lossy().into_owned(),
            root_file: "main.tex".into(),
            title: "T".into(),
            used_at: 1,
        }));
    }

    #[test]
    fn folders_differing_only_in_case_are_one_on_windows() {
        assert!(same_folder(r"C:\A", r"c:\a", true));
        assert!(!same_folder("/A", "/a", false));
    }
}
