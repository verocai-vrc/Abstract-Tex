//! Recovering from a snapshot, as the Snapshots list needs it (S11.6, design interview B3).
//!
//! The crate (`abstract-tex-snapshot`) reads; this module is the one place a snapshot is allowed
//! to *write* into the working tree, and it does so for exactly one file at a time.
//!
//! **What it must never do:**
//!
//! - Never restore over work that is not itself kept. Before a file is replaced, the project as it
//!   stands is snapshotted, so a restore is undone by restoring the snapshot it just made. The
//!   author can press the button without being sure, which is the whole point of a button for
//!   people who "never once pressed commit".
//! - Never write outside the project, or a path the snapshot does not hold. The path is resolved
//!   by `Project::resolve`, the same refusal every write command uses.
//! - Never hide the write from the watcher. Unlike an edit made in our own editor, a restore is
//!   *news* to an open tab, so it is not recorded as our own echo: the watcher reports it and the
//!   tab reconciles with it exactly as it would after `git checkout`.

use std::path::Path;

use crate::project::write_atomically;

/// At most one snapshot at a time — taken by the compile's snapshot and by [`restore_file`]'s own.
///
/// Builds are serialised (S9.9), but a snapshot outlives the build that triggered it, so a slow
/// one could still be hashing a folder of figures when the next compile finishes. Two snapshots
/// racing would each read the same parent and each move the ref, leaving one of the two commits
/// written but off the chain — no corruption, but a version silently missing from the history
/// the author would go looking through. One lock is cheaper than reasoning about that again.
pub static AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Put `path` back as it was in snapshot `id`, keeping the version it replaces.
///
/// `absolute` is `path` already resolved against the project by `Project::resolve` — the caller
/// holds the project lock for that, and the blocking work below must not. Every refusal comes
/// before anything is written. Text files only: the Snapshots list offers `.tex` and `.bib`, and
/// a file that is not UTF-8 would be written back through a lossy decode.
pub fn restore_file(project_dir: &Path, absolute: &Path, id: &str, path: &str) -> Result<(), String> {
    let old = abstract_tex_snapshot::read(project_dir, id, path)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("That version of the project has no {path}."))?;
    let old = String::from_utf8(old)
        .map_err(|_| format!("{path} is not a text file, so it cannot be restored here."))?;

    {
        let _guard = AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        abstract_tex_snapshot::snapshot(project_dir).map_err(|error| {
            format!("The current version could not be kept first, so nothing was changed: {error}")
        })?;
    }

    write_atomically(absolute, &old).map_err(|error| format!("{path} could not be written: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A project with `main.tex` snapshotted twice, and the id of the first (older) snapshot.
    fn project_with_two_versions() -> (tempfile::TempDir, crate::project::Project, String) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.tex"), "the first draft").unwrap();
        let abstract_tex_snapshot::Snapshot::Took(first) =
            abstract_tex_snapshot::snapshot(tmp.path()).unwrap()
        else {
            panic!("expected a snapshot")
        };
        fs::write(tmp.path().join("main.tex"), "the second draft, better").unwrap();
        abstract_tex_snapshot::snapshot(tmp.path()).unwrap();
        let project = crate::project::Project::open(tmp.path()).unwrap();
        (tmp, project, first.to_string())
    }

    /// What the command does before calling in: resolve the path inside the project.
    fn restore(
        tmp: &tempfile::TempDir,
        project: &crate::project::Project,
        id: &str,
        path: &str,
    ) -> Result<(), String> {
        let absolute = project.resolve(path).map_err(|error| error.to_string())?;
        restore_file(tmp.path(), &absolute, id, path)
    }

    #[test]
    fn restoring_puts_the_old_text_back() {
        let (tmp, project, first) = project_with_two_versions();
        restore(&tmp, &project, &first, "main.tex").unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("main.tex")).unwrap(),
            "the first draft"
        );
    }

    #[test]
    fn the_version_a_restore_replaces_is_kept_and_can_be_restored_in_turn() {
        let (tmp, project, first) = project_with_two_versions();
        // Work the author has not compiled yet: the newest snapshot does not have it.
        fs::write(tmp.path().join("main.tex"), "typed since the last build").unwrap();

        restore(&tmp, &project, &first, "main.tex").unwrap();

        let rows = abstract_tex_snapshot::list(tmp.path(), 10).unwrap();
        assert_eq!(
            rows.len(),
            3,
            "the unsaved-to-history version was snapshotted before it was replaced"
        );
        let kept = abstract_tex_snapshot::read(tmp.path(), &rows[0].id, "main.tex")
            .unwrap()
            .unwrap();
        assert_eq!(kept, b"typed since the last build");

        restore(&tmp, &project, &rows[0].id, "main.tex").unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("main.tex")).unwrap(),
            "typed since the last build"
        );
    }

    #[test]
    fn a_path_outside_the_project_or_not_in_the_snapshot_is_refused_and_nothing_changes() {
        let (tmp, project, first) = project_with_two_versions();
        for bad in ["../elsewhere.tex", "/etc/passwd", "chapters/missing.tex"] {
            assert!(
                restore(&tmp, &project, &first, bad).is_err(),
                "{bad} was accepted"
            );
        }
        assert_eq!(
            fs::read_to_string(tmp.path().join("main.tex")).unwrap(),
            "the second draft, better"
        );
        assert_eq!(
            abstract_tex_snapshot::list(tmp.path(), 10).unwrap().len(),
            2,
            "a refusal must not snapshot"
        );
    }

    #[test]
    fn an_id_that_is_not_a_snapshot_is_refused() {
        let (tmp, project, _first) = project_with_two_versions();
        let error = restore(
            &tmp,
            &project,
            "0000000000000000000000000000000000000000",
            "main.tex",
        )
        .unwrap_err();
        assert!(error.contains("not one of this project's snapshots"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_text_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("figure.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
        let abstract_tex_snapshot::Snapshot::Took(id) = abstract_tex_snapshot::snapshot(tmp.path()).unwrap()
        else {
            panic!()
        };
        let project = crate::project::Project::open(tmp.path()).unwrap();

        let error = restore(&tmp, &project, &id.to_string(), "figure.bin").unwrap_err();

        assert!(error.contains("not a text file"), "{error}");
    }
}
