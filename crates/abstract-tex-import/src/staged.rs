//! A folder that is made out of sight, beside where it is going, and appears whole or not at all.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

/// What a staging folder's name begins with. `tempfile` puts six letters or digits behind it.
const PREFIX: &str = ".abstract-tex-new-";
/// The name of the folder in it that is the document.
const DOCUMENT: &str = "document";
const A_DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Remove the staging folders in `place` that a crash left a day ago or more. Only what is surely ours: a
/// folder and no link, with such a name, and with nothing in it but a folder `document`. What cannot be
/// removed stays.
fn remove_abandoned(place: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(place) else {
        return;
    };
    for entry in entries.flatten() {
        if is_abandoned(&entry, now) {
            // It removes the links it meets, and never what they lead to.
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn is_abandoned(entry: &fs::DirEntry, now: SystemTime) -> bool {
    let name = entry.file_name();
    let is_ours = name
        .to_str()
        .and_then(|name| name.strip_prefix(PREFIX))
        .is_some_and(|end| end.len() == 6 && end.chars().all(|c| c.is_ascii_alphanumeric()));
    // Neither this nor `metadata` follows a link.
    let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
    if !is_ours || !is_folder {
        return false;
    }
    // `duration_since` fails for a time to come: a clock that was set back says nothing.
    let is_old = entry
        .metadata()
        .and_then(|found| found.modified())
        .is_ok_and(|made| now.duration_since(made).is_ok_and(|age| age >= A_DAY));
    let Ok(mut inside) = fs::read_dir(entry.path()) else {
        return false;
    };
    is_old
        && inside.all(|inner| {
            inner.is_ok_and(|inner| {
                inner.file_name() == DOCUMENT && inner.file_type().is_ok_and(|kind| kind.is_dir())
            })
        })
}

/// A folder that is ready to be put in its place. Dropping it removes it.
#[derive(Debug)]
pub struct Staged {
    /// Dropping a `TempDir` deletes it, which is what makes every early return leave nothing behind. It
    /// is only kept for that: the underscore says that nothing reads it.
    _staging: TempDir,
    /// The folder inside it that is the document.
    document: PathBuf,
}

impl Staged {
    /// An empty one, in a hidden folder in `beside`, so that putting it in its place is one rename.
    pub(crate) fn begin(beside: &Path) -> io::Result<Self> {
        fs::create_dir_all(beside)?;
        remove_abandoned(beside, SystemTime::now());
        let staging = tempfile::Builder::new().prefix(PREFIX).tempdir_in(beside)?;
        // A folder in the staging folder, because `tempfile` makes that one readable by its owner only,
        // and a folder keeps its permissions when it is renamed.
        let document = staging.path().join(DOCUMENT);
        fs::create_dir(&document)?;
        Ok(Self {
            _staging: staging,
            document,
        })
    }

    pub(crate) fn document(&self) -> &Path {
        &self.document
    }

    /// Zips are often made of a folder: then the document is in it, and it is left out.
    pub(crate) fn remove_wrapping_folder(&mut self) -> io::Result<()> {
        let mut inside = fs::read_dir(&self.document)?;
        if let (Some(only), None) = (inside.next(), inside.next()) {
            let only = only?;
            if only.file_type()?.is_dir() {
                self.document = only.path();
            }
        }
        Ok(())
    }

    /// Put it at `destination`, where there must be nothing.
    pub fn place(self, destination: &Path) -> io::Result<()> {
        // Unix would rename a folder over an empty one.
        if destination.symlink_metadata().is_ok() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        fs::rename(&self.document, destination)
    }
}

/// `name` in `place`, with a number if it is taken: `thesis`, `thesis-2`, ...
pub fn free_name(place: &Path, name: &str) -> PathBuf {
    // `symlink_metadata` and not `exists`, for which a link to nothing is nothing.
    let is_taken = |path: &PathBuf| path.symlink_metadata().is_ok();
    let mut path = place.join(name);
    let mut number = 2;
    while is_taken(&path) {
        path = place.join(format!("{name}-{number}"));
        number += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_a_number() {
        let place = tempfile::tempdir().unwrap();
        assert_eq!(free_name(place.path(), "thesis"), place.path().join("thesis"));
        fs::create_dir(place.path().join("thesis")).unwrap();
        fs::write(place.path().join("thesis-2"), "").unwrap();
        assert_eq!(free_name(place.path(), "thesis"), place.path().join("thesis-3"));
    }

    #[cfg(unix)]
    #[test]
    fn a_name_is_taken_by_a_link_to_nothing_too() {
        let place = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("nowhere", place.path().join("thesis")).unwrap();
        assert_eq!(free_name(place.path(), "thesis"), place.path().join("thesis-2"));
    }

    #[test]
    fn it_is_made_beside_where_it_goes_and_removed_if_it_goes_nowhere() {
        // Elsewhere might be another disk, and a folder cannot be renamed from one to another.
        let place = tempfile::tempdir().unwrap();
        let staged = Staged::begin(place.path()).unwrap();
        assert!(staged.document().starts_with(place.path()));
        drop(staged);
        assert_eq!(fs::read_dir(place.path()).unwrap().count(), 0);
    }

    #[test]
    fn what_a_crash_left_is_removed_after_a_day_and_nothing_else() {
        let place = tempfile::tempdir().unwrap();
        let make = |name: &str| {
            fs::create_dir_all(place.path().join(name)).unwrap();
            fs::write(place.path().join(name).join("x.tex"), "").unwrap();
        };
        // Ours, made and never removed.
        make(".abstract-tex-new-a1B2c3/document");
        // Not ours: another name, something else in it, or a file.
        make(".abstract-tex-new-a1B2c3 copy/document");
        make(".abstract-tex-new-thesis-2/document");
        make(".abstract-tex/document");
        make(".abstract-tex-new-d1B2c3/thesis");
        make(".abstract-tex-new-e1B2c3/document");
        make(".abstract-tex-new-e1B2c3/thesis");
        fs::write(place.path().join(".abstract-tex-new-f1B2c3"), "").unwrap();
        let mut kept = names_in(place.path());
        let hour = Duration::from_secs(60 * 60);

        remove_abandoned(place.path(), SystemTime::now() + 23 * hour);
        assert_eq!(names_in(place.path()), kept, "it may still be at work");
        remove_abandoned(place.path(), SystemTime::now() - 48 * hour);
        assert_eq!(names_in(place.path()), kept, "a clock that was set back");
        remove_abandoned(place.path(), SystemTime::now() + 25 * hour);
        kept.retain(|name| name != ".abstract-tex-new-a1B2c3");
        assert_eq!(names_in(place.path()), kept);
    }

    #[cfg(unix)]
    #[test]
    fn removing_what_a_crash_left_follows_no_link() {
        use std::os::unix::fs::symlink;
        let place = tempfile::tempdir().unwrap();
        let theirs = tempfile::tempdir().unwrap();
        fs::create_dir(theirs.path().join("document")).unwrap();
        fs::write(theirs.path().join("document/thesis.tex"), "").unwrap();
        // The folder itself, the one in it, and one deeper.
        symlink(theirs.path(), place.path().join(".abstract-tex-new-a1B2c3")).unwrap();
        for name in [".abstract-tex-new-b1B2c3", ".abstract-tex-new-c1B2c3/document"] {
            fs::create_dir_all(place.path().join(name)).unwrap();
        }
        symlink(
            theirs.path(),
            place.path().join(".abstract-tex-new-b1B2c3/document"),
        )
        .unwrap();
        symlink(
            theirs.path(),
            place.path().join(".abstract-tex-new-c1B2c3/document/in"),
        )
        .unwrap();
        remove_abandoned(place.path(), SystemTime::now() + 2 * A_DAY);
        assert_eq!(
            names_in(place.path()),
            [".abstract-tex-new-a1B2c3", ".abstract-tex-new-b1B2c3"]
        );
        assert!(theirs.path().join("document/thesis.tex").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn beginning_one_removes_what_a_crash_left() {
        let place = tempfile::tempdir().unwrap();
        let left = place.path().join(".abstract-tex-new-a1B2c3");
        fs::create_dir_all(left.join("document")).unwrap();
        fs::File::open(&left)
            .unwrap()
            .set_modified(SystemTime::now() - 2 * A_DAY)
            .unwrap();
        drop(Staged::begin(place.path()).unwrap());
        assert_eq!(names_in(place.path()), Vec::<String>::new());
    }

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn it_is_never_put_over_something() {
        let place = tempfile::tempdir().unwrap();
        fs::create_dir(place.path().join("thesis")).unwrap();
        let staged = Staged::begin(place.path()).unwrap();
        assert!(staged.place(&place.path().join("thesis")).is_err());
        assert_eq!(fs::read_dir(place.path()).unwrap().count(), 1);
    }
}
