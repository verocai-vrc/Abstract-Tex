//! Small pictures of page one of the documents used last, for Home (S17.15).
//!
//! Kept in the app's local data on this computer (`recent-pictures/`), never in a document and never sent
//! anywhere: a manuscript can be confidential. A picture is there only while its document is on the list,
//! and nothing depends on one being there. The window draws them, so what arrives here is only a claim to
//! be one: it is checked before it is kept.

use std::fs;
use std::path::{Path, PathBuf};

use crate::project::write_bytes_atomically;
use crate::recents::{self, RecentDocument};

/// The first bytes of every PNG.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Far more than a picture 320 pixels wide needs.
pub const LARGEST: usize = 500_000;

#[derive(Debug, thiserror::Error)]
pub enum PictureError {
    #[error("that is not a picture of a page")]
    NotAPicture,
    #[error("that document is not in the list")]
    NotListed,
    #[error("the picture could not be kept: {0}")]
    CannotWrite(#[from] std::io::Error),
}

/// Where they are kept. One per app; handed to commands as Tauri state.
#[derive(Debug, Clone)]
pub struct Pictures {
    folder: PathBuf,
}

impl Pictures {
    pub fn at(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// Where the picture of the document in `folder` is, if there is one.
    pub fn path_for(&self, folder: &Path) -> PathBuf {
        self.folder
            .join(name_for(&folder.to_string_lossy(), cfg!(windows)))
    }

    /// Keep `png` for a document that is on the list, and take the others' off that no longer are: the
    /// list keeps its newest thirty, and drops the rest without telling anybody.
    pub fn save(&self, listed: &[RecentDocument], folder: &Path, png: &[u8]) -> Result<(), PictureError> {
        let wanted = folder.to_string_lossy();
        if !listed
            .iter()
            .any(|document| recents::same_folder(&document.folder, &wanted, cfg!(windows)))
        {
            return Err(PictureError::NotListed);
        }
        if png.len() > LARGEST || !png.starts_with(PNG) {
            return Err(PictureError::NotAPicture);
        }
        fs::create_dir_all(&self.folder)?;
        write_bytes_atomically(&self.path_for(folder), png)?;
        self.keep_only(listed);
        Ok(())
    }

    fn keep_only(&self, listed: &[RecentDocument]) {
        let names: Vec<String> = listed
            .iter()
            .map(|document| name_for(&document.folder, cfg!(windows)))
            .collect();
        for entry in fs::read_dir(&self.folder).into_iter().flatten().flatten() {
            if !names.iter().any(|name| entry.file_name() == name.as_str()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Remove a document's picture. Silent when there is none.
    pub fn forget(&self, folder: &Path) {
        let _ = fs::remove_file(self.path_for(folder));
    }

    /// Remove them all.
    pub fn clear(&self) {
        self.keep_only(&[]);
    }
}

/// A file name that says nothing of the folder it stands for: these pictures are for whoever uses this
/// computer, and a name like `my-resignation.png` would also be for whoever looks over a list of files.
///
/// A number made from the folder's path (FNV-1a, two lines of arithmetic). Not `std`'s own hasher, which
/// may give another number in another version of Rust, and then every picture would be lost; and not a
/// cryptographic one, as nobody gains anything from two folders with one picture.
fn name_for(folder: &str, ignoring_case: bool) -> String {
    let folder = if ignoring_case {
        folder.to_lowercase()
    } else {
        folder.to_string()
    };
    let mut number: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in folder.bytes() {
        number ^= u128::from(byte);
        number = number.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    format!("{number:032x}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(folders: &[&str]) -> Vec<RecentDocument> {
        folders
            .iter()
            .map(|folder| RecentDocument {
                folder: folder.to_string(),
                root_file: "main.tex".into(),
                title: "T".into(),
                used_at: 1,
            })
            .collect()
    }

    fn png(size: usize) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        bytes.resize(size, 0);
        bytes
    }

    #[test]
    fn a_picture_is_kept_under_a_name_that_is_not_the_folder() {
        let data = tempfile::tempdir().unwrap();
        let pictures = Pictures::at(data.path().join("recent-pictures"));
        let folder = Path::new("/home/ana/secret-thesis");
        pictures
            .save(&listed(&["/home/ana/secret-thesis"]), folder, &png(100))
            .unwrap();
        let path = pictures.path_for(folder);
        assert!(path.is_file());
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for part in ["home", "ana", "secret", "thesis"] {
            assert!(!name.contains(part), "{name}");
        }
        // The same tomorrow, and in the next version: this exact name is on people's disks.
        assert_eq!(name, "e933ab675f7c01a6d507ed37ee5ed680.png");
        assert_eq!(name_for("", false), "6c62272e07bb014262b821756295c58d.png");
        assert_ne!(name, name_for("/home/ana/secret-thesis2", false));
        assert_eq!(name_for("C:\\A", true), name_for("c:\\a", true));
        assert_ne!(name_for("/A", false), name_for("/a", false));
    }

    #[test]
    fn only_a_png_under_500_kb_is_kept() {
        let data = tempfile::tempdir().unwrap();
        let pictures = Pictures::at(data.path().join("recent-pictures"));
        let folder = Path::new("/a");
        let mut other = png(100);
        other[0] = b'<';
        for refused in [other, png(500_001), b"\x89PNG".to_vec(), Vec::new()] {
            let answer = pictures.save(&listed(&["/a"]), folder, &refused);
            assert!(matches!(answer, Err(PictureError::NotAPicture)));
            assert!(!pictures.path_for(folder).exists());
        }
        pictures.save(&listed(&["/a"]), folder, &png(500_000)).unwrap();
        assert!(pictures.path_for(folder).is_file());
    }

    #[test]
    fn a_folder_that_is_not_in_the_list_gets_no_picture() {
        let data = tempfile::tempdir().unwrap();
        let pictures = Pictures::at(data.path().join("recent-pictures"));
        let answer = pictures.save(&listed(&["/a"]), Path::new("/b"), &png(100));
        assert!(matches!(answer, Err(PictureError::NotListed)));
        assert!(!pictures.path_for(Path::new("/b")).exists());
    }

    #[test]
    fn the_pictures_of_documents_that_fell_off_the_list_go() {
        let data = tempfile::tempdir().unwrap();
        let pictures = Pictures::at(data.path().join("recent-pictures"));
        let [a, b] = [Path::new("/a"), Path::new("/b")];
        pictures.save(&listed(&["/a", "/b"]), a, &png(100)).unwrap();
        pictures.save(&listed(&["/a", "/b"]), b, &png(100)).unwrap();
        assert!(pictures.path_for(a).exists());
        pictures.save(&listed(&["/b"]), b, &png(100)).unwrap();
        assert!(!pictures.path_for(a).exists());
        assert!(pictures.path_for(b).exists());
        pictures.clear();
        assert!(!pictures.path_for(b).exists());
    }
}
