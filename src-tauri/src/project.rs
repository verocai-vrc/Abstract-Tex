//! A project is a folder (DESIGN.md §5.8). Nothing more.
//!
//! This module owns: opening a folder, listing its files, finding the root `.tex`, and reading
//! and writing `abstract-tex.toml`. It must never write anything into the source tree except
//! `abstract-tex.toml` and the files the author edits; every other artifact goes under
//! `.abstract-tex/`, which must be deletable at any moment at the cost of one slow compile.
//!
//! The app was called Preamble until September 2026, and projects opened by that build carry a
//! `preamble.toml` and a `.preamble/` folder. The first is read when there is no new config and
//! moved to the new name the first time the config is saved; the second is disposable build
//! output, so it is only kept out of the file tree and the watcher, never read or deleted.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use abstract_tex_includes::IncludeGraph;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// The one configuration file a project may carry. Human-editable, diff-friendly TOML.
pub const CONFIG_FILE: &str = "abstract-tex.toml";
/// Disposable per-project state. Gitignored by the file we drop inside it.
pub const STATE_DIR: &str = ".abstract-tex";
/// Where the engine writes `.pdf`, `.log`, `.aux` and friends.
pub const BUILD_SUBDIR: &str = "build";
/// Beside `build/` under `STATE_DIR`: the one-chapter draft (S9.9).
pub const DRAFT_SUBDIR: &str = "draft";
/// The config file's name before the rename (see the module doc).
pub const LEGACY_CONFIG_FILE: &str = "preamble.toml";
/// The state folder's name before the rename. Still ignored, so an old project does not show a
/// stale build folder in its tree or recompile when something inside it changes.
pub const LEGACY_STATE_DIR: &str = ".preamble";

/// Directories we never list and never watch.
const IGNORED_DIRS: &[&str] = &[".git", STATE_DIR, LEGACY_STATE_DIR, "node_modules", ".svn", ".hg"];
/// Build junk left behind by other tools in the source tree. Hidden, not deleted: not ours.
const JUNK_EXTENSIONS: &[&str] = &[
    "aux", "log", "out", "toc", "bbl", "blg", "fls", "fdb_latexmk", "nav", "snm", "lof", "lot", "bcf", "xdv",
];

/// `abstract-tex.toml`, deserialised. Every field is optional so an empty file is valid.
///
/// `#[serde(default)]` on the struct means missing fields take their `Default` value instead of
/// failing to parse; `derive(Default)` supplies that value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectConfig {
    pub project: ProjectSection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectSection {
    /// The root `.tex` file, relative to the project folder, with forward slashes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// `"tectonic"` (the default, bundled), or `"pdflatex"`, `"xelatex"`, `"lualatex"` for a system
    /// TeX's `latexmk` (S9.4; `abstract_tex_engine::latexmk::EngineChoice` reads it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// `.bib` files the bibliography index reads in addition to whatever the document's own
    /// `\bibliography`/`\addbibresource` commands name (S8.2) — a Better BibTeX auto-export
    /// path, most often, but nothing here assumes that; it is just another `.bib` on disk.
    /// Project-relative, forward slashes. Empty (the default) means "none": most projects never
    /// touch this field.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_bib_files: Vec<String>,
}

/// One entry in the file tree the sidebar shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    /// Relative to the project folder, forward slashes, so the frontend can use it as a key.
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

/// What the frontend receives when a project opens.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub root_dir: String,
    pub root_file: Option<String>,
    pub build_dir: String,
    pub tree: Vec<TreeNode>,
    /// The `\input`/`\include`/`\subfile` graph's nodes, root first, project-relative and
    /// forward-slash (S4.1). Empty when there is no root file to walk from.
    pub document_files: Vec<String>,
    /// `false` when at least one directive in the document could not be resolved
    /// (`abstract_tex_includes::IncludeGraph::is_complete`). The frontend's `shouldCompileFor` falls
    /// back to recompiling on every `.tex` change while this is false: an include graph that
    /// might be missing a file is a worse mistake to compile around than an extra rebuild.
    pub document_files_complete: bool,
    /// Set only when a project opens, and only when its `engine` setting could not be honoured
    /// (S9.4): the sentence the frontend shows as a notice.
    pub engine_notice: Option<String>,
}

/// An open project: an absolute folder path and its configuration.
#[derive(Debug)]
pub struct Project {
    pub root_dir: PathBuf,
    pub config: ProjectConfig,
    /// The include graph from the last `info()`, reused while none of its files changed (S9.6).
    graph_cache: Option<GraphCache>,
    /// How many times `info()` actually walked the graph: what the cache is tested by.
    graph_builds: usize,
}

/// The include graph `info()` walked, and what each of its files looked like when it did. The
/// whole graph, not only its file list, since S9.9 asks it which chapter a file belongs to.
#[derive(Debug)]
struct GraphCache {
    root: PathBuf,
    /// One per `graph.nodes`, in the same order.
    stamps: Vec<FileStamp>,
    graph: IncludeGraph,
}

/// Enough to tell that a file changed without reading it: whether it exists, its size, and
/// when it was last written. A file named by the document but missing is stamped too, so the
/// moment it appears the graph is walked again.
#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    exists: bool,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

fn stamp(path: &Path) -> FileStamp {
    match fs::metadata(path) {
        Ok(meta) => FileStamp { exists: true, len: meta.len(), modified: meta.modified().ok() },
        Err(_) => FileStamp { exists: false, len: 0, modified: None },
    }
}

impl Project {
    /// Open a folder as a project. Creates `.abstract-tex/build/` and nothing else.
    pub fn open(dir: &Path) -> Result<Self> {
        // `absolute` cleans up `.` and `..` without touching the filesystem. We avoid
        // `canonicalize` on purpose: on Windows it returns `\\?\C:\...` paths, which are correct
        // but confuse every tool and every user who sees them.
        let root_dir = std::path::absolute(dir).with_context(|| format!("not a valid path: {}", dir.display()))?;
        if !root_dir.is_dir() {
            bail!("{} is not a folder", root_dir.display());
        }

        let config = load_config(&root_dir)?;

        let state_dir = root_dir.join(STATE_DIR);
        fs::create_dir_all(state_dir.join(BUILD_SUBDIR))
            .with_context(|| format!("could not create {}", state_dir.display()))?;
        // Belt and braces: even before the project is a Git repository with our .gitignore,
        // a `*` inside .abstract-tex/ keeps it out of any `git add .` the author might run.
        let keep_out = state_dir.join(".gitignore");
        if !keep_out.exists() {
            fs::write(&keep_out, "*\n")?;
        }

        Ok(Self { root_dir, config, graph_cache: None, graph_builds: 0 })
    }

    pub fn build_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR).join(BUILD_SUBDIR)
    }

    /// Where one-chapter drafts are built (S9.9, DESIGN.md §5.8): beside `build/`, which
    /// `draft::prepare` insists on, and created by it, not here.
    pub fn draft_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR).join(DRAFT_SUBDIR)
    }

    /// The root `.tex`, relative to the project. Configured value first, detection second.
    pub fn root_file(&self) -> Option<PathBuf> {
        if let Some(configured) = &self.config.project.root {
            let candidate = PathBuf::from(configured);
            if self.root_dir.join(&candidate).is_file() {
                return Some(candidate);
            }
            tracing::warn!(root = configured, "configured root file does not exist; detecting instead");
        }
        detect_root(&self.root_dir)
    }

    /// Record a root file choice in `abstract-tex.toml`.
    pub fn set_root_file(&mut self, relative: &str) -> Result<()> {
        let resolved = self.resolve(relative)?;
        if !resolved.is_file() {
            bail!("{relative} is not a file in this project");
        }
        self.config.project.root = Some(relative.replace('\\', "/"));
        self.save_config()
    }

    /// Add a `.bib` file to `extra_bib_files` (S8.2's "link a collection") and save. Idempotent:
    /// linking the same path twice is a no-op, not a duplicate entry — the author re-opening the
    /// link dialog and picking the same collection again should not grow the list.
    pub fn add_extra_bib_file(&mut self, relative: &str) -> Result<()> {
        let normalised = relative.replace('\\', "/");
        if !self.config.project.extra_bib_files.iter().any(|existing| existing == &normalised) {
            self.config.project.extra_bib_files.push(normalised);
        }
        self.save_config()
    }

    /// Remove a `.bib` file from `extra_bib_files` (S8.7's "unlink a collection") and save.
    /// Only the list entry goes: the `.bib` on disk is the author's and stays, and Better BibTeX's
    /// auto-export in Zotero is left as it is, since removing it would be a second write to Zotero
    /// that DESIGN.md §5.4 does not allow. Unlinking a path that is not listed changes nothing and
    /// does not rewrite `abstract-tex.toml`.
    pub fn remove_extra_bib_file(&mut self, relative: &str) -> Result<()> {
        let normalised = relative.replace('\\', "/");
        let before = self.config.project.extra_bib_files.len();
        self.config.project.extra_bib_files.retain(|existing| existing != &normalised);
        if self.config.project.extra_bib_files.len() == before {
            return Ok(());
        }
        self.save_config()
    }

    pub fn save_config(&self) -> Result<()> {
        let text = toml::to_string_pretty(&self.config).context("could not serialise abstract-tex.toml")?;
        let header = "# Abstract-Tex project configuration. Safe to edit by hand; safe to commit.\n\n";
        write_atomically(&self.root_dir.join(CONFIG_FILE), &format!("{header}{text}"))?;
        // Only after the new file is safely written: a project still carrying the pre-rename
        // config has now had its contents carried over, so the old file goes rather than leaving
        // two configs that could drift apart.
        let legacy = self.root_dir.join(LEGACY_CONFIG_FILE);
        if legacy.is_file() {
            fs::remove_file(&legacy).with_context(|| format!("could not remove {}", legacy.display()))?;
        }
        Ok(())
    }

    /// Turn a frontend-supplied relative path into an absolute one, refusing anything that
    /// tries to leave the project folder. The frontend is our own code, but a path that
    /// escapes the project is a bug we want to hear about loudly rather than act on.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let rel = Path::new(relative);
        if rel.is_absolute() {
            bail!("expected a path relative to the project, got {relative}");
        }
        for component in rel.components() {
            match component {
                Component::Normal(_) | Component::CurDir => {}
                _ => bail!("path {relative} is not inside the project"),
            }
        }
        Ok(self.root_dir.join(rel))
    }

    /// The inverse of `resolve`: an absolute path inside the project, as a forward-slash
    /// relative string. `None` if the path is outside the project.
    pub fn relative(&self, absolute: &Path) -> Option<String> {
        let rel = absolute.strip_prefix(&self.root_dir).ok()?;
        Some(to_forward_slashes(rel))
    }

    /// Everything the frontend shows about the project. `&mut self` because it keeps the include
    /// graph between calls (S9.6): this runs on every debounced tree refresh — after *any* file
    /// change — and walking the graph reads every document file. Now it reads them only when one
    /// of them changed, and otherwise costs one `metadata` call per file.
    pub fn info(&mut self) -> ProjectInfo {
        // Borrowed to build the graph, then moved into the response below: the borrow checker
        // allows this because the borrow (inside the `match`) ends before the `.map()` that
        // consumes `root_file` runs.
        let root_file = self.root_file();
        let (document_files, document_files_complete) = match &root_file {
            Some(root) => self.document_graph(root),
            // No root at all: there is no document to walk, and nothing to be incomplete about.
            None => (Vec::new(), true),
        };

        ProjectInfo {
            root_dir: self.root_dir.to_string_lossy().into_owned(),
            root_file: root_file.map(|p| to_forward_slashes(&p)),
            build_dir: self.build_dir().to_string_lossy().into_owned(),
            tree: list_tree(&self.root_dir),
            document_files,
            document_files_complete,
            engine_notice: None,
        }
    }

    /// The document's files and whether the graph is complete.
    fn document_graph(&mut self, root: &Path) -> (Vec<String>, bool) {
        let graph = self.graph(root);
        (graph.nodes.iter().map(|node| node.path.clone()).collect(), graph.is_complete())
    }

    /// The include graph: from the cache while every file it walked still looks the same, from a
    /// fresh walk otherwise.
    fn graph(&mut self, root: &Path) -> &IncludeGraph {
        let unchanged = self.graph_cache.as_ref().is_some_and(|cache| {
            cache.root == root
                && cache.graph.nodes.iter().zip(&cache.stamps).all(|(node, then)| stamp(&self.root_dir.join(&node.path)) == *then)
        });
        if !unchanged {
            self.graph_builds += 1;
            let graph = abstract_tex_includes::build_graph(&self.root_dir, root);
            let stamps = graph.nodes.iter().map(|node| stamp(&self.root_dir.join(&node.path))).collect();
            self.graph_cache = Some(GraphCache { root: root.to_path_buf(), stamps, graph });
        }
        &self.graph_cache.as_ref().expect("filled just above when it was empty or stale").graph
    }

    /// The `\include` argument of the chapter `relative` belongs to, for a one-chapter draft
    /// (S9.9). `None` without a root, and for the root itself, its preamble, or any file no
    /// `\include` reaches: those have no chapter to draft.
    pub fn chapter_of(&mut self, relative: &str) -> Option<String> {
        let root = self.root_file()?;
        self.graph(&root).chapter_of(relative).map(|chapter| chapter.argument.clone())
    }
}

fn load_config(root_dir: &Path) -> Result<ProjectConfig> {
    let mut path = root_dir.join(CONFIG_FILE);
    if !path.exists() {
        path = root_dir.join(LEGACY_CONFIG_FILE);
    }
    if !path.exists() {
        return Ok(ProjectConfig::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("{} is not valid: check its syntax", path.display()))
}

/// Find the root `.tex` file of a folder, or `None` if there is no candidate.
///
/// Order: `main.tex` at the top level; then any top-level `.tex` containing `\documentclass`
/// that no other file in the project includes; then the same test up to three folders deep.
/// Ties go to the shortest path, then the name.
///
/// "No other file includes it" is what stops a chapter that happens to carry its own
/// `\documentclass` (a `\subfile`-style chapter, meant to compile standalone for a quick preview)
/// from outranking the real root just because it was found first.
pub fn detect_root(dir: &Path) -> Option<PathBuf> {
    let main = dir.join("main.tex");
    if main.is_file() {
        return Some(PathBuf::from("main.tex"));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    // Every include target seen anywhere in the walk, kept as forward-slash strings rather than
    // `PathBuf`s so comparing them against `candidates` below can never fall over a separator
    // mismatch (`to_forward_slashes` exists for exactly this reason; S3.4/S3.6 both hit it).
    let mut included: HashSet<String> = HashSet::new();
    collect_documentclass_files(dir, dir, 0, &mut candidates, &mut included);
    candidates.retain(|candidate| !included.contains(&to_forward_slashes(candidate)));
    candidates.sort_by(|a, b| {
        let depth = |p: &PathBuf| p.components().count();
        depth(a).cmp(&depth(b)).then_with(|| a.cmp(b))
    });
    candidates.into_iter().next()
}

fn collect_documentclass_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    candidates: &mut Vec<PathBuf>,
    included: &mut HashSet<String>,
) {
    const MAX_DEPTH: usize = 3;
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if depth < MAX_DEPTH && !is_ignored_dir(&name) {
                collect_documentclass_files(root, &path, depth + 1, candidates, included);
            }
            continue;
        }
        if !path.extension().is_some_and(|e| e == "tex") {
            continue;
        }
        // Read once, use twice: the `\documentclass` check below and the include scan both want
        // the file's text, and a second read would be wasted work for the same answer.
        let Ok(text) = fs::read_to_string(&path) else { continue };

        // Resolved against the project root, not this file's own directory: at this point we do
        // not yet know which file the root even is (that is what this function is deciding), and
        // a document's root is overwhelmingly at the top level in practice — the `main.tex` fast
        // path above already covers the common case where it is not this file doing the
        // including. `build_graph` re-resolves properly once a root is actually chosen.
        for directive in abstract_tex_includes::scan_includes(&text) {
            if let abstract_tex_includes::Directive::Include { argument, .. } = directive {
                if let Some(target) = abstract_tex_includes::resolve_include_argument(root, Path::new(""), &argument) {
                    included.insert(target);
                }
            }
        }

        if file_declares_documentclass(&text) {
            if let Ok(rel) = path.strip_prefix(root) {
                candidates.push(rel.to_path_buf());
            }
        }
    }
}

/// Cheap check: does the file mention `\documentclass` outside a comment?
fn file_declares_documentclass(text: &str) -> bool {
    text.lines().any(|line| {
        let code = line.split('%').next().unwrap_or("");
        code.contains("\\documentclass")
    })
}

/// The file tree, folders first, case-insensitive order, ignoring what we never show.
pub fn list_tree(dir: &Path) -> Vec<TreeNode> {
    list_tree_inner(dir, dir)
}

fn list_tree_inner(root: &Path, dir: &Path) -> Vec<TreeNode> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut nodes: Vec<TreeNode> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = path.is_dir();
            if (is_dir && is_ignored_dir(&name)) || (!is_dir && is_junk_file(&name)) {
                return None;
            }
            let rel = path.strip_prefix(root).ok()?;
            Some(TreeNode {
                path: to_forward_slashes(rel),
                children: if is_dir { list_tree_inner(root, &path) } else { Vec::new() },
                name,
                is_dir,
            })
        })
        .collect();
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    nodes
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

fn is_junk_file(name: &str) -> bool {
    if name.ends_with(".synctex.gz") || name.ends_with(".run.xml") || name.ends_with(".abstract-tex-tmp") {
        return true;
    }
    Path::new(name)
        .extension()
        .is_some_and(|ext| JUNK_EXTENSIONS.contains(&ext.to_string_lossy().as_ref()))
}

/// Windows paths use `\`; the frontend and `abstract-tex.toml` always use `/`.
pub fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Write a file so that a crash at any instant leaves either the old content or the new one,
/// never a truncated mix. Write to a sibling temp file, then rename over the target; rename is
/// atomic on every filesystem we care about (DESIGN.md §9, first row).
pub fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = path.with_file_name(format!("{file_name}.abstract-tex-tmp"));
    fs::write(&temp, contents)?;
    // `rename` replaces an existing destination on Windows too (MoveFileEx with REPLACE_EXISTING).
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn scaffold(files: &[(&str, &str)]) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, content) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        dir
    }

    #[test]
    fn main_tex_wins_when_present() {
        let dir = scaffold(&[("main.tex", ""), ("paper.tex", "\\documentclass{article}")]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("main.tex")));
    }

    #[test]
    fn documentclass_is_detected_and_comments_are_ignored() {
        let dir = scaffold(&[
            ("sections/intro.tex", "\\section{Intro}"),
            ("notes.tex", "% \\documentclass{article} commented out"),
            ("paper.tex", "\\documentclass{article}"),
        ]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("paper.tex")));
    }

    #[test]
    fn shallower_candidate_beats_deeper_one() {
        let dir = scaffold(&[("deep/thesis.tex", "\\documentclass{book}"), ("z.tex", "\\documentclass{article}")]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("z.tex")));
    }

    #[test]
    fn a_file_another_file_includes_is_not_treated_as_a_root_candidate() {
        let dir = scaffold(&[
            ("outer.tex", "\\documentclass{article}\n\\input{inner}\n"),
            ("inner.tex", "\\documentclass{article}\n"),
        ]);
        // Same depth, so a name tie-break alone would pick "inner.tex" first; it must lose
        // because outer.tex includes it — a chapter that can also compile alone is still a
        // chapter, not the document (S4.1).
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("outer.tex")));
    }

    #[test]
    fn no_candidate_yields_none() {
        let dir = scaffold(&[("readme.md", "hi")]);
        assert_eq!(detect_root(dir.path()), None);
    }

    #[test]
    fn tree_hides_state_dirs_and_junk_and_sorts_folders_first() {
        let dir = scaffold(&[
            ("main.tex", ""),
            ("main.aux", ""),
            ("main.synctex.gz", ""),
            (".git/HEAD", ""),
            (".abstract-tex/build/main.pdf", ""),
            ("sections/b.tex", ""),
            ("sections/A.tex", ""),
            ("refs.bib", ""),
        ]);
        let tree = list_tree(dir.path());
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["sections", "main.tex", "refs.bib"]);
        let sections = &tree[0];
        assert_eq!(sections.path, "sections");
        let inner: Vec<&str> = sections.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(inner, vec!["A.tex", "b.tex"]);
        assert_eq!(sections.children[1].path, "sections/b.tex");
    }

    #[test]
    fn open_creates_state_dir_and_reads_config() {
        let dir = scaffold(&[("paper.tex", "\\documentclass{article}"), ("abstract-tex.toml", "[project]\nroot = \"paper.tex\"\n")]);
        let project = Project::open(dir.path()).unwrap();
        assert!(project.build_dir().is_dir());
        assert!(dir.path().join(".abstract-tex/.gitignore").is_file());
        assert_eq!(project.config.project.root.as_deref(), Some("paper.tex"));
        assert_eq!(project.root_file(), Some(PathBuf::from("paper.tex")));
    }

    #[test]
    fn missing_configured_root_falls_back_to_detection() {
        let dir = scaffold(&[("main.tex", ""), ("abstract-tex.toml", "[project]\nroot = \"gone.tex\"\n")]);
        let project = Project::open(dir.path()).unwrap();
        assert_eq!(project.root_file(), Some(PathBuf::from("main.tex")));
    }

    #[test]
    fn set_root_file_round_trips_through_toml() {
        let dir = scaffold(&[("a.tex", ""), ("b.tex", "")]);
        let mut project = Project::open(dir.path()).unwrap();
        project.set_root_file("b.tex").unwrap();
        let reloaded = Project::open(dir.path()).unwrap();
        assert_eq!(reloaded.config.project.root.as_deref(), Some("b.tex"));
        assert!(fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap().contains("root = \"b.tex\""));
    }

    #[test]
    fn document_files_matches_the_done_when_fixture_in_order() {
        let dir = scaffold(&[
            ("main.tex", "\\input{preamble}\n\\include{sections/intro}\n"),
            ("preamble.tex", ""),
            ("sections/intro.tex", "\\input{sections/fig}\n"),
            ("sections/fig.tex", ""),
            ("figures/plot.tex", "\\documentclass{standalone}\n"),
        ]);
        let mut project = Project::open(dir.path()).unwrap();
        let info = project.info();
        assert_eq!(info.root_file.as_deref(), Some("main.tex"));
        assert_eq!(
            info.document_files,
            vec!["main.tex", "preamble.tex", "sections/intro.tex", "sections/fig.tex"]
        );
        assert!(info.document_files_complete);
    }

    #[test]
    fn an_unresolved_include_marks_document_files_incomplete() {
        let dir = scaffold(&[("main.tex", "\\input{\\chapdir/x}\n")]);
        let mut project = Project::open(dir.path()).unwrap();
        let info = project.info();
        assert!(!info.document_files_complete);
    }

    #[test]
    fn no_root_file_means_an_empty_but_complete_document() {
        let dir = scaffold(&[("readme.md", "hi")]);
        let mut project = Project::open(dir.path()).unwrap();
        let info = project.info();
        assert_eq!(info.root_file, None);
        assert!(info.document_files.is_empty());
        assert!(info.document_files_complete);
    }

    #[test]
    fn resolve_refuses_to_leave_the_project() {
        let dir = scaffold(&[("main.tex", "")]);
        let project = Project::open(dir.path()).unwrap();
        assert!(project.resolve("../etc/passwd").is_err());
        assert!(project.resolve("sections/../../x").is_err());
        assert!(project.resolve("C:/Windows/x").is_err() || !cfg!(windows));
        assert!(project.resolve("sections/intro.tex").is_ok());
    }

    #[test]
    fn linking_twice_lists_once_and_unlinking_removes_it_and_survives_a_reopen() {
        let dir = scaffold(&[("main.tex", "")]);
        let mut project = Project::open(dir.path()).unwrap();
        project.add_extra_bib_file("zotero\\Thesis.bib").unwrap();
        project.add_extra_bib_file("zotero/Thesis.bib").unwrap();
        assert_eq!(project.config.project.extra_bib_files, vec!["zotero/Thesis.bib"]);

        project.remove_extra_bib_file("zotero/Thesis.bib").unwrap();
        assert!(project.config.project.extra_bib_files.is_empty());
        // Saved, not only changed in memory: a fresh open reads the same empty list back.
        assert!(Project::open(dir.path()).unwrap().config.project.extra_bib_files.is_empty());
    }

    #[test]
    fn unlinking_a_path_that_is_not_linked_does_not_touch_the_config_file() {
        let dir = scaffold(&[("main.tex", "")]);
        let mut project = Project::open(dir.path()).unwrap();
        project.remove_extra_bib_file("zotero/never-linked.bib").unwrap();
        assert!(!dir.path().join(CONFIG_FILE).exists());
    }

    #[test]
    fn a_pre_rename_config_is_read_then_moved_to_the_new_name_on_save() {
        let dir = scaffold(&[
            ("paper.tex", "\\documentclass{article}"),
            (LEGACY_CONFIG_FILE, "[project]\nroot = \"paper.tex\"\n"),
        ]);
        let mut project = Project::open(dir.path()).unwrap();
        assert_eq!(project.root_file(), Some(PathBuf::from("paper.tex")));

        project.add_extra_bib_file("zotero/Thesis.bib").unwrap();
        assert!(dir.path().join(CONFIG_FILE).is_file());
        assert!(!dir.path().join(LEGACY_CONFIG_FILE).exists());
        // Nothing was lost in the move: the root survives alongside the new setting.
        let reopened = Project::open(dir.path()).unwrap();
        assert_eq!(reopened.root_file(), Some(PathBuf::from("paper.tex")));
        assert_eq!(reopened.config.project.extra_bib_files, vec!["zotero/Thesis.bib"]);
    }

    #[test]
    fn the_pre_rename_state_folder_is_not_listed() {
        assert!(is_ignored_dir(LEGACY_STATE_DIR));
    }

    #[test]
    fn info_walks_the_include_graph_again_only_when_a_document_file_changed() {
        let dir = scaffold(&[("main.tex", "\\input{intro}\n"), ("intro.tex", "Hello."), ("notes.txt", "")]);
        let mut project = Project::open(dir.path()).unwrap();
        assert_eq!(project.info().document_files, vec!["main.tex", "intro.tex"]);
        assert_eq!(project.graph_builds, 1);

        // A refresh with nothing changed, and one after a file outside the document changed.
        project.info();
        fs::write(dir.path().join("notes.txt"), "unrelated").unwrap();
        project.info();
        assert_eq!(project.graph_builds, 1, "nothing in the document changed");

        // A chapter gains an include: its size changes, so the graph is walked again and sees it.
        fs::write(dir.path().join("intro.tex"), "Hello.\\input{fig}\n").unwrap();
        let info = project.info();
        assert_eq!(project.graph_builds, 2);
        assert_eq!(info.document_files, vec!["main.tex", "intro.tex", "fig.tex"], "named, though not on disk yet");

        // The missing file appears, with an include of its own. It was stamped as missing, so its
        // arrival is a change, and the new walk follows it one level further.
        fs::write(dir.path().join("fig.tex"), "\\input{table}\n").unwrap();
        let info = project.info();
        assert_eq!(project.graph_builds, 3);
        assert_eq!(info.document_files, vec!["main.tex", "intro.tex", "fig.tex", "table.tex"]);
    }

    #[test]
    fn chapter_of_names_the_include_a_file_is_reached_through_and_reuses_the_graph() {
        let dir = scaffold(&[
            ("main.tex", "\\input{preamble}\n\\include{chapters/one}\n"),
            ("preamble.tex", ""),
            ("chapters/one.tex", "\\input{figures/plot}\n"),
            ("figures/plot.tex", ""),
        ]);
        let mut project = Project::open(dir.path()).unwrap();
        assert_eq!(project.chapter_of("chapters/one.tex").as_deref(), Some("chapters/one"));
        assert_eq!(project.chapter_of("figures/plot.tex").as_deref(), Some("chapters/one"), "through its \\input");
        assert_eq!(project.chapter_of("preamble.tex"), None, "the preamble has no chapter to draft");
        assert_eq!(project.chapter_of("main.tex"), None);
        assert_eq!(project.graph_builds, 1, "four questions, one walk");
        assert!(project.draft_dir().ends_with(".abstract-tex/draft"));
        assert_eq!(project.draft_dir().parent(), project.build_dir().parent(), "beside build/, as prepare requires");
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_and_replaces_content() {
        let dir = scaffold(&[("main.tex", "old")]);
        let target = dir.path().join("main.tex");
        write_atomically(&target, "new").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("preamble-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
