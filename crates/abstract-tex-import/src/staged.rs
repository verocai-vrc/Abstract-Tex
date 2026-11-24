//! A folder that is made out of sight, beside where it is going, and appears whole or not at all.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

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
        let staging = tempfile::Builder::new()
            .prefix(".abstract-tex-zip-")
            .tempdir_in(beside)?;
        // A folder in the staging folder, because `tempfile` makes that one readable by its owner only,
        // and a folder keeps its permissions when it is renamed.
        let document = staging.path().join("document");
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
    fn it_is_never_put_over_something() {
        let place = tempfile::tempdir().unwrap();
        fs::create_dir(place.path().join("thesis")).unwrap();
        let staged = Staged::begin(place.path()).unwrap();
        assert!(staged.place(&place.path().join("thesis")).is_err());
        assert_eq!(fs::read_dir(place.path()).unwrap().count(), 1);
    }
}
