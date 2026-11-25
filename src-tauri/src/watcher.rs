//! Filesystem watching for an open project (DESIGN.md §3, the dashed edge; §5.6).
//!
//! Owns: turning raw `notify` events into a small, debounced stream of "this path changed"
//! notifications, with two filters applied:
//!
//! 1. Paths under `.abstract-tex/` are dropped: the engine writes there on every build and none
//!    of it is an edit to the manuscript. Paths under `.git/` are not an edit either, but a
//!    handful of them change what `git status` would say, so they are reported as
//!    [`Change::GitMetadata`] instead of as a file the author touched (S10.3a).
//! 2. Reads are not changes. `notify` asks inotify for *open* events too, so a build opening
//!    `main.tex` arrives as an event for `main.tex`, and reporting it would make every build
//!    trigger the next. The debouncer is `notify-debouncer-full` rather than the mini one because
//!    only it keeps each event's kind, which is what lets an access event be told from a write.
//! 3. Files whose content matches what *we* just wrote are dropped. Otherwise every debounced
//!    save would come back as an "external change" and the reconciler would run a no-op diff.
//!    We compare a content hash rather than a timestamp window because a slow disk can deliver
//!    the event after any window we pick, and a hash cannot be late. A hash is forgotten as soon
//!    as the file is seen holding anything else, so an outside change *back* to those same bytes
//!    is still reported.
//!
//! Must never decide what an external change *means*. That is the frontend's job: it reads the
//! file, diffs it into the CRDT, or asks the author if the buffer is dirty.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
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

/// What the watcher saw, in the only two kinds the rest of the app reacts to differently.
///
/// An enum rather than a flag on [`FsEvent`], because the two carry different things: a
/// manuscript change is *a path*, which the frontend reads, diffs and may ask a question about;
/// a Git change is only the news that the answer to `git status` may have moved, and the path it
/// happened at (`.git/index`) is meaningless outside Git.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A file in the project that the author, or another program, wrote.
    Manuscript(FsEvent),
    /// Something inside `.git/` that changes what the Source Control view would show.
    GitMetadata,
}

/// Keeps the watcher thread alive. Drop it to stop watching.
pub struct ProjectWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    /// The system's file-watch limit ran out while the folders were being added, so some
    /// subfolders are not watched (R8.7). What *was* added keeps working.
    incomplete: bool,
}

impl ProjectWatcher {
    /// Whether some subfolders of the project are not being watched.
    pub fn is_incomplete(&self) -> bool {
        self.incomplete
    }
}

/// The sentence for a project some of whose subfolders could not be watched (R8.7). The limit is
/// one budget for the whole user, shared with every editor and sync client they run.
pub fn incomplete_notice() -> String {
    format!(
        "This document has more folders than your system lets programs watch, so changes that other programs make to files in some subfolders may not show up here until you reopen the document.{}",
        raising_the_limit()
    )
}

/// The sentence for a project that could not be watched at all (R8.7): it opens anyway, because
/// the editor, the build and the PDF do not need it. `error` is shown with its whole chain, since
/// the outer message alone ("could not watch <folder>") says nothing about why.
pub fn unavailable_notice(error: &anyhow::Error) -> String {
    format!(
        "Changes that other programs make to files of this document will not show up here until you reopen it: the folder could not be watched ({error:#}).{}",
        raising_the_limit()
    )
}

/// What to do about it, where there is something to do. Only Linux has a limit a person can raise.
fn raising_the_limit() -> &'static str {
    if cfg!(target_os = "linux") {
        " Raising fs.inotify.max_user_watches (and max_user_instances) in your system settings fixes this."
    } else {
        ""
    }
}

/// Remember that we wrote these bytes to this path, so the echo can be recognised.
pub fn remember_write(written: &WrittenHashes, path: &Path, contents: &[u8]) {
    written
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), content_hash(contents));
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
    F: Fn(Change) + Send + 'static,
{
    let handler = move |result: DebounceEventResult| match result {
        Ok(events) => {
            // One `GitMetadata` per debounce window at most. A single `git commit` writes the
            // index, `HEAD` and a ref in the same burst, and three identical "ask again" events
            // would mean three status reads for one answer.
            let mut git_changed = false;
            // One report per path per window: this debouncer can hand back several events for
            // the same file (a create and a modify), and the frontend only needs to hear once.
            let mut reported: HashSet<PathBuf> = HashSet::new();
            for event in events {
                // A read is not a change (see the module comment, point 2).
                if event.kind.is_access() {
                    continue;
                }
                for path in &event.paths {
                    if is_ignored(path) {
                        continue;
                    }
                    if inside_git_dir(path) {
                        git_changed |= changes_git_status(path);
                        continue;
                    }
                    if !reported.insert(path.clone()) {
                        continue;
                    }
                    let exists = path.exists();
                    if exists && path.is_file() && is_our_own_write(&written, path) {
                        debug!(path = %path.display(), "ignoring echo of our own write");
                        continue;
                    }
                    on_event(Change::Manuscript(FsEvent {
                        path: path.to_string_lossy().into_owned(),
                        exists,
                    }));
                }
            }
            if git_changed {
                on_event(Change::GitMetadata);
            }
        }
        Err(errors) => {
            for error in errors {
                warn!(%error, "file watcher error");
            }
        }
    };

    let mut debouncer = new_debouncer(DEBOUNCE, None, handler).context("could not create file watcher")?;
    let incomplete = match debouncer.watch(root, RecursiveMode::Recursive) {
        Ok(()) => false,
        // The kernel's watch budget ran out part-way through the folders. The watches that were
        // added stay in place and the debouncer keeps delivering for them (checked against the real
        // kernel, `a_full_watch_limit_leaves_a_working_but_incomplete_watcher`), so what is left is
        // a watcher that is missing some subfolders, which is better than a project that will not open.
        Err(error) if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) => {
            warn!(root = %root.display(), "file watch limit reached; some subfolders are not watched");
            true
        }
        Err(error) => {
            return Err(anyhow::Error::new(error).context(format!("could not watch {}", root.display())));
        }
    };
    Ok(ProjectWatcher {
        _debouncer: debouncer,
        incomplete,
    })
}

fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        // `.preamble` is the state folder from before the rename (see `project.rs`).
        name == ".abstract-tex" || name == ".preamble" || name.ends_with(".abstract-tex-tmp")
    })
}

fn inside_git_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".git")
}

/// Whether this file inside `.git/` is one whose change the Source Control view would see.
///
/// Four names, not "anything under `.git/`", and S10.1 is why it has to be that way: a snapshot
/// writes an object and moves `refs/abstract-tex/snapshots` after *every successful compile*, so
/// the broad filter would refresh the panel on every build for a change nobody can see. What is
/// left is what `git status` and the branch line actually read: the index, what `HEAD` points
/// at, the local branches, and `MERGE_HEAD` — which is how a repository says it is mid-merge,
/// and so how a conflict appears without any file being written.
fn changes_git_status(path: &Path) -> bool {
    let mut after_git_dir = path.components().skip_while(|c| c.as_os_str() != ".git").skip(1);
    let Some(first) = after_git_dir.next() else {
        return false;
    };
    let first = first.as_os_str().to_string_lossy();
    match first.as_ref() {
        // Git writes the index as `index.lock` and renames it, so the rename is what arrives.
        "index" | "HEAD" | "MERGE_HEAD" => true,
        "refs" => after_git_dir.next().is_some_and(|c| c.as_os_str() == "heads"),
        _ => false,
    }
}

/// Whether the file at `path` still holds exactly what the app last wrote there.
///
/// A remembered hash is only good for as long as the file keeps matching it. The moment the file
/// is seen holding anything else, somebody else has written it, the app's own save is no longer
/// what is on disk, and the hash is forgotten. Without that, a later outside change back to the
/// very same bytes (an undo in another editor, a `git checkout`) matches the old hash and is
/// swallowed as "our own echo", leaving the buffer different from the disk with nothing to say so
/// (`bugs-issues-fixes.md`). Still no clock: a late event for our own write still matches, because
/// the file still holds those bytes.
fn is_our_own_write(written: &WrittenHashes, path: &Path) -> bool {
    let mut remembered = written.lock().unwrap();
    let Some(expected) = remembered.get(path).copied() else {
        return false;
    };
    match std::fs::read(path) {
        Ok(bytes) if content_hash(&bytes) == expected => true,
        _ => {
            remembered.remove(path);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn wait_for_change(rx: &mpsc::Receiver<Change>, timeout: Duration) -> Option<Change> {
        rx.recv_timeout(timeout).ok()
    }

    /// The manuscript path out of a change, for the tests that only care about that.
    fn manuscript(change: Option<Change>) -> Option<FsEvent> {
        match change {
            Some(Change::Manuscript(event)) => Some(event),
            _ => None,
        }
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

        let event = manuscript(wait_for_change(&rx, Duration::from_secs(5))).expect("expected an fs event");
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
            wait_for_change(&rx, Duration::from_millis(1500)).is_none(),
            "a write whose hash we recorded must not be reported"
        );
    }

    /// Found on the owed smoke walk (`bugs-issues-fixes.md`): the app saves A; another program
    /// changes the file to B and then back to exactly A. The hash of A was still remembered from
    /// the app's own save, so the return to A was taken for an echo and swallowed — the buffer kept
    /// B while the disk held A. A remembered hash must stop counting once the file has been seen
    /// holding anything else.
    #[test]
    fn restoring_an_earlier_version_from_outside_is_reported_not_taken_for_our_own_echo() {
        let dir = tempfile::tempdir().unwrap();
        let written = WrittenHashes::default();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), written.clone(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let target = dir.path().join("main.tex");

        // The app saves A.
        remember_write(&written, &target, b"A");
        std::fs::write(&target, "A").unwrap();
        assert!(
            wait_for_change(&rx, Duration::from_millis(1200)).is_none(),
            "our own write"
        );

        // Another program changes it to B: reported.
        std::fs::write(&target, "B").unwrap();
        assert!(
            manuscript(wait_for_change(&rx, Duration::from_secs(5))).is_some(),
            "B is news"
        );

        // ... and back to exactly A: still news.
        std::fs::write(&target, "A").unwrap();
        assert!(
            manuscript(wait_for_change(&rx, Duration::from_secs(5))).is_some(),
            "a return to A from outside is an external change, not an echo of the app's old save"
        );
    }

    /// Reading a file is not changing it. A build opens `main.tex`, and if that were reported the
    /// build would trigger another build, for ever.
    #[test]
    fn reading_a_file_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("main.tex");
        std::fs::write(&target, "hello").unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), WrittenHashes::default(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        for _ in 0..3 {
            let _ = std::fs::read_to_string(&target).unwrap();
            let _ = std::fs::File::open(&target).unwrap();
        }

        assert_eq!(
            wait_for_change(&rx, Duration::from_millis(1500)),
            None,
            "a read must not be reported"
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

        assert!(
            wait_for_change(&rx, Duration::from_millis(1500)).is_none(),
            "build artifacts must not be reported"
        );
    }

    /// S10.3a: `git add` in a terminal writes only `.git/index`, and the panel has to move.
    #[test]
    fn a_write_to_the_git_index_asks_for_a_status_refresh_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        std::fs::create_dir_all(git.join("objects")).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(dir.path(), WrittenHashes::default(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(git.join("index"), b"not really an index").unwrap();

        assert_eq!(
            wait_for_change(&rx, Duration::from_secs(5)),
            Some(Change::GitMetadata)
        );
        assert!(
            wait_for_change(&rx, Duration::from_millis(500)).is_none(),
            "nothing inside .git is a manuscript change"
        );
    }

    #[test]
    fn ignore_rules_match_the_state_dir_but_no_longer_the_git_dir() {
        assert!(is_ignored(Path::new("C:/p/.abstract-tex/build/main.pdf")));
        assert!(is_ignored(Path::new("/p/main.tex.abstract-tex-tmp")));
        assert!(!is_ignored(Path::new("/p/sections/intro.tex")));
        // `.git` is classified rather than ignored now; `changes_git_status` is what filters it.
        assert!(!is_ignored(Path::new("/p/.git/index")));
        assert!(inside_git_dir(Path::new("/p/.git/index")));
        assert!(!inside_git_dir(Path::new("/p/sections/intro.tex")));
    }

    /// The filter that keeps a snapshot-on-compile from refreshing the panel after every build.
    #[test]
    fn only_the_four_names_that_change_the_answer_count() {
        assert!(changes_git_status(Path::new("/p/.git/index")));
        assert!(changes_git_status(Path::new("/p/.git/HEAD")));
        assert!(changes_git_status(Path::new("/p/.git/MERGE_HEAD")));
        assert!(changes_git_status(Path::new("/p/.git/refs/heads/main")));
        assert!(changes_git_status(Path::new("C:/p/.git/refs/heads/feature/x")));

        // S10.1 writes both of these after every successful compile.
        assert!(!changes_git_status(Path::new(
            "/p/.git/refs/abstract-tex/snapshots"
        )));
        assert!(!changes_git_status(Path::new("/p/.git/objects/ab/cdef")));
        assert!(!changes_git_status(Path::new("/p/.git/logs/HEAD")));
        assert!(!changes_git_status(Path::new("/p/.git/COMMIT_EDITMSG")));
    }

    /// R8.7: when the system's file-watch limit runs out, `watch` used to fail, and with it opening
    /// the project. The kernel's limit is one budget for the whole user, so this really happens.
    /// Lowering it needs a user namespace (the limit is per namespace there and the machine's own is
    /// untouched), so this test runs itself again inside `unshare -Ur` and skips where that is not
    /// allowed. The inner run does the real work.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_full_watch_limit_leaves_a_working_but_incomplete_watcher() {
        const INNER: &str = "ABSTRACT_TEX_WATCH_LIMIT_INNER_RUN";
        if std::env::var_os(INNER).is_some() {
            let dir = tempfile::tempdir().unwrap();
            for number in 0..60 {
                std::fs::create_dir(dir.path().join(format!("folder{number}"))).unwrap();
            }
            let (tx, rx) = mpsc::channel();
            let watcher = watch(dir.path(), WrittenHashes::default(), move |e| {
                let _ = tx.send(e);
            })
            .expect("a full watch limit must not be an error");
            assert!(watcher.is_incomplete());
            std::thread::sleep(Duration::from_millis(500));
            while rx.try_recv().is_ok() {} // whatever the walk itself stirred up

            std::fs::write(dir.path().join("main.tex"), "edited outside").unwrap();
            let event = manuscript(wait_for_change(&rx, Duration::from_secs(5)))
                .expect("a file in the root folder is still watched");
            assert!(event.path.ends_with("main.tex"), "{}", event.path);
            return;
        }

        let namespaces_allowed = std::process::Command::new("unshare")
            .args(["-Ur", "true"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !namespaces_allowed {
            eprintln!("skipped: no unprivileged user namespace here, so the watch limit cannot be lowered");
            return;
        }
        // `module_path!()` is `crate::watcher::tests`; the harness names a test without the crate.
        let this_test = format!(
            "{}::a_full_watch_limit_leaves_a_working_but_incomplete_watcher",
            module_path!().split_once("::").unwrap().1
        );
        let script = format!(
            "echo 16 > /proc/sys/user/max_inotify_watches || exit 77; exec '{}' --exact {this_test} --nocapture",
            std::env::current_exe().unwrap().display()
        );
        let output = std::process::Command::new("unshare")
            .args(["-Ur", "sh", "-c", &script])
            .env(INNER, "1")
            .output()
            .unwrap();
        if output.status.code() == Some(77) {
            eprintln!("skipped: the watch limit could not be lowered in the namespace");
            return;
        }
        assert!(
            output.status.success(),
            "inner run failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn the_notices_say_what_is_missing_and_a_failure_says_why() {
        assert!(incomplete_notice().contains("some subfolders"));
        let error = anyhow::anyhow!("OS file watch limit reached").context("could not watch /work/thesis");
        let text = unavailable_notice(&error);
        // The outer message alone is what `to_string` shows; the cause is the part that helps.
        assert!(text.contains("could not watch /work/thesis"), "{text}");
        assert!(text.contains("OS file watch limit reached"), "{text}");
        assert!(text.contains("until you reopen it"), "{text}");
        if cfg!(target_os = "linux") {
            assert!(
                text.contains("fs.inotify.max_user_watches")
                    && incomplete_notice().contains("fs.inotify.max_user_watches")
            );
        } else {
            assert!(!text.contains("inotify"));
        }
    }
}
