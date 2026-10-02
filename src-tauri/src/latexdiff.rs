//! Change review between two commits: planning one (S11.4c; DESIGN.md §5.7, §5.8).
//!
//! Owns the `.abstract-tex/latexdiff/` folder, which only ever holds the latest comparison, and
//! the order of the checks a comparison makes before it writes a single file.
//! `abstract_tex_latexdiff` does the marking-up; this module only decides what goes where.
//!
//! **What it must never do:**
//!
//! - Never keep two comparisons on disk, and never write outside `.abstract-tex/latexdiff/`.
//! - Never refuse a comparison after it has already touched the disk: every check comes first.

use std::path::{Path, PathBuf};

use abstract_tex_engine::BuildJob;
use abstract_tex_git::{Oid, Repository};
use abstract_tex_latexdiff::LatexdiffError;

/// A full commit id from the frontend, or a sentence saying it is not one.
pub fn parse_commit(id: &str) -> Result<Oid, String> {
    Oid::from_str(id).map_err(|_| format!("\"{id}\" is not a commit id."))
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
    use std::fs;

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
}
