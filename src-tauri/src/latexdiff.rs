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
use abstract_tex_latexdiff::{LatexdiffError, Markup};
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
        Ticket {
            number,
            latest: Arc::clone(&self.latest_request),
        }
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
    /// `Standard` the first time; `Simpler` for the retry after a comparison failed to compile (A7).
    pub markup: Markup,
}

/// Run one comparison, start to finish: wait its turn, stop whatever the lane was still
/// building, prepare the marked-up document, and hand it to the diff lane — reporting through
/// `on_event` the way the live lane does. Answers at once; the compile reports later.
pub async fn compare<F>(
    lane: &DiffLane,
    request: ComparisonRequest,
    on_event: F,
) -> Result<ComparisonStarted, String>
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
        let ComparisonRequest {
            project_dir,
            root_file,
            latexdiff_dir,
            a,
            b,
            markup,
            ..
        } = blocking_request;
        prepare(
            Path::new("latexdiff"),
            &project_dir,
            &root_file,
            &latexdiff_dir,
            &a,
            &b,
            markup,
        )
    })
    .await
    .map_err(|e| e.to_string())??;

    if !ticket.is_latest() {
        return Ok(ComparisonStarted::Superseded); // asked for again while this one rendered
    }
    let (older, newer) = (plan.older.to_string(), plan.newer.id());
    if reused {
        return Ok(ComparisonStarted::Ready {
            older,
            newer,
            pdf_path: plan.pdf.to_string_lossy().into_owned(),
        });
    }
    let generation =
        lane.orchestrator
            .request(plan.job(&request.root_file, request.shell_escape), None, on_event);
    Ok(ComparisonStarted::Building {
        older,
        newer,
        generation,
    })
}

/// The TeX transcript of the comparison on disk, or an empty string when there is none.
///
/// `.abstract-tex/latexdiff/` only ever holds the latest comparison (see [`make_room`]), so no
/// pair needs naming — and nothing the webview sends is turned into a path, which is the point of
/// not offering a "read this log file" command. The log is the engine's `<stem>.log` inside the
/// comparison's `build/` folder, where the live lane's `read_log` finds its own.
pub fn read_comparison_log(latexdiff_dir: &Path, root_file: &Path) -> String {
    let stem = root_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
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

/// What the frontend sends in place of a commit id for "the files as they are now" (A11).
pub const WORKING_TREE: &str = "working-tree";

/// The newer side of a comparison: a commit, or the working copy (A11). The older side is always
/// a commit, because a working copy that is older than a commit is not a thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newer {
    Commit(Oid),
    WorkingTree,
}

impl Newer {
    /// What the frontend is told and sends back: the full commit id, or [`WORKING_TREE`].
    pub fn id(&self) -> String {
        match self {
            Newer::Commit(commit) => commit.to_string(),
            Newer::WorkingTree => WORKING_TREE.to_string(),
        }
    }

    /// The folder-name part: seven characters of a commit, or `working`.
    fn short(&self) -> String {
        match self {
            Newer::Commit(commit) => short(*commit),
            Newer::WorkingTree => "working".to_string(),
        }
    }
}

/// The newer side as the frontend named it: a commit id, or [`WORKING_TREE`].
pub fn parse_newer(id: &str) -> Result<Newer, String> {
    if id == WORKING_TREE {
        Ok(Newer::WorkingTree)
    } else {
        parse_commit(id).map(Newer::Commit)
    }
}

/// The frontend's optional `simpler` flag as a [`Markup`]: absent means the first attempt, which
/// is `latexdiff`'s own defaults.
pub fn markup_from(simpler: Option<bool>) -> Markup {
    if simpler == Some(true) {
        Markup::Simpler
    } else {
        Markup::Standard
    }
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
    Ready {
        older: String,
        newer: String,
        pdf_path: String,
    },
    /// The marked-up document is compiling.
    Building {
        older: String,
        newer: String,
        generation: u64,
    },
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
    /// `markup` is part of the name: the same pair under a different setting is a different
    /// document, so a retry never finds the failed attempt's folder or a standard PDF and takes
    /// it for its own.
    pub fn new(latexdiff_dir: &Path, older: Oid, newer: Newer, markup: Markup) -> Self {
        let suffix = match markup {
            Markup::Standard => "",
            Markup::Simpler => "-simpler",
        };
        let root = latexdiff_dir.join(format!("{}-{}{suffix}", short(older), newer.short()));
        Self {
            old: root.join("old"),
            new: root.join("new"),
            build: root.join("build"),
            root,
        }
    }

    /// Where the engine writes the marked-up PDF: `<stem>.pdf` in `build/`, as both engines name it.
    pub fn pdf(&self, root_file: &Path) -> PathBuf {
        let stem = root_file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".into());
        self.build.join(format!("{stem}.pdf"))
    }
}

/// Everything a comparison has worked out before it writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub older: Oid,
    pub newer: Newer,
    /// The root file as a commit's tree spells it: from the repository's top, `/`-separated.
    pub tree_root_file: String,
    pub dirs: ComparisonDirs,
    /// Where the project sits inside the exported newer tree: the folder the engine runs in.
    pub project_in_export: PathBuf,
    /// Where the engine will write the marked-up PDF.
    pub pdf: PathBuf,
    /// How much markup `latexdiff` is asked for (A7).
    pub markup: Markup,
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

/// The pair as `(older commit, newer side)`. Two commits go older first, whatever order they were
/// clicked in (A4); a commit and the working copy always put the working copy last, since nothing
/// the author has not committed yet can be older than a commit.
fn order(repository: &Repository, a: &str, b: &str) -> Result<(Oid, Newer), String> {
    match (parse_newer(a)?, parse_newer(b)?) {
        (Newer::WorkingTree, Newer::WorkingTree) => {
            Err("Pick a commit to compare your working copy with.".to_string())
        }
        (Newer::WorkingTree, Newer::Commit(commit)) | (Newer::Commit(commit), Newer::WorkingTree) => {
            Ok((commit, Newer::WorkingTree))
        }
        (Newer::Commit(a), Newer::Commit(b)) => {
            let (older, newer) =
                abstract_tex_git::older_first(repository, a, b).map_err(|e| e.to_string())?;
            Ok((older, Newer::Commit(newer)))
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
    markup: Markup,
) -> Result<Plan, String> {
    let (older, newer) = order(repository, a, b)?;

    let prefix = project_prefix(repository, project_dir);
    let tree_root_file = prefix.join(root_file).to_string_lossy().replace('\\', "/");
    let root_name = root_file.to_string_lossy().replace('\\', "/");
    let sides = [
        Some(older),
        match newer {
            Newer::Commit(commit) => Some(commit),
            Newer::WorkingTree => None,
        },
    ];
    for side in sides {
        match side {
            Some(commit) => {
                if !abstract_tex_git::has_path(repository, commit, &tree_root_file)
                    .map_err(|e| e.to_string())?
                {
                    return Err(format!(
                        "Commit {} has no {root_name}: the root file was renamed, or did not exist yet, so \
                         there is nothing in it to compare.",
                        short(commit),
                    ));
                }
            }
            None => {
                if !project_dir.join(root_file).is_file() {
                    return Err(format!(
                        "Your working copy has no {root_name} right now, so there is nothing in it to compare."
                    ));
                }
            }
        }
    }

    let dirs = ComparisonDirs::new(latexdiff_dir, older, newer, markup);
    Ok(Plan {
        older,
        newer,
        tree_root_file,
        project_in_export: dirs.new.join(&prefix),
        pdf: dirs.pdf(root_file),
        dirs,
        markup,
    })
}

/// Clear `.abstract-tex/latexdiff/` of everything but this plan's pair, and say whether that
/// pair's PDF is already there to reuse.
///
/// A folder for this very pair with no PDF in it is a comparison that failed or was interrupted.
/// It is cleared too, rather than exported over: an export only ever *adds* files, so a stale one
/// from a half-finished run could otherwise end up in the next compile.
pub fn make_room(latexdiff_dir: &Path, plan: &Plan) -> std::io::Result<bool> {
    // A working copy changes between one click and the next, so its PDF is never reusable.
    let reusable = plan.pdf.is_file() && plan.newer != Newer::WorkingTree;
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
    markup: Markup,
) -> Result<(Plan, bool), String> {
    if !abstract_tex_latexdiff::is_installed(latexdiff) {
        return Err(sentence(LatexdiffError::NotInstalled));
    }
    let repository = abstract_tex_git::open(project_dir).map_err(|e| e.to_string())?;
    let plan = plan(&repository, project_dir, root_file, latexdiff_dir, a, b, markup)?;
    let reused = make_room(latexdiff_dir, &plan).map_err(|e| e.to_string())?;
    if !reused {
        render(latexdiff, &repository, &plan, project_dir)?;
    }
    Ok((plan, reused))
}

/// Export both revisions and mark up the newer one's root file (S11.4b). The first attempt is
/// `latexdiff`'s own defaults, `--flatten` the only flag (design interview A7, A8); the plan's
/// `markup` asks for the simpler setting on a retry.
pub fn render(
    latexdiff: &Path,
    repository: &Repository,
    plan: &Plan,
    project_dir: &Path,
) -> Result<(), String> {
    let dirs = abstract_tex_latexdiff::ExportDirs {
        old: &plan.dirs.old,
        new: &plan.dirs.new,
    };
    match plan.newer {
        Newer::Commit(newer) => abstract_tex_latexdiff::render_with(
            latexdiff,
            repository,
            plan.older,
            newer,
            &plan.tree_root_file,
            dirs,
            plan.markup,
        )
        .map(|_| ()),
        Newer::WorkingTree => {
            // Checked first, as `render_with` does: a machine with no `latexdiff` must not have
            // two exports written for a comparison that cannot run.
            if !abstract_tex_latexdiff::is_installed(latexdiff) {
                return Err(sentence(LatexdiffError::NotInstalled));
            }
            abstract_tex_git::export_tree(repository, plan.older, &plan.dirs.old)
                .map_err(|e| e.to_string())?;
            // Copied to where the project sits inside the export, so the root file's path is the
            // same one `plan.tree_root_file` names for the older side.
            export_working_tree(repository, project_dir, &plan.project_in_export)
                .map_err(|e| e.to_string())?;
            abstract_tex_latexdiff::render_exported(latexdiff, &plan.tree_root_file, dirs, plan.markup)
                .map(|_| ())
        }
    }
    .map_err(sentence)
}

/// Copy the project folder as it is now to `destination`, without what is not the document:
/// Git's own folder, ours, and anything the repository's ignore rules exclude (build output, mostly).
///
/// A copy and not a tree export because the working copy has files Git has never seen, and the
/// whole point of comparing against it is to see an uncommitted chapter.
pub fn export_working_tree(
    repository: &Repository,
    project_dir: &Path,
    destination: &Path,
) -> std::io::Result<()> {
    let prefix = project_prefix(repository, project_dir);
    copy_working_tree(repository, project_dir, destination, &prefix)
}

fn copy_working_tree(
    repository: &Repository,
    from: &Path,
    to: &Path,
    relative_to_repository: &Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if matches!(name_text.as_ref(), ".git" | ".abstract-tex" | ".preamble") {
            continue;
        }
        let relative = relative_to_repository.join(&name);
        // `is_path_ignored` errs for a path it cannot judge; treat that as "not ignored" so an odd
        // file is copied rather than silently missing from the comparison.
        if repository.is_path_ignored(&relative).unwrap_or(false) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_working_tree(repository, &entry.path(), &to.join(&name), &relative)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
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
    let Some(workdir) = repository.workdir() else {
        return PathBuf::new();
    };
    // Canonical on both sides, or a symlinked home folder (or Windows's `\\?\` form on one side
    // only) makes a project inside the repository look like one outside it.
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(project_dir)
        .strip_prefix(canonical(workdir))
        .map(Path::to_path_buf)
        .unwrap_or_default()
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
            repository
                .commit(Some("HEAD"), &who, &who, text, &tree, &parents)
                .unwrap()
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

        let forwards = plan(
            &repository,
            &project,
            root,
            &latexdiff_dir,
            &first.to_string(),
            &second.to_string(),
            Markup::Standard,
        )
        .unwrap();
        let backwards = plan(
            &repository,
            &project,
            root,
            &latexdiff_dir,
            &second.to_string(),
            &first.to_string(),
            Markup::Standard,
        )
        .unwrap();

        assert_eq!(forwards, backwards);
        assert_eq!((forwards.older, forwards.newer), (first, Newer::Commit(second)));
        assert_eq!(forwards.tree_root_file, "paper/main.tex");
        assert_eq!(forwards.project_in_export, forwards.dirs.new.join("paper"));
        assert_eq!(forwards.pdf, forwards.dirs.build.join("main.pdf"));
        assert!(forwards
            .dirs
            .root
            .ends_with(format!("{}-{}", short(first), short(second))));
    }

    /// A root file that one of the two commits lacks is refused by name, before anything exists.
    #[test]
    fn a_commit_without_the_root_file_is_refused_with_its_id() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");

        let refusal = plan(
            &repository,
            &project,
            Path::new("thesis.tex"),
            &latexdiff_dir,
            &first.to_string(),
            &second.to_string(),
            Markup::Standard,
        )
        .unwrap_err();

        assert!(
            refusal.contains(&short(first)) && refusal.contains("thesis.tex"),
            "{refusal}"
        );
        assert!(
            !latexdiff_dir.exists(),
            "nothing may be written for a refused comparison"
        );
    }

    /// Only the latest comparison survives: another pair's folder goes, and this pair's own folder
    /// is kept — and reported reusable — only when its PDF made it.
    #[test]
    fn making_room_keeps_only_this_pair_and_only_if_it_finished() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let the_plan = plan(
            &repository,
            &project,
            Path::new("main.tex"),
            &latexdiff_dir,
            &first.to_string(),
            &second.to_string(),
            Markup::Standard,
        )
        .unwrap();
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

    // ---- A11: the working copy as the newer side ----

    fn ask(repository: &Repository, project: &Path, a: &str, b: &str) -> Result<Plan, String> {
        plan(
            repository,
            project,
            Path::new("main.tex"),
            &project.join(".abstract-tex/latexdiff"),
            a,
            b,
            Markup::Standard,
        )
    }

    #[test]
    fn the_working_copy_is_always_the_newer_side_whichever_way_it_was_asked() {
        let (tmp, repository, first, _second) = two_commits();
        let project = tmp.path().join("paper");
        let id = first.to_string();

        let forwards = ask(&repository, &project, &id, WORKING_TREE).unwrap();
        let backwards = ask(&repository, &project, WORKING_TREE, &id).unwrap();

        assert_eq!(forwards, backwards);
        assert_eq!(forwards.older, first);
        assert_eq!(forwards.newer, Newer::WorkingTree);
        assert_eq!(forwards.newer.id(), "working-tree");
        assert_eq!(forwards.tree_root_file, "paper/main.tex");
        assert_eq!(forwards.project_in_export, forwards.dirs.new.join("paper"));
        assert!(forwards
            .dirs
            .root
            .to_string_lossy()
            .ends_with(&format!("{}-working", short(first))));
    }

    #[test]
    fn a_working_copy_against_itself_is_not_a_comparison() {
        let (tmp, repository, _first, _second) = two_commits();
        let refusal = ask(&repository, &tmp.path().join("paper"), WORKING_TREE, WORKING_TREE).unwrap_err();
        assert!(refusal.contains("commit"), "{refusal}");
    }

    #[test]
    fn a_working_copy_without_the_root_file_is_refused_by_name() {
        let (tmp, repository, first, _second) = two_commits();
        let project = tmp.path().join("paper");
        fs::remove_file(project.join("main.tex")).unwrap();

        let refusal = ask(&repository, &project, &first.to_string(), WORKING_TREE).unwrap_err();

        assert!(
            refusal.contains("working copy") && refusal.contains("main.tex"),
            "{refusal}"
        );
    }

    /// The working copy changes between one click and the next, so a PDF on disk for "this commit
    /// against the working copy" is a PDF of an older working copy, never a reusable one.
    #[test]
    fn a_working_copy_comparison_is_never_reused() {
        let (tmp, repository, first, _second) = two_commits();
        let project = tmp.path().join("paper");
        let the_plan = ask(&repository, &project, &first.to_string(), WORKING_TREE).unwrap();
        fs::create_dir_all(&the_plan.dirs.build).unwrap();
        fs::write(&the_plan.pdf, "%PDF").unwrap();

        let reusable = make_room(&project.join(".abstract-tex/latexdiff"), &the_plan).unwrap();

        assert!(!reusable);
    }

    #[test]
    fn the_working_copy_is_copied_without_git_our_folder_or_ignored_files() {
        let (tmp, repository, _first, _second) = two_commits();
        let project = tmp.path().join("paper");
        fs::create_dir_all(project.join("chapters")).unwrap();
        fs::write(project.join("chapters/a.tex"), "a new, uncommitted chapter\n").unwrap();
        fs::create_dir_all(project.join(".abstract-tex/build")).unwrap();
        fs::write(project.join(".abstract-tex/build/main.pdf"), "x").unwrap();
        fs::write(tmp.path().join(".gitignore"), "*.aux\n").unwrap();
        fs::write(project.join("main.aux"), "build junk").unwrap();
        let destination = tmp.path().join("copy");

        export_working_tree(&repository, &project, &destination).unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("chapters/a.tex")).unwrap(),
            "a new, uncommitted chapter\n"
        );
        assert!(destination.join("main.tex").is_file());
        assert!(
            !destination.join(".abstract-tex").exists(),
            "our folder is not the author's"
        );
        assert!(
            !destination.join("main.aux").exists(),
            "an ignored file is not part of the document"
        );
    }

    /// End to end through `prepare`, with a stand-in `latexdiff` that passes the newer file
    /// through: what lands in the marked-up document is the *uncommitted* text.
    #[cfg(unix)]
    #[test]
    fn prepare_marks_up_the_uncommitted_text_of_the_working_copy() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, _repository, first, _second) = two_commits();
        let project = tmp.path().join("paper");
        fs::write(project.join("main.tex"), "uncommitted words\n").unwrap();
        let script = tmp.path().join("latexdiff");
        fs::write(&script, "#!/bin/sh\n[ \"$1\" = --version ] && exit 0\nfor a in \"$@\"; do last=\"$a\"; done\ncat \"$last\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let (the_plan, reused) = prepare(
            &script,
            &project,
            Path::new("main.tex"),
            &project.join(".abstract-tex/latexdiff"),
            &first.to_string(),
            WORKING_TREE,
            Markup::Standard,
        )
        .unwrap();

        assert!(!reused);
        assert_eq!(
            fs::read_to_string(the_plan.project_in_export.join("main.tex")).unwrap(),
            "uncommitted words\n"
        );
    }

    // ---- A7: try again with simpler markup ----

    #[test]
    fn only_an_explicit_true_asks_for_simpler_markup() {
        assert_eq!(markup_from(None), Markup::Standard);
        assert_eq!(markup_from(Some(false)), Markup::Standard);
        assert_eq!(markup_from(Some(true)), Markup::Simpler);
    }

    /// The retry compares the same pair under a different setting, and must not find the failed
    /// attempt's folder (or a standard comparison's PDF) and mistake it for its own.
    #[test]
    fn a_simpler_comparison_of_the_same_pair_has_a_folder_of_its_own() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let ask = |markup| {
            plan(
                &repository,
                &project,
                Path::new("main.tex"),
                &latexdiff_dir,
                &first.to_string(),
                &second.to_string(),
                markup,
            )
            .unwrap()
        };

        let standard = ask(Markup::Standard);
        let simpler = ask(Markup::Simpler);

        assert_ne!(standard.dirs.root, simpler.dirs.root);
        assert!(simpler.dirs.root.to_string_lossy().ends_with("-simpler"));
        assert_eq!(simpler.markup, Markup::Simpler);
        assert_eq!(standard.markup, Markup::Standard);
    }

    #[test]
    fn a_finished_standard_comparison_is_not_reused_for_the_simpler_one() {
        let (tmp, repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let ask = |markup| {
            plan(
                &repository,
                &project,
                Path::new("main.tex"),
                &latexdiff_dir,
                &first.to_string(),
                &second.to_string(),
                markup,
            )
            .unwrap()
        };
        let standard = ask(Markup::Standard);
        fs::create_dir_all(&standard.dirs.build).unwrap();
        fs::write(&standard.pdf, "%PDF").unwrap();

        let simpler = ask(Markup::Simpler);
        let reusable = make_room(&latexdiff_dir, &simpler).unwrap();

        assert!(
            !reusable,
            "the standard PDF is the wrong document for a simpler comparison"
        );
        assert!(
            !standard.dirs.root.exists(),
            "only the latest comparison stays on disk"
        );
    }

    /// The setting has to survive the trip through `prepare` to the program. A stand-in
    /// `latexdiff` written as a shell script, so Unix only: it answers `--version` and otherwise
    /// prints every argument it was given.
    #[cfg(unix)]
    #[test]
    fn the_simpler_setting_reaches_latexdiff_through_prepare() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, _repository, first, second) = two_commits();
        let project = tmp.path().join("paper");
        let latexdiff_dir = project.join(".abstract-tex/latexdiff");
        let script = tmp.path().join("latexdiff");
        fs::write(
            &script,
            "#!/bin/sh\n[ \"$1\" = --version ] && exit 0\necho \"% args: $*\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let (the_plan, reused) = prepare(
            &script,
            &project,
            Path::new("main.tex"),
            &latexdiff_dir,
            &first.to_string(),
            &second.to_string(),
            Markup::Simpler,
        )
        .unwrap();

        assert!(!reused);
        let marked_up = fs::read_to_string(the_plan.project_in_export.join("main.tex")).unwrap();
        assert!(marked_up.contains("--math-markup=whole"), "{marked_up}");
        assert!(marked_up.contains("--graphics-markup=none"), "{marked_up}");
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
        let refusal = prepare(
            &missing,
            &project,
            Path::new("main.tex"),
            &latexdiff_dir,
            &first.to_string(),
            &second.to_string(),
            Markup::Standard,
        )
        .unwrap_err();

        assert!(refusal.contains("latexdiff isn't installed"), "{refusal}");
        assert!(
            last_time.is_dir(),
            "the previous comparison must survive a refusal"
        );
        assert_eq!(
            fs::read_dir(&latexdiff_dir).unwrap().count(),
            1,
            "nothing new was exported"
        );
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
            markup: Markup::Standard,
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
        assert!(
            second.is_err(),
            "the latest comparison runs, and is refused by name: {second:?}"
        );
    }

    #[test]
    fn the_comparisons_own_log_is_read_from_its_build_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let latexdiff_dir = tmp.path().join("latexdiff");
        assert_eq!(
            read_comparison_log(&latexdiff_dir, Path::new("main.tex")),
            "",
            "no comparison yet"
        );

        let build = latexdiff_dir.join("aaaaaaa-bbbbbbb/build");
        fs::create_dir_all(&build).unwrap();
        assert_eq!(
            read_comparison_log(&latexdiff_dir, Path::new("main.tex")),
            "",
            "a build with no log"
        );

        fs::write(build.join("main.log"), "! Undefined control sequence.\n").unwrap();
        fs::write(build.join("other.log"), "not this one").unwrap();
        assert_eq!(
            read_comparison_log(&latexdiff_dir, Path::new("main.tex")),
            "! Undefined control sequence.\n"
        );
        assert_eq!(
            read_comparison_log(&latexdiff_dir, Path::new("chapters/other.tex")),
            "not this one"
        );
    }

    /// Walking away from a comparison that is still exporting makes it stop before it compiles,
    /// and one already compiling is cancelled, without a newer comparison having to be asked for.
    #[tokio::test]
    async fn cancelling_stops_a_build_and_retires_a_comparison_still_preparing() {
        let lane = DiffLane::new(Some(
            Arc::new(SleepyEngine(Duration::from_secs(5))) as Arc<dyn Engine>
        ));
        let preparing = lane.take_ticket();
        let (tx, mut rx) = mpsc::unbounded_channel::<CompileEvent>();
        lane.orchestrator.request(job(), None, move |event| {
            let _ = tx.send(event);
        });

        lane.cancel();

        assert!(
            !preparing.is_latest(),
            "a comparison still exporting must find it was retired"
        );
        let outcome = tokio::time::timeout(Duration::from_millis(500), async {
            while let Some(event) = rx.recv().await {
                if matches!(event, CompileEvent::Finished { .. }) {
                    return true;
                }
            }
            false
        })
        .await;
        assert_ne!(
            outcome,
            Ok(true),
            "a cancelled build must not report a finished comparison"
        );
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
        let ready = ComparisonStarted::Ready {
            older: "a".into(),
            newer: "b".into(),
            pdf_path: "/p.pdf".into(),
        };
        let json = serde_json::to_value(&ready).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["pdfPath"], "/p.pdf");
        assert_eq!(
            serde_json::to_value(ComparisonStarted::Superseded).unwrap()["status"],
            "superseded"
        );
    }

    /// Builds by sleeping, so a test controls which build is still running when.
    struct SleepyEngine(Duration);

    #[async_trait::async_trait]
    impl Engine for SleepyEngine {
        async fn probe(&self) -> Result<EngineInfo, EngineError> {
            Ok(EngineInfo {
                name: "sleepy".into(),
                version: "0".into(),
                path: PathBuf::new(),
            })
        }

        async fn build(
            &self,
            _: &BuildJob,
            cancel: CancellationToken,
            _: Option<ProgressSink>,
        ) -> Result<BuildOutcome, EngineError> {
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
        BuildJob {
            project_dir: ".".into(),
            root_file: "main.tex".into(),
            out_dir: "build".into(),
            synctex: false,
            shell_escape: false,
        }
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
        while let Ok(Some((lane_name, event))) =
            tokio::time::timeout(Duration::from_millis(600), rx.recv()).await
        {
            if let CompileEvent::Finished { generation, .. } = event {
                finished.push((lane_name, generation));
            }
        }
        finished.sort();
        assert_eq!(finished, vec![("diff", second_diff), ("live", live_generation)]);
        assert_ne!(first_diff, second_diff);
    }
}
