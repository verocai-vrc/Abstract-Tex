//! What the person chose for the app on this computer (S17.6): where new documents are kept, their
//! name, how big the text is, and which colours.
//!
//! Kept in the app's own config folder (`preferences.toml`), never in a document. It must never be a
//! reason that the app does not start: a file that cannot be read is the defaults, and one value that
//! makes no sense is its own default and does not cost the others. Nor may it be lost: a file that cannot
//! be read is not replaced, and one that cannot be understood is kept beside the new one.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::project::write_atomically;

/// The text sizes on offer, in percent.
pub const TEXT_SIZES: [u16; 6] = [100, 115, 130, 150, 175, 200];

/// Held around every read-change-write, as in `recents.rs`.
static CHANGE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Colours {
    /// As the computer is set.
    #[default]
    Match,
    Light,
    Dark,
    ContrastLight,
    ContrastDark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    /// Where new documents are kept; `None` until someone picks a place.
    pub place: Option<String>,
    pub author: String,
    pub text_size: u16,
    pub colours: Colours,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            place: None,
            author: String::new(),
            text_size: TEXT_SIZES[0],
            colours: Colours::default(),
        }
    }
}

/// Where they are kept. One per app; handed to commands as Tauri state.
#[derive(Debug, Clone)]
pub struct PreferencesFile {
    file: PathBuf,
}

impl PreferencesFile {
    pub fn at(file: PathBuf) -> Self {
        Self { file }
    }

    pub fn read(&self) -> Preferences {
        self.table().ok().flatten().map(understood).unwrap_or_default()
    }

    /// `None` for a file that is not TOML.
    fn table(&self) -> std::io::Result<Option<toml::Table>> {
        match fs::read(&self.file) {
            Ok(bytes) => Ok(std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| text.parse().ok())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Some(toml::Table::new())),
            Err(error) => Err(error),
        }
    }

    /// Change some of them, and answer them all as they are now.
    pub fn change(&self, edit: impl FnOnce(&mut Preferences)) -> Result<Preferences> {
        // Poisoned: another thread panicked while holding it. The file is still whole, so carry on.
        let _one_change_at_a_time = CHANGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // A file that could not be read just now must not become one with the defaults.
        let table = self
            .table()
            .with_context(|| format!("could not read {}", self.file.display()))?;
        let is_toml = table.is_some();
        let mut preferences = table.map(understood).unwrap_or_default();
        edit(&mut preferences);
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder).with_context(|| format!("could not create {}", folder.display()))?;
        }
        if !is_toml {
            // Somebody wrote it by hand, and one wrong quote makes the whole of it no TOML.
            let aside = self.file.with_extension("toml.unreadable");
            fs::rename(&self.file, &aside)
                .with_context(|| format!("could not rename {}", self.file.display()))?;
        }
        write_atomically(&self.file, &toml::to_string(&preferences)?)
            .with_context(|| format!("could not write {}", self.file.display()))?;
        Ok(preferences)
    }
}

fn understood(table: toml::Table) -> Preferences {
    // Value by value, rather than the whole struct at once: serde would refuse all of them over
    // one that somebody mistyped.
    let text = |name: &str| {
        table
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };
    Preferences {
        place: text("place").filter(|place| !place.is_empty()),
        author: text("author").unwrap_or_default(),
        text_size: table
            .get("textSize")
            .and_then(|value| value.as_integer())
            .and_then(|size| u16::try_from(size).ok())
            .filter(|size| TEXT_SIZES.contains(size))
            .unwrap_or(TEXT_SIZES[0]),
        colours: table
            .get("colours")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default(),
    }
}

/// Where new documents go until someone picks a place: a folder of our own in Documents, or in the home
/// folder when the system names no Documents.
pub fn default_place(documents: Option<PathBuf>, home: PathBuf) -> PathBuf {
    documents.unwrap_or(home).join("Abstract-Tex")
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Barrier;

    use super::*;

    fn kept(text: &str) -> Preferences {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("preferences.toml");
        fs::write(&file, text).unwrap();
        PreferencesFile::at(file).read()
    }

    #[test]
    fn what_cannot_be_read_is_the_default() {
        let nowhere = PreferencesFile::at(PathBuf::from("/nowhere/preferences.toml"));
        assert_eq!(nowhere.read(), Preferences::default());
        assert_eq!(kept("this is not TOML"), Preferences::default());
        assert_eq!(kept("textSize = 900").text_size, 100);
        assert_eq!(kept("textSize = -1").text_size, 100);
        assert_eq!(kept("textSize = \"big\"").text_size, 100);
        assert_eq!(kept("colours = \"pink\"").colours, Colours::Match);
        assert_eq!(kept("place = \"\"").place, None);
    }

    #[test]
    fn one_bad_value_does_not_lose_the_others() {
        let read = kept("author = \"Ana\"\ntextSize = 900\ncolours = \"contrast-dark\"");
        assert_eq!(
            (read.author.as_str(), read.text_size, read.colours),
            ("Ana", 100, Colours::ContrastDark)
        );
    }

    #[test]
    fn a_change_is_there_at_the_next_read_and_leaves_the_rest() {
        let folder = tempfile::tempdir().unwrap();
        let file = PreferencesFile::at(folder.path().join("app").join("preferences.toml"));
        file.change(|p| p.place = Some("/usb/writing".into())).unwrap();
        let answered = file.change(|p| p.colours = Colours::ContrastLight).unwrap();
        assert_eq!(answered, file.read());
        assert_eq!(answered.place.as_deref(), Some("/usb/writing"));
        assert_eq!(answered.colours, Colours::ContrastLight);
    }

    // A virus scanner or a sync program can hold a file for a moment.
    #[test]
    fn preferences_that_cannot_be_read_just_now_are_not_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let file = PreferencesFile::at(folder.path().join("preferences.toml"));
        fs::create_dir(&file.file).unwrap(); // a folder at its name cannot be read as a file
        assert_eq!(file.read(), Preferences::default());
        let refused = file.change(|p| p.author = "Ana".into());
        assert!(format!("{:#}", refused.unwrap_err()).contains("could not read"));
    }

    #[test]
    fn a_file_that_is_not_toml_is_kept_aside() {
        let folder = tempfile::tempdir().unwrap();
        let file = PreferencesFile::at(folder.path().join("preferences.toml"));
        let typed = "textSize = 200\nplace = \"C:\\Users\\Ana\"";
        fs::write(&file.file, typed).unwrap();
        let kept = file.change(|p| p.author = "Ana".into()).unwrap();
        assert_eq!(kept, file.read());
        let aside = folder.path().join("preferences.toml.unreadable");
        assert_eq!(fs::read_to_string(aside).unwrap(), typed);
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);
    }

    #[test]
    fn two_changes_at_once_are_both_kept() {
        let folder = tempfile::tempdir().unwrap();
        let file = PreferencesFile::at(folder.path().join("preferences.toml"));
        for round in 0..20 {
            file.change(|p| *p = Preferences::default()).unwrap();
            // Two threads that set off together, so that they really overlap.
            let start = Barrier::new(2);
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    start.wait();
                    file.change(|p| p.author = "Ana".into()).unwrap();
                });
                threads.spawn(|| {
                    start.wait();
                    file.change(|p| p.text_size = 150).unwrap();
                });
            });
            let read = file.read();
            assert_eq!(
                (read.author.as_str(), read.text_size),
                ("Ana", 150),
                "round {round}"
            );
        }
    }

    #[test]
    fn new_documents_go_to_documents_or_to_home() {
        assert_eq!(
            default_place(Some("/h/Documents".into()), "/h".into()),
            Path::new("/h/Documents/Abstract-Tex")
        );
        assert_eq!(default_place(None, "/h".into()), Path::new("/h/Abstract-Tex"));
    }
}
