//! Folders that hold many things and are no document: Downloads, the home folder, a whole drive. Opened
//! as one, every file in them would be listed, watched and copied into the saved versions.

use std::path::{Path, PathBuf};

/// The crowded folders of this computer.
pub struct Crowd {
    listed: Vec<PathBuf>,
    home: Option<PathBuf>,
    /// Handed in, so that a test needs no drive. A plain `fn` and not a closure: it remembers nothing.
    is_mount_point: fn(&Path) -> bool,
}

impl Crowd {
    /// `listed` are the ones the system names (Desktop, Downloads...) and any of the app's own.
    pub fn new(listed: Vec<PathBuf>, home: Option<PathBuf>, is_mount_point: fn(&Path) -> bool) -> Self {
        Self {
            listed,
            home,
            is_mount_point,
        }
    }

    /// Whether `folder` is one of them. A folder *inside* one is not.
    pub fn holds(&self, folder: &Path) -> bool {
        if folder.parent().is_none() {
            return true; // `/`, or a Windows drive
        }
        let real = folder.canonicalize().ok();
        // By its own name and by where it really is: `~/Documents` is often a link to another drive.
        let is_folder = |other: &Path| {
            same(folder, other)
                || real
                    .as_deref()
                    .is_some_and(|real| same(real, &other.canonicalize().unwrap_or_else(|_| other.into())))
        };
        self.listed.iter().any(|listed| is_folder(listed))
            // `ancestors` starts with the folder itself.
            || self.home.iter().flat_map(|home| home.ancestors()).any(is_folder)
            || (is_where_drives_are_mounted(folder) && (self.is_mount_point)(folder))
    }
}

/// Windows and macOS do not tell `Downloads` from `downloads`.
fn same(a: &Path, b: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Where Linux and macOS put a USB stick. Only there: a document that is its own mount (a share for one
/// book) is a document.
fn is_where_drives_are_mounted(folder: &Path) -> bool {
    let parts: Vec<_> = folder.iter().map(|part| part.to_string_lossy()).collect();
    let parts: Vec<&str> = parts.iter().map(|part| part.as_ref()).collect();
    matches!(
        parts.as_slice(),
        ["/", "media", _]
            | ["/", "media", _, _]
            | ["/", "run", "media", _, _]
            | ["/", "mnt", _]
            | ["/", "Volumes", _]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crowd() -> Crowd {
        Crowd::new(
            vec!["/home/ana/Downloads".into(), "/tmp".into()],
            Some("/home/ana".into()),
            |_| true,
        )
    }

    #[test]
    fn the_listed_folders_home_its_ancestors_and_a_root_are_crowded() {
        for folder in [
            "/home/ana/Downloads",
            "/home/ana/Downloads/",
            "/tmp",
            "/home/ana",
            "/home",
            "/",
        ] {
            assert!(crowd().holds(Path::new(folder)), "{folder}");
        }
    }

    #[test]
    fn a_folder_inside_a_crowded_one_is_not() {
        for folder in ["/home/ana/Downloads/thesis", "/home/ana/thesis", "/home/ben"] {
            assert!(!crowd().holds(Path::new(folder)), "{folder}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_mounted_drive_is_crowded_but_a_mounted_document_is_not() {
        for folder in [
            "/media/ana/USB",
            "/media/cdrom",
            "/run/media/ana/USB",
            "/mnt/data",
            "/Volumes/USB",
        ] {
            assert!(crowd().holds(Path::new(folder)), "{folder}");
        }
        for folder in ["/home/ana/thesis", "/media/ana/USB/thesis", "/mnt/data/thesis"] {
            assert!(!crowd().holds(Path::new(folder)), "{folder}");
        }
        let nothing_is_mounted = Crowd::new(Vec::new(), None, |_| false);
        assert!(
            nothing_is_mounted.holds(Path::new("/")),
            "a root needs no telling"
        );
        assert!(!nothing_is_mounted.holds(Path::new("/mnt/data")));
    }

    #[cfg(unix)]
    #[test]
    fn a_crowded_folder_is_known_by_its_real_place_too() {
        let place = tempfile::tempdir().unwrap();
        let (real, link) = (place.path().join("real"), place.path().join("Documents"));
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let crowd = Crowd::new(vec![link.clone()], None, |_| false);
        assert!(crowd.holds(&link) && crowd.holds(&real));
        assert!(!crowd.holds(place.path()));
    }
}
