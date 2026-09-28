//! Filesystem watching for an open project (DESIGN.md §3, the dashed edge; §5.6).
//!
//! Owns: turning raw `notify` events into a small, debounced stream of "this path changed"
//! notifications, with two filters applied:
//!
//! 1. Paths under `.abstract-tex/` and `.git/` are dropped. The engine writes into the first on
//!    every build and Git churns the second; neither is an edit to the manuscript.
//! 2. Files whose content matches what *we* just wrote are dropped. Otherwise every debounced
//!    save would come back as an "external change" and the reconciler would run a no-op diff.
//!    We compare a content hash rather than a timestamp window because a slow disk can deliver
//!    the event after any window we pick, and a hash cannot be late.
//!
//! Must never decide what an external change *means*. That is the frontend's job: it reads the
//! file, diffs it into the CRDT, or asks the author if the buffer is dirty.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use tracing::{debug, warn};

/// How long to wait after the last event before reporting. Editors and Git both touch a file
/// several times in quick succession; one notification per burst is what we want.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Path → hash of the bytes we last wrote there. Shared with the `write_file` command.
///
/// A type alias keeps the long `Arc<Mutex<HashMap<…>>>` spelled once. `Arc` so the watcher
/// thread and the command handlers share one map; `Mutex` because both sides write to it.
pub type WrittenHashes = Arc<Mutex<HashMap<PathBuf, u64>>>;

/// Sent to the frontend as the `fs:changed` event.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FsEvent {
    /// Absolute path. The frontend maps it to a project-relative one.
    pub path: String,
    /// `false` if the path was deleted or renamed away.
    pub exists: bool,
}

/// Keeps the watcher thread alive. Drop it to stop watching.
pub struct ProjectWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

/// Remember that we wrote these bytes to this path, so the echo can be recognised.
pub fn remember_write(written: &WrittenHashes, path: &Path, contents: &[u8]) {
    written.lock().unwrap().insert(path.to_path_buf(), content_hash(contents));
}

pub fn content_hash(bytes: &[u8]) -> u64 {
    // `DefaultHasher` is not cryptographic and does not need to be: we are recognising our own
    // writes, not defending against forgery.
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Start watching `root` recursively. `on_event` runs on the watcher's thread.
pub fn watch<F>(root: &Path, written: WrittenHashes, on_event: F) -> Result<ProjectWatcher>
where
    F: Fn(FsEvent) + Send + 'static,
{
    let handler = move |result: DebounceEventResult| match result {
        Ok(events) => {
            for event in events {
                if is_ignored(&event.path) {
                    continue;
                }
                let exists = event.path.exists();
                if exists && event.path.is_file() && is_our_own_write(&written, &event.path) {
                    debug!(path = %event.path.display(), "ignoring echo of our own write");
                    continue;
                }
                on_event(FsEvent { path: event.path.to_string_lossy().into_owned(), exists });
            }
        }
        Err(error) => warn!(%error, "file watcher error"),
    };

    let mut debouncer = new_debouncer(DEBOUNCE, handler).context("could not create file watcher")?;
    debouncer
        .watcher()
        .watch(root, RecursiveMode::Recursive)
        .with_context(|| format!("could not watch {}", root.display()))?;
    Ok(ProjectWatcher { _debouncer: debouncer })
}

fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        // `.preamble` is the state folder from before the rename (see `project.rs`).
        name == ".abstract-tex" || name == ".preamble" || name == ".git" || name.ends_with(".abstract-tex-tmp")
    })
}

fn is_our_own_write(written: &WrittenHashes, path: &Path) -> bool {
    let Some(expected) = written.lock().unwrap().get(path).copied() else { return false };
    match std::fs::read(path) {
        Ok(bytes) => content_hash(&bytes) == expected,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn wait_for_event(rx: &mpsc::Receiver<FsEvent>, timeout: Duration) -> Option<FsEvent> {
        rx.recv_timeout(timeout).ok()
    }

    #[test]
    fn external_write_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), WrittenHashes::default(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let target = dir.path().join("main.tex");
        std::fs::write(&target, "hello").unwrap();

        let event = wait_for_event(&rx, Duration::from_secs(5)).expect("expected an fs event");
        assert!(event.exists);
        assert!(event.path.ends_with("main.tex"), "{}", event.path);
    }

    #[test]
    fn our_own_write_is_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        let written = WrittenHashes::default();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), written.clone(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let target = dir.path().join("main.tex");
        remember_write(&written, &target, b"from the app");
        std::fs::write(&target, "from the app").unwrap();

        assert!(
            wait_for_event(&rx, Duration::from_millis(1500)).is_none(),
            "a write whose hash we recorded must not be reported"
        );
    }

    #[test]
    fn build_directory_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let build = dir.path().join(".abstract-tex").join("build");
        std::fs::create_dir_all(&build).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), WrittenHashes::default(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(build.join("main.pdf"), b"%PDF").unwrap();

        assert!(wait_for_event(&rx, Duration::from_millis(1500)).is_none(), "build artifacts must not be reported");
    }

    #[test]
    fn ignore_rules_match_state_and_git_dirs() {
        assert!(is_ignored(Path::new("C:/p/.abstract-tex/build/main.pdf")));
        assert!(is_ignored(Path::new("/p/.git/index")));
        assert!(is_ignored(Path::new("/p/main.tex.abstract-tex-tmp")));
        assert!(!is_ignored(Path::new("/p/sections/intro.tex")));
    }
}
