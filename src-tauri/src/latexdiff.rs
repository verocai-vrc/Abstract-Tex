//! Change review between two commits, compiled (S11.4c; DESIGN.md §5.1, §5.7, §5.8).
//!
//! Owns three things. The *diff lane*: a second [`Orchestrator`], so a 30-second marked-up thesis
//! and the live build never cancel each other. The `.abstract-tex/latexdiff/` folder, which only
//! ever holds the latest comparison. And the order of the checks a comparison makes before it
//! writes a single file. `abstract_tex_latexdiff` does the marking-up and `abstract_tex_engine`
//! the compiling; this module only decides what goes where, and when.
//!
//! **What it must never do:**
//!
//! - Never cancel a live build, or be cancelled by one. The diff lane has its own generation
//!   counter, its own cancel and its own event, [`COMPILE_DIFF`].
//! - Never snapshot. A marked-up document is not a recoverable state of anything the author wrote.
//! - Never keep two comparisons on disk, and never write outside `.abstract-tex/latexdiff/`.
//! - Never ask for a compile on behalf of a comparison the author has already replaced.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use abstract_tex_engine::{BuildJob, Engine};
use abstract_tex_git::{Oid, Repository};
use abstract_tex_latexdiff::LatexdiffError;
use serde::Serialize;

use crate::compile::{CompileEvent, Orchestrator};

/// The event a diff build reports through: the same `CompileEvent` shape as the live lane's
/// `compile`, so the frontend decodes both the same way, under a name of its own so neither lane
/// can mistake the other's events for its own.
pub const COMPILE_DIFF: &str = "compile-diff";

/// Everything the diff lane keeps between comparisons. Lives in `AppState`.
pub struct DiffLane {
    /// Builds a comparison's marked-up document. The very struct the live lane uses, constructed
    /// a second time: its one rule — one build in flight, a newer request cancels the older — is
    /// exactly the rule this lane needs within itself, and being a separate instance is what keeps
    /// it from ever touching a live build.
    pub orchestrator: Orchestrator,
    /// Raised by every comparison asked for (see [`Ticket`]).
    latest_request: Arc<AtomicU64>,
    /// Held for the whole of one comparison — clearing the folder, exporting, marking up, asking
    /// for the compile — so two comparisons asked for in quick succession can never export into,
    /// or delete, each other's folders. It guards no data, only the order of steps, hence `()`.
    ///
    /// `tokio::sync::Mutex`, not `std::sync::Mutex`: this one is held across `.await`s, which a
    /// `std` lock must never be — a thread parked on one would block every other task sharing
    /// that thread. Tokio's version makes a second comparison *wait its turn* instead.
    one_at_a_time: tokio::sync::Mutex<()>,
}

impl DiffLane {
    pub fn new(engine: Option<Arc<dyn Engine>>) -> Self {
        Self {
            orchestrator: Orchestrator::new(engine),
            latest_request: Arc::new(AtomicU64::new(0)),
            one_at_a_time: tokio::sync::Mutex::new(()),
        }
    }

    /// A place in line for a comparison just asked for. It is the latest until the next one.
    pub fn take_ticket(&self) -> Ticket {
        let number = self.latest_request.fetch_add(1, Ordering::SeqCst) + 1;
        Ticket { number, latest: Arc::clone(&self.latest_request) }
    }

    /// Stop whatever the lane is doing for a comparison the author has walked away from.
    ///
    /// Two halves, because a comparison has two stages. A build already compiling is cancelled
    /// through the orchestrator. One still exporting or running `latexdiff` cannot be interrupted,
    /// so taking a ticket nobody holds makes it find, at its next check, that it is no longer the
    /// latest — and stop before asking for a compile. Idempotent: with nothing running it only
    /// raises the ticket count.
    pub fn cancel(&self) {
        self.take_ticket();
        self.orchestrator.cancel();
    }
}

/// One comparison's place in line. Exporting and running `latexdiff` cannot be cancelled halfway,
/// so a comparison checks its ticket instead — once it is its turn, and again once it has
/// rendered — and a newer one having been asked for in the meantime means it stops without
/// compiling anything. `Arc` because the count is shared with every ticket handed out.
#[derive(Debug, Clone)]
pub struct Ticket {
    number: u64,
    latest: Arc<AtomicU64>,
}

impl Ticket {
    pub fn is_latest(&self) -> bool {
        self.latest.load(Ordering::SeqCst) == self.number
    }
}

/// What a comparison needs from the open project, gathered under its lock and then released.
#[derive(Debug, Clone)]
pub struct ComparisonRequest {
    pub project_dir: PathBuf,
    /// Relative to `project_dir`, as `Project::root_file` gives it.
    pub root_file: PathBuf,
    pub latexdiff_dir: PathBuf,
    /// This machine's shell-escape consent for the project folder (S9.8). The export is that
    /// folder's own history — the same trust a sync already gives it.
    pub shell_escape: bool,
    /// The two commits, in the order the author clicked them.
    pub a: String,
    pub b: String,
}

/// Run one comparison, start to finish: wait its turn, stop whatever the lane was still
/// building, prepare the marked-up document, and hand it to the diff lane — reporting through
/// `on_event` the way the live lane does. Answers at once; the compile reports later.
pub async fn compare<F>(lane: &DiffLane, request: ComparisonRequest, on_event: F) -> Result<ComparisonStarted, String>
where
    F: Fn(CompileEvent) + Send + Sync + 'static,
{
    let ticket = lane.take_ticket();
    let _turn = lane.one_at_a_time.lock().await;
    if !ticket.is_latest() {
        return Ok(ComparisonStarted::Superseded); // a newer one was asked for while this waited
    }
    // The previous comparison's build may still be compiling from the folder this one is about to
    // clear. It is superseded either way, so stop it, and wait until it has really stopped.
    lane.orchestrator.cancel_and_wait().await;

    let blocking_request = request.clone();
    // `spawn_blocking`: libgit2, two whole-tree exports and a Perl process are all blocking work,
    // the reason `git::in_repository_blocking` gives for push and sync.
    let (plan, reused) = tauri::async_runtime::spawn_blocking(move || {
        let ComparisonRequest { project_dir, root_file, latexdiff_dir, a, b, .. } = blocking_request;
        prepare(Path::new("latexdiff"), &project_dir, &root_file, &latexdiff_dir, &a, &b)
    })
    .await
    .map_err(|e| e.to_string())??;

    if !ticket.is_latest() {
        return Ok(ComparisonStarted::Superseded); // asked for again while this one rendered
    }
    let (older, newer) = (plan.older.to_string(), plan.newer.to_string());
    if reused {
        return Ok(ComparisonStarted::Ready { older, newer, pdf_path: plan.pdf.to_string_lossy().into_owned() });
    }
    let generation = lane.orchestrator.request(plan.job(&request.root_file, request.shell_escape), None, on_event);
    Ok(ComparisonStarted::Building { older, newer, generation })
}

/// The TeX transcript of the comparison on disk, or an empty string when there is none.
///
/// `.abstract-tex/latexdiff/` only ever holds the latest comparison (see [`make_room`]), so no
/// pair needs naming — and nothing the webview sends is turned into a path, which is the point of
/// not offering a "read this log file" command. The log is the engine's `<stem>.log` inside the
/// comparison's `build/` folder, where the live lane's `read_log` finds its own.
pub fn read_comparison_log(latexdiff_dir: &Path, root_file: &Path) -> String {
    let stem = root_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
    let Ok(entries) = std::fs::read_dir(latexdiff_dir) else {
        return String::new(); // no comparison has ever run here
    };
    for entry in entries.flatten() {
        if let Ok(bytes) = std::fs::read(entry.path().join("build").join(format!("{stem}.log"))) {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    String::new()
}

/// A full commit id from the frontend, or a sentence saying it is not one.
pub fn parse_commit(id: &str) -> Result<Oid, String> {
    Oid::from_str(id).map_err(|_| format!("\"{id}\" is not a commit id."))
}

/// What `compare_revisions` answers with, straight away — the compile itself reports later, as
/// [`COMPILE_DIFF`] events carrying `generation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ComparisonStarted {
    /// The same pair was compared last time and its PDF is still there: nothing to do.
    Ready { older: String, newer: String, pdf_path: String },
    /// The marked-up document is compiling.
    Building { older: String, newer: String, generation: u64 },
    /// A newer comparison was asked for while this one was exporting. Not a refusal: the author
    /// is already waiting on the newer one, and this one says nothing.
    Superseded,
}

/// One comparison's folders, named for its pair so asking for the same pair again finds the last
/// answer still there (design interview A3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonDirs {
    pub root: PathBuf,
    pub old: PathBuf,
    pub new: PathBuf,
    pub build: PathBuf,
}

impl ComparisonDirs {
    pub fn new(latexdiff_dir: &Path, older: Oid, newer: Oid) -> Self {
        let root = latexdiff_dir.join(format!("{}-{}", short(older), short(newer)));
        Self { old: root.join("old"), new: root.join("new"), build: root.join("build"), root }
    }

    /// Where the engine writes the marked-up PDF: `<stem>.pdf` in `build/`, as both engines name it.
    pub fn pdf(&self, root_file: &Path) -> PathBuf {
        let stem = root_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
        self.build.join(format!("{stem}.pdf"))
    }
}

/// Everything a comparison has worked out before it writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub older: Oid,
    pub newer: Oid,
    /// The root file as a commit's tree spells it: from the repository's top, `/`-separated.
    pub tree_root_file: String,
    pub dirs: ComparisonDirs,
    /// Where the project sits inside the exported newer tree: the folder the engine runs in.
    pub project_in_export: PathBuf,
    /// Where the engine will write the marked-up PDF.
    pub pdf: PathBuf,
}

impl Plan {
    /// The diff build. No SyncTeX: not one line of the marked-up file is a line the author can
    /// edit (design interview A6).
    pub fn job(&self, root_file: &Path, shell_escape: bool) -> BuildJob {
        BuildJob {
            project_dir: self.project_in_export.clone(),
            root_file: root_file.to_path_buf(),
            out_dir: self.dirs.build.clone(),
            synctex: false,
            shell_escape,
        }
    }
}

/// Order the pair and refuse what cannot be compared, touching nothing on disk.
///
/// `a` and `b` arrive in click order; the plan is always older → newer (design interview A4).
/// `root_file` is relative to the project, which may be a subfolder of its repository.
pub fn plan(
    repository: &Repository,
    project_dir: &Path,
    root_file: &Path,
    latexdiff_dir: &Path,
    a: &str,
    b: &str,
) -> Result<Plan, String> {
    let (older, newer) = abstract_tex_git::older_first(repository, parse_commit(a)?, parse_commit(b)?).map_err(|e| e.to_string())?;

    let prefix = project_prefix(repository, project_dir);
    let tree_root_file = prefix.join(root_file).to_string_lossy().replace('\\', "/");
    for commit in [older, newer] {
        if !abstract_tex_git::has_path(repository, commit, &tree_root_file).map_err(|e| e.to_string())? {
            return Err(format!(
                "Commit {} has no {}: the root file was renamed, or did not exist yet, so there is \
                 nothing in it to compare.",
                short(commit),
                root_file.to_string_lossy().replace('\\', "/")
            ));
        }
    }

    let dirs = ComparisonDirs::new(latexdiff_dir, older, newer);
    Ok(Plan { older, newer, tree_root_file, project_in_export: dirs.new.join(&prefix), pdf: dirs.pdf(root_file), dirs })
}

/// Clear `.abstract-tex/latexdiff/` of everything but this plan's pair, and say whether that
/// pair's PDF is already there to reuse.
///
/// A folder for this very pair with no PDF in it is a comparison that failed or was interrupted.
/// It is cleared too, rather than exported over: an export only ever *adds* files, so a stale one
/// from a half-finished run could otherwise end up in the next compile.
pub fn make_room(latexdiff_dir: &Path, plan: &Plan) -> std::io::Result<bool> {
    let reusable = plan.pdf.is_file();
    let Ok(entries) = std::fs::read_dir(latexdiff_dir) else {
        return Ok(false); // no comparison has ever run here
    };
    for entry in entries {
        let path = entry?.path();
        if reusable && path == plan.dirs.root {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(reusable)
}

/// Everything a comparison does before its compile, in the order the design interview set (A3,
/// A5, A12): is `latexdiff` here, are both commits comparable, make room, then export and mark up
/// — or find this pair's PDF still there and do nothing more. `Ok((plan, reused))`.
///
/// Every refusal happens before anything on disk changes, the previous comparison included: a
/// machine with no `latexdiff` keeps the last PDF it managed to make. Blocking: libgit2, two
/// whole-tree exports and a Perl process — run it through `spawn_blocking`.
pub fn prepare(
    latexdiff: &Path,
    project_dir: &Path,
    root_file: &Path,
    latexdiff_dir: &Path,
    a: &str,
    b: &str,
) -> Result<(Plan, bool), String> {
    if !abstract_tex_latexdiff::is_installed(latexdiff) {
        return Err(sentence(LatexdiffError::NotInstalled));
    }
    let repository = abstract_tex_git::open(project_dir).map_err(|e| e.to_string())?;
    let plan = plan(&repository, project_dir, root_file, latexdiff_dir, a, b)?;
    let reused = make_room(latexdiff_dir, &plan).map_err(|e| e.to_string())?;
    if !reused {
        render(latexdiff, &repository, &plan)?;
    }
    Ok((plan, reused))
}

/// Export both revisions and mark up the newer one's root file (S11.4b), with `latexdiff`'s own
/// defaults: `--flatten` is the only flag (design interview A7, A8).
pub fn render(latexdiff: &Path, repository: &Repository, plan: &Plan) -> Result<(), String> {
    abstract_tex_latexdiff::render_with(
        latexdiff,
        repository,
        plan.older,
        plan.newer,
        &plan.tree_root_file,
        &plan.dirs.old,
        &plan.dirs.new,
    )
    .map(|_| ())
    .map_err(sentence)
}

/// A comparison's refusal as the Graph shows it (design interview A12). On Windows, one more
/// clause: MiKTeX's `latexdiff` is a Perl script and MiKTeX installs no Perl, while TeX Live
/// brings its own. Nothing here can tell which distribution a machine has, so the sentence names
/// both and is right either way.
pub fn sentence(error: LatexdiffError) -> String {
    match error {
        LatexdiffError::NotInstalled if cfg!(windows) => {
            format!("{error} With MiKTeX it also needs Perl, which MiKTeX does not install; TeX Live brings its own.")
        }
        other => other.to_string(),
    }
}

/// Where the project sits inside its repository: empty for a project at the top, `paper` for one
/// in a subfolder. `abstract_tex_git::open` *discovers* the repository (S10.1), so the two are not
/// always the same folder — and a commit's tree is spelled from the repository's top.
fn project_prefix(repository: &Repository, project_dir: &Path) -> PathBuf {
    let Some(workdir) = repository.workdir() else { return PathBuf::new() };
    // Canonical on both sides, or a symlinked home folder (or Windows's `\\?\` form on one side
    // only) makes a project inside the repository look like one outside it.
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(project_dir).strip_prefix(canonical(workdir)).map(Path::to_path_buf).unwrap_or_default()
}

/// The seven characters `git log --oneline` shows, as the Graph does.
fn short(id: Oid) -> String {
    id.to_string().chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use abstract_tex_engine::{BuildOutcome, EngineError, EngineInfo, ProgressSink};
    use std::fs;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    /// A repository whose project lives in `paper/`, with two commits of `paper/main.tex`.
    fn two_commits() -> (tempfile::TempDir, Repository, Oid, Oid) {
        let tmp = tempfile::tempdir().unwrap();
        let repository = Repository::init(tmp.path()).unwrap();
        fs::create_dir(tmp.path().join("paper")).unwrap();
        let commit = |text: &str| {
            fs::write(tmp.path().join("paper/main.tex"), text).unwrap();
            let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();
            let mut index = repository.index().unwrap();
            index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
            index.write().unwrap();
            let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
            let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
            let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
            repository.commit(Some("HEAD"), &who, &who, text, &tree, &parents).unwrap()
        };
        let first = commit("first draft\n");
        let second = commit("second draft\n");
        (tmp, repository, first, second)
    }

    /// Clicked newer-first or older-first, the plan — folders, PDF and all — is the same one, and
    /// a project in a subfolder of its repository is found inside the export where it belongs.
    #[test]
    fn a_plan_is_older_to_newer_whatever_order_it_was_asked_in() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let root = Path::new("main.tex");

        let forwards = plan(&repository, &project, root, &latexdiff_dir, &first.to_string(), &second.to_string()).unwrap();
        let backwards = plan(&repository, &project, root, &latexdiff_dir, &second.to_string(), &first.to_string()).unwrap();

        assert_eq!(forwards, backwards);
        assert_eq!((forwards.older, forwards.newer), (first, second));
        assert_eq!(forwards.tree_root_file, "paper/main.tex");
        assert_eq!(forwards.project_in_export, forwards.dirs.new.join("paper"));
        assert_eq!(forwards.pdf, forwards.dirs.build.join("main.pdf"));
        assert!(forwards.dirs.root.ends_with(format!("{}-{}", short(first), short(second))));
    }

    /// A root file that one of the two commits lacks is refused by name, before anything exists.
    #[test]
    fn a_commit_without_the_root_file_is_refused_with_its_id() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");

        let refusal = plan(&repository, &project, Path::new("thesis.tex"), &latexdiff_dir, &first.to_string(), &second.to_string())
            .unwrap_err();

        assert!(refusal.contains(&short(first)) && refusal.contains("thesis.tex"), "{refusal}");
        assert!(!latexdiff_dir.exists(), "nothing may be written for a refused comparison");
    }

    /// Only the latest comparison survives: another pair's folder goes, and this pair's own folder
    /// is kept — and reported reusable — only when its PDF made it.
    #[test]
    fn making_room_keeps_only_this_pair_and_only_if_it_finished() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let the_plan = plan(&repository, &project, Path::new("main.tex"), &latexdiff_dir, &first.to_string(), &second.to_string()).unwrap();
        let someone_else = latexdiff_dir.join("aaaaaaa-bbbbbbb");
        fs::create_dir_all(someone_else.join("build")).unwrap();

        // This pair started once and never got as far as a PDF: cleared, not reused.
        fs::create_dir_all(the_plan.dirs.new.join("paper")).unwrap();
        assert!(!make_room(&latexdiff_dir, &the_plan).unwrap());
        assert!(!someone_else.exists() && !the_plan.dirs.root.exists());

        // This pair finished: kept, and reused.
        fs::create_dir_all(&the_plan.dirs.build).unwrap();
        fs::write(&the_plan.pdf, "%PDF").unwrap();
        fs::create_dir_all(&someone_else).unwrap();
        assert!(make_room(&latexdiff_dir, &the_plan).unwrap());
        assert!(the_plan.pdf.is_file() && !someone_else.exists());
    }

    /// No `latexdiff` on this machine: refused with the install sentence, and the last
    /// comparison's folder — the only PDF the author may have — is left exactly where it was.
    #[test]
    fn a_machine_without_latexdiff_is_refused_before_anything_is_cleared() {
        let (tmp, _repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let last_time = latexdiff_dir.join("aaaaaaa-bbbbbbb/build");
        fs::create_dir_all(&last_time).unwrap();

        let missing = tmp.path().join("no-latexdiff-here");
        let refusal = prepare(&missing, &project, Path::new("main.tex"), &latexdiff_dir, &first.to_string(), &second.to_string())
            .unwrap_err();

        assert!(refusal.contains("latexdiff isn't installed"), "{refusal}");
        assert!(last_time.is_dir(), "the previous comparison must survive a refusal");
        assert_eq!(fs::read_dir(&latexdiff_dir).unwrap().count(), 1, "nothing new was exported");
    }

    /// Two comparisons asked for while a third holds the lane: the first, once its turn comes, sees
    /// it has been replaced and stops — touching nothing — while the second goes ahead (here into
    /// a folder that is not a repository, which is refused, so neither compiles anything).
    #[tokio::test]
    async fn a_comparison_replaced_while_it_waited_its_turn_does_nothing() {
        let lane = DiffLane::new(None);
        let not_a_repository = tempfile::tempdir().unwrap();
        let request = || ComparisonRequest {
            project_dir: not_a_repository.path().to_path_buf(),
            root_file: "main.tex".into(),
            latexdiff_dir: not_a_repository.path().join(".abstract-tex/latexdiff"),
            shell_escape: false,
            a: "0".repeat(40),
            b: "1".repeat(40),
        };

        let someone_else = lane.one_at_a_time.lock().await;
        let first = compare(&lane, request(), |_: CompileEvent| {});
        let second = compare(&lane, request(), |_: CompileEvent| {});
        // `join!` polls in order: both comparisons take their tickets and queue for the lock
        // before this one lets go of it. Tokio's mutex is fair, so the first queued goes first.
        let let_go = async {
            tokio::task::yield_now().await;
            drop(someone_else);
        };
        let (first, second, ()) = tokio::join!(first, second, let_go);

        assert_eq!(first, Ok(ComparisonStarted::Superseded));
        assert!(second.is_err(), "the latest comparison runs, and is refused by name: {second:?}");
    }

    #[test]
    fn the_comparisons_own_log_is_read_from_its_build_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let latexdiff_dir = tmp.path().join("latexdiff");
        assert_eq!(read_comparison_log(&latexdiff_dir, Path::new("main.tex")), "", "no comparison yet");

        let build = latexdiff_dir.join("aaaaaaa-bbbbbbb/build");
        fs::create_dir_all(&build).unwrap();
        assert_eq!(read_comparison_log(&latexdiff_dir, Path::new("main.tex")), "", "a build with no log");

        fs::write(build.join("main.log"), "! Undefined control sequence.\n").unwrap();
        fs::write(build.join("other.log"), "not this one").unwrap();
        assert_eq!(read_comparison_log(&latexdiff_dir, Path::new("main.tex")), "! Undefined control sequence.\n");
        assert_eq!(read_comparison_log(&latexdiff_dir, Path::new("chapters/other.tex")), "not this one");
    }

    /// Walking away from a comparison that is still exporting makes it stop before it compiles,
    /// and one already compiling is cancelled, without a newer comparison having to be asked for.
    #[tokio::test]
    async fn cancelling_stops_a_build_and_retires_a_comparison_still_preparing() {
        let lane = DiffLane::new(Some(Arc::new(SleepyEngine(Duration::from_secs(5))) as Arc<dyn Engine>));
        let preparing = lane.take_ticket();
        let (tx, mut rx) = mpsc::unbounded_channel::<CompileEvent>();
        lane.orchestrator.request(job(), None, move |event| {
            let _ = tx.send(event);
        });

        lane.cancel();

        assert!(!preparing.is_latest(), "a comparison still exporting must find it was retired");
        let outcome = tokio::time::timeout(Duration::from_millis(500), async {
            while let Some(event) = rx.recv().await {
                if matches!(event, CompileEvent::Finished { .. }) {
                    return true;
                }
            }
            false
        })
        .await;
        assert_ne!(outcome, Ok(true), "a cancelled build must not report a finished comparison");
    }

    #[test]
    fn only_the_newest_ticket_is_the_latest() {
        let lane = DiffLane::new(None);
        let first = lane.take_ticket();
        assert!(first.is_latest());
        let second = lane.take_ticket();
        assert!(!first.is_latest() && second.is_latest());
    }

    /// The frontend reads `pdfPath`, and switches on `status`.
    #[test]
    fn a_comparison_answer_serialises_in_camel_case() {
        let ready = ComparisonStarted::Ready { older: "a".into(), newer: "b".into(), pdf_path: "/p.pdf".into() };
        let json = serde_json::to_value(&ready).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["pdfPath"], "/p.pdf");
        assert_eq!(serde_json::to_value(ComparisonStarted::Superseded).unwrap()["status"], "superseded");
    }

    /// Builds by sleeping, so a test controls which build is still running when.
    struct SleepyEngine(Duration);

    #[async_trait::async_trait]
    impl Engine for SleepyEngine {
        async fn probe(&self) -> Result<EngineInfo, EngineError> {
            Ok(EngineInfo { name: "sleepy".into(), version: "0".into(), path: PathBuf::new() })
        }

        async fn build(&self, _: &BuildJob, cancel: CancellationToken, _: Option<ProgressSink>) -> Result<BuildOutcome, EngineError> {
            tokio::select! {
                _ = tokio::time::sleep(self.0) => Ok(BuildOutcome {
                    success: true, pdf: None, log: None, synctex: None, stderr: String::new(),
                    exit_code: Some(0), duration: self.0, steps: Default::default(),
                }),
                _ = cancel.cancelled() => Err(EngineError::Cancelled),
            }
        }
    }

    fn job() -> BuildJob {
        BuildJob { project_dir: ".".into(), root_file: "main.tex".into(), out_dir: "build".into(), synctex: false, shell_escape: false }
    }

    /// The card's done-when, in one test: a live build and two diff builds requested together. The
    /// live one finishes; the second diff cancels the first and finishes itself. The first diff
    /// never says `finished`, and neither lane's request touched the other's.
    #[tokio::test]
    async fn the_diff_lane_and_the_live_lane_never_cancel_each_other() {
        let engine = || Some(Arc::new(SleepyEngine(Duration::from_millis(120))) as Arc<dyn Engine>);
        let live = Orchestrator::new(engine());
        let lane = DiffLane::new(engine());
        let (tx, mut rx) = mpsc::unbounded_channel::<(&'static str, CompileEvent)>();

        let send = |lane_name: &'static str| {
            let tx = tx.clone();
            move |event: CompileEvent| {
                let _ = tx.send((lane_name, event));
            }
        };
        let live_generation = live.request(job(), None, send("live"));
        let first_diff = lane.orchestrator.request(job(), None, send("diff"));
        let second_diff = lane.orchestrator.request(job(), None, send("diff"));

        let mut finished = Vec::new();
        while let Ok(Some((lane_name, event))) = tokio::time::timeout(Duration::from_millis(600), rx.recv()).await {
            if let CompileEvent::Finished { generation, .. } = event {
                finished.push((lane_name, generation));
            }
        }
        finished.sort();
        assert_eq!(finished, vec![("diff", second_diff), ("live", live_generation)]);
        assert_ne!(first_diff, second_diff);
    }
}
