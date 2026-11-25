//! Shell-escape consent (S9.8): the project folders the person at this machine has allowed to run
//! programs while they build — what `minted` needs, and what lets a document run any command.
//!
//! Kept in the app's own config folder (`shell-escape.toml`), one entry per absolute project
//! folder. It must never be read from, or written into, a project: `abstract-tex.toml` and
//! everything under `.abstract-tex/` can arrive with a cloned repository or an unzipped archive,
//! and nothing a project carries may turn this on (S9.4's point 4). It must never be granted
//! without the person saying so; this module only records what they said.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::project::write_atomically;

/// Held around every read-change-write, as in `recents.rs`: a permission that was just withdrawn must not
/// be written back by a change that read the file before.
static CHANGE: Mutex<()> = Mutex::new(());

/// The file's shape: a list, so a person reading it sees exactly which folders they allowed.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsentFile {
    #[serde(default)]
    allowed: Vec<String>,
}

/// Where this machine's consent is kept. One per app; handed to commands as Tauri state.
#[derive(Debug)]
pub struct ShellEscapeConsent {
    file: PathBuf,
}

impl ShellEscapeConsent {
    pub fn at(file: PathBuf) -> Self {
        Self { file }
    }

    /// Whether builds of `project_dir` may run programs. Read from disk every time, so a
    /// consent withdrawn by editing the file takes effect on the next build. Anything that goes
    /// wrong reading it — no file, a file that does not parse — means no.
    pub fn allows(&self, project_dir: &Path) -> bool {
        let key = key_for(project_dir);
        self.read()
            .map(|consent| consent.allowed.contains(&key))
            .unwrap_or(false)
    }

    pub fn allow(&self, project_dir: &Path) -> Result<()> {
        // Poisoned: another thread panicked while holding it. The file is still whole, so carry on.
        let _one_change_at_a_time = CHANGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // A file that does not parse is replaced rather than trusted: it could only ever have
        // meant "no", and the person is now saying "yes" to one folder.
        let mut consent = self.read().unwrap_or_default();
        let key = key_for(project_dir);
        if !consent.allowed.contains(&key) {
            consent.allowed.push(key);
        }
        self.write(&consent)
    }

    /// After an `Ok`, builds of `project_dir` may not run programs.
    pub fn disallow(&self, project_dir: &Path) -> Result<()> {
        let _one_change_at_a_time = CHANGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut allowed = self.allowed_folders()?;
        let key = key_for(project_dir);
        if !allowed.contains(&key) {
            return Ok(());
        }
        allowed.retain(|allowed| *allowed != key);
        self.write(&ConsentFile { allowed })
    }

    /// The folders whose builds may run programs, as they were spelled when they were allowed.
    pub fn allowed_folders(&self) -> Result<Vec<String>> {
        match fs::read_to_string(&self.file) {
            // A file that does not parse allows nothing already.
            Ok(text) => Ok(toml::from_str::<ConsentFile>(&text).unwrap_or_default().allowed),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            // Not "nothing is allowed": a file that cannot be read now may well be readable at the next
            // build, with this folder still in it.
            Err(error) => Err(error).with_context(|| format!("could not read {}", self.file.display())),
        }
    }

    fn read(&self) -> Option<ConsentFile> {
        let text = fs::read_to_string(&self.file).ok()?;
        toml::from_str(&text).ok()
    }

    fn write(&self, consent: &ConsentFile) -> Result<()> {
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder).with_context(|| format!("could not create {}", folder.display()))?;
        }
        let text = toml::to_string(consent)?;
        write_atomically(&self.file, &text)
            .with_context(|| format!("could not write {}", self.file.display()))
    }
}

/// The folder as the entry is written: the absolute path `Project::open` already made, so the
/// same folder opened twice gives the same key, and a copy of the project somewhere else — a
/// fresh clone, say — does not inherit the consent its original was given.
fn key_for(project_dir: &Path) -> String {
    project_dir.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consent() -> (tempfile::TempDir, ShellEscapeConsent) {
        let config = tempfile::tempdir().unwrap();
        let consent = ShellEscapeConsent::at(config.path().join("app").join("shell-escape.toml"));
        (config, consent)
    }

    /// Readable now or not, it may be at the next build, with the folder still in it (S17.6).
    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_is_not_taken_as_withdrawn() {
        let config = tempfile::tempdir().unwrap();
        let in_the_way = config.path().join("app");
        std::fs::write(&in_the_way, "a file where the folder should be").unwrap();
        let consent = ShellEscapeConsent::at(in_the_way.join("kept"));
        assert!(consent.disallow(Path::new("/work/thesis")).is_err());
    }

    #[test]
    fn nothing_is_allowed_until_someone_allows_it() {
        let (_config, consent) = consent();
        assert!(!consent.allows(Path::new("/work/thesis")));
    }

    #[test]
    fn allowing_one_folder_allows_that_folder_only_and_can_be_taken_back() {
        let (_config, consent) = consent();
        consent.allow(Path::new("/work/thesis")).unwrap();
        consent.allow(Path::new("/work/thesis")).unwrap();
        assert!(consent.allows(Path::new("/work/thesis")));
        assert!(
            !consent.allows(Path::new("/work/thesis-clone")),
            "a copy elsewhere is another folder"
        );
        assert!(!consent.allows(Path::new("/work")), "nor is its parent");

        let text = fs::read_to_string(&consent.file).unwrap();
        assert_eq!(text.matches("/work/thesis").count(), 1, "listed once: {text}");

        consent.disallow(Path::new("/work/thesis")).unwrap();
        assert!(!consent.allows(Path::new("/work/thesis")));
    }

    #[test]
    fn a_file_that_does_not_parse_allows_nothing_and_is_replaced_by_the_next_yes() {
        let (_config, consent) = consent();
        fs::create_dir_all(consent.file.parent().unwrap()).unwrap();
        fs::write(&consent.file, "allowed = \"/work/thesis\"  # not a list").unwrap();
        assert!(!consent.allows(Path::new("/work/thesis")));
        consent.allow(Path::new("/work/paper")).unwrap();
        assert!(consent.allows(Path::new("/work/paper")));
        assert!(!consent.allows(Path::new("/work/thesis")));
    }

    // Found by the review of S17: a withdrawn permission could be written back by a change that had read
    // the file before it was withdrawn.
    #[test]
    fn two_changes_at_once_are_both_kept() {
        let (_config, consent) = consent();
        let (gone, new) = (Path::new("/work/gone"), Path::new("/work/new"));
        for round in 0..50 {
            consent.allow(gone).unwrap();
            consent.disallow(new).unwrap();
            // Two threads that set off together, so that they really overlap.
            let start = std::sync::Barrier::new(2);
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    start.wait();
                    consent.disallow(gone).unwrap();
                });
                threads.spawn(|| {
                    start.wait();
                    consent.allow(new).unwrap();
                });
            });
            assert!(!consent.allows(gone) && consent.allows(new), "round {round}");
        }
    }
}
