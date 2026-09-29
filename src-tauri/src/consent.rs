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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::project::write_atomically;

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
        self.read().map(|consent| consent.allowed.contains(&key)).unwrap_or(false)
    }

    pub fn allow(&self, project_dir: &Path) -> Result<()> {
        // A file that does not parse is replaced rather than trusted: it could only ever have
        // meant "no", and the person is now saying "yes" to one folder.
        let mut consent = self.read().unwrap_or_default();
        let key = key_for(project_dir);
        if !consent.allowed.contains(&key) {
            consent.allowed.push(key);
        }
        self.write(&consent)
    }

    pub fn disallow(&self, project_dir: &Path) -> Result<()> {
        let Some(mut consent) = self.read() else {
            return Ok(()); // nothing readable means nothing is allowed already
        };
        let key = key_for(project_dir);
        consent.allowed.retain(|allowed| *allowed != key);
        self.write(&consent)
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
        write_atomically(&self.file, &text).with_context(|| format!("could not write {}", self.file.display()))
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
        assert!(!consent.allows(Path::new("/work/thesis-clone")), "a copy elsewhere is another folder");
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
}
