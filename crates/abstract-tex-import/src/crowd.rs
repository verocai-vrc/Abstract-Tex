//! Folders that hold many things and are no document: Downloads, the home folder, a whole drive. Opened
//! as one, every file in them would be listed, watched and copied into the saved versions.
//!
//! They are on drives that have nothing to do with the folder in question, and any of those can have
//! stopped answering. So nothing here waits for one of them longer than [`LIMIT`] ([`SlowPlaces`]).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long a folder has to say where it really is.
const LIMIT: Duration = Duration::from_secs(1);

/// A question to a disk. `Box<dyn FnOnce>`: any closure, asked once, whatever it captured.
pub type Question = Box<dyn FnOnce() -> Option<PathBuf> + Send>;

/// One question that is being asked. A `Condvar` is what lets several threads sleep until the answer is
/// there, or until they have waited long enough.
struct Asked {
    since: Instant,
    /// `None` until it answered.
    answer: Mutex<Option<Option<PathBuf>>>,
    answered: Condvar,
}

impl Asked {
    /// The answer, waiting until [`LIMIT`] after it was *asked*: whoever asks again about a drive that is
    /// already known not to answer does not wait again.
    fn wait(&self) -> Option<PathBuf> {
        let left = LIMIT.saturating_sub(self.since.elapsed());
        let answer = self.answer.lock().unwrap();
        let (answer, _timed_out) = self
            .answered
            .wait_timeout_while(answer, left, |answer| answer.is_none())
            .unwrap();
        answer.clone().flatten()
    }
}

/// Asks disks, each question on a thread of its own. A thread stuck on a dead drive cannot be ended, so
/// what is still being asked is never asked again, and a dead drive has one thread, however often it is
/// asked. A copy shares the questions being asked.
#[derive(Clone)]
pub struct SlowPlaces {
    /// By what is asked about: a folder, or the name of one that the system knows.
    running: Arc<Mutex<HashMap<PathBuf, Arc<Asked>>>>,
    /// Where a folder really is. Handed in, so that a test needs no dead drive.
    real_place: Arc<RealPlace>,
}

type RealPlace = dyn Fn(&Path) -> Option<PathBuf> + Send + Sync;

impl Default for SlowPlaces {
    fn default() -> Self {
        Self::asking(|folder| folder.canonicalize().ok())
    }
}

impl SlowPlaces {
    pub fn asking(real_place: impl Fn(&Path) -> Option<PathBuf> + Send + Sync + 'static) -> Self {
        Self {
            running: Arc::default(),
            real_place: Arc::new(real_place),
        }
    }

    /// Ask them all at once; `None` for one that has no answer in time.
    pub fn ask(&self, questions: Vec<(PathBuf, Question)>) -> Vec<Option<PathBuf>> {
        let asked: Vec<Arc<Asked>> = questions
            .into_iter()
            .map(|(about, question)| self.begin(about, question))
            .collect();
        asked.iter().map(|asked| asked.wait()).collect()
    }

    fn begin(&self, about: PathBuf, question: Question) -> Arc<Asked> {
        let mut running = self.running.lock().unwrap();
        if let Some(asked) = running.get(&about) {
            return Arc::clone(asked);
        }
        let asked = Arc::new(Asked {
            since: Instant::now(),
            answer: Mutex::new(None),
            answered: Condvar::new(),
        });
        running.insert(about.clone(), Arc::clone(&asked));
        let (answering, still_running) = (Arc::clone(&asked), Arc::clone(&self.running));
        // A thread that cannot be started never answers, which is what waiting for it then says.
        let _ = std::thread::Builder::new().spawn(move || {
            let answer = question();
            still_running.lock().unwrap().remove(&about);
            *answering.answer.lock().unwrap() = Some(answer);
            answering.answered.notify_all();
        });
        asked
    }

    fn real_places(&self, folders: &[&Path]) -> Vec<Option<PathBuf>> {
        let question = |folder: &Path| -> Question {
            let (real_place, folder) = (Arc::clone(&self.real_place), folder.to_path_buf());
            Box::new(move || real_place(&folder))
        };
        self.ask(
            folders
                .iter()
                .map(|folder| (folder.to_path_buf(), question(folder)))
                .collect(),
        )
    }
}

/// The crowded folders of this computer.
pub struct Crowd {
    listed: Vec<PathBuf>,
    home: Option<PathBuf>,
    /// Handed in, so that a test needs no drive. A plain `fn` and not a closure: it remembers nothing.
    is_mount_point: fn(&Path) -> bool,
    places: SlowPlaces,
}

impl Crowd {
    /// `listed` are the ones the system names (Desktop, Downloads...) and any of the app's own.
    pub fn new(
        listed: Vec<PathBuf>,
        home: Option<PathBuf>,
        is_mount_point: fn(&Path) -> bool,
        places: SlowPlaces,
    ) -> Self {
        Self {
            listed,
            home,
            is_mount_point,
            places,
        }
    }

    /// Whether `folder` is one of them. A folder *inside* one is not. Asks the disk `folder` is on, and
    /// the others within [`LIMIT`]: one that does not answer is known by its spelling only.
    pub fn holds(&self, folder: &Path) -> bool {
        if folder.parent().is_none() {
            return true; // `/`, or a Windows drive
        }
        let others: Vec<&Path> = self
            .listed
            .iter()
            .map(PathBuf::as_path)
            // `ancestors` starts with the folder itself.
            .chain(self.home.iter().flat_map(|home| home.ancestors()))
            .collect();
        // By its own name, which costs nothing, and by where it really is: `~/Documents` is often a link
        // to another drive.
        if others.iter().any(|other| same(folder, other)) || self.is_one_account_in_the_cloud(folder) {
            return true;
        }
        if let Ok(real) = folder.canonicalize() {
            let real_others = self.places.real_places(&others);
            let is_real = |(other, real_other): (&&Path, &Option<PathBuf>)| {
                same(&real, real_other.as_deref().unwrap_or(other))
            };
            if others.iter().zip(&real_others).any(is_real) || self.is_one_account_in_the_cloud(&real) {
                return true;
            }
        }
        is_where_drives_are_mounted(folder) && (self.is_mount_point)(folder)
    }

    /// macOS has a folder for each cloud account in one place, under names nobody can list beforehand
    /// (`OneDrive-University of X`).
    fn is_one_account_in_the_cloud(&self, folder: &Path) -> bool {
        let accounts = self.home.as_ref().map(|home| home.join("Library/CloudStorage"));
        matches!((accounts, folder.parent()), (Some(accounts), Some(parent)) if same(&accounts, parent))
    }
}

/// The folders a cloud service keeps in step, as far as they can be known without asking a disk.
/// `variable` reads one of the environment's.
pub fn cloud_folders(home: Option<&Path>, variable: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    let named = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .into_iter()
        .filter_map(variable)
        .filter(|folder| folder.is_absolute());
    let usual = [
        "OneDrive",
        "Dropbox",
        "Google Drive",
        "Nextcloud",
        "Library/Mobile Documents/com~apple~CloudDocs",
    ];
    let in_home = home
        .into_iter()
        .flat_map(|home| usual.map(|name| home.join(name)));
    named.chain(in_home).collect()
}

/// Whether two spellings are of one folder, as far as the spelling says: the separators do not count, nor
/// one at the end. Windows and macOS do not tell `Downloads` from `downloads` either.
fn same(a: &Path, b: &Path) -> bool {
    if !cfg!(any(windows, target_os = "macos")) {
        return a == b;
    }
    let lowered = |part: Component| part.as_os_str().to_string_lossy().to_lowercase();
    a.components().map(lowered).eq(b.components().map(lowered))
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
            SlowPlaces::default(),
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
        let nothing_is_mounted = Crowd::new(Vec::new(), None, |_| false, SlowPlaces::default());
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
        let crowd = Crowd::new(vec![link.clone()], None, |_| false, SlowPlaces::default());
        assert!(crowd.holds(&link) && crowd.holds(&real));
        assert!(!crowd.holds(place.path()));
    }

    /// A folder on a drive that does not answer.
    const DEAD: &str = "/nowhere/network/writing";

    /// Every folder is where it is called, except that [`DEAD`] never says. Counts how often that is asked.
    fn one_dead_drive() -> (SlowPlaces, Arc<Mutex<u32>>) {
        let times_asked = Arc::new(Mutex::new(0));
        let counted = Arc::clone(&times_asked);
        let places = SlowPlaces::asking(move |folder| {
            if folder == Path::new(DEAD) {
                *counted.lock().unwrap() += 1;
                std::thread::sleep(Duration::from_secs(20));
            }
            folder.canonicalize().ok()
        });
        (places, times_asked)
    }

    #[test]
    fn a_drive_that_does_not_answer_delays_one_question_and_has_one_thread() {
        let place = tempfile::tempdir().unwrap();
        let (downloads, thesis) = (place.path().join("Downloads"), place.path().join("thesis"));
        std::fs::create_dir(&downloads).unwrap();
        std::fs::create_dir(&thesis).unwrap();
        let (places, times_asked) = one_dead_drive();
        let crowd = || {
            Crowd::new(
                vec![DEAD.into(), downloads.clone()],
                None,
                |_| false,
                places.clone(),
            )
        };
        let asked = Instant::now();
        assert!(!crowd().holds(&thesis));
        assert!(asked.elapsed() < LIMIT * 2, "{:?}", asked.elapsed());
        let asked = Instant::now();
        for _ in 0..20 {
            assert!(!crowd().holds(&thesis));
            assert!(crowd().holds(&downloads));
            assert!(crowd().holds(Path::new(DEAD)), "by its spelling");
        }
        assert!(asked.elapsed() < LIMIT, "{:?}", asked.elapsed());
        assert_eq!(*times_asked.lock().unwrap(), 1);
    }

    #[test]
    fn a_question_that_was_answered_is_asked_again_the_next_time() {
        let places = SlowPlaces::default();
        let ask = |answer: &'static str| -> Option<PathBuf> {
            places
                .ask(vec![("desktop".into(), Box::new(move || Some(answer.into())))])
                .remove(0)
        };
        assert_eq!(ask("/a"), Some("/a".into()));
        assert_eq!(ask("/b"), Some("/b".into()));
    }

    #[test]
    fn the_folders_a_cloud_service_keeps_are_crowded_and_what_is_in_them_is_not() {
        // `home` as the system at hand spells one: Windows would not call "/home/ana" absolute.
        let home = std::env::temp_dir().join("ana");
        let elsewhere = std::env::temp_dir().join("cloud").join("OneDrive - University");
        let listed = cloud_folders(Some(&home), |name| match name {
            "OneDriveCommercial" => Some(elsewhere.clone()),
            "OneDrive" => Some("relative".into()),
            _ => None,
        });
        assert!(!listed.contains(&PathBuf::from("relative")));
        let crowd = Crowd::new(listed, Some(home.clone()), |_| false, SlowPlaces::default());
        for folder in [
            "OneDrive",
            "Dropbox",
            "Google Drive",
            "Nextcloud",
            "Library/Mobile Documents/com~apple~CloudDocs",
            "Library/CloudStorage/GoogleDrive-ana@example.org",
        ] {
            assert!(crowd.holds(&home.join(folder)), "{folder}");
            assert!(!crowd.holds(&home.join(folder).join("thesis")), "{folder}/thesis");
        }
        assert!(crowd.holds(&elsewhere) && !crowd.holds(&elsewhere.join("thesis")));
    }
}
