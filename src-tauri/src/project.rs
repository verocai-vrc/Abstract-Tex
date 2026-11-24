//! A project is a folder (DESIGN.md §5.8). Nothing more.
//!
//! This module owns: opening a folder, listing its files, finding the root `.tex`, and reading
//! and writing `abstract-tex.toml`. It must never write anything into the source tree except
//! `abstract-tex.toml` and the files the author edits; every other artifact goes under
//! `.abstract-tex/`. Deleting that folder while the app is closed never harms the source: most of it
//! rebuilds in one slow compile. Three things in it do not come back: the saved versions
//! (`snapshots.git`) and unpushed comments (`comments.git`) of a folder that is not itself a Git
//! repository, and the copies kept before joining a live session (`join-backups/`).
//!
//! The app was called Preamble until September 2026, and projects opened by that build carry a
//! `preamble.toml` and a `.preamble/` folder. The first is read when there is no new config and
//! moved to the new name the first time the config is saved; the second is disposable build
//! output, so it is only kept out of the file tree and the watcher, never read or deleted.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use abstract_tex_includes::IncludeGraph;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// The one configuration file a project may carry. Human-editable, diff-friendly TOML.
pub const CONFIG_FILE: &str = "abstract-tex.toml";
/// Our own per-project state (what deleting it costs: the module comment). Gitignored by the file we drop inside it.
pub const STATE_DIR: &str = ".abstract-tex";
/// Where the engine writes `.pdf`, `.log`, `.aux` and friends.
pub const BUILD_SUBDIR: &str = "build";
/// Beside `build/` under `STATE_DIR`: the one-chapter draft (S9.9).
pub const DRAFT_SUBDIR: &str = "draft";
/// Beside `build/` under `STATE_DIR`: the latest change review between two commits (S11.4c).
pub const LATEXDIFF_SUBDIR: &str = "latexdiff";
/// The config file's name before the rename (see the module doc).
pub const LEGACY_CONFIG_FILE: &str = "preamble.toml";
/// The state folder's name before the rename. Still ignored, so an old project does not show a
/// stale build folder in its tree or recompile when something inside it changes.
pub const LEGACY_STATE_DIR: &str = ".preamble";

/// Directories we never list and never watch.
const IGNORED_DIRS: &[&str] = &[".git", STATE_DIR, LEGACY_STATE_DIR, "node_modules", ".svn", ".hg"];
/// Build junk left behind by other tools in the source tree. Hidden, not deleted: not ours.
const JUNK_EXTENSIONS: &[&str] = &[
    "aux",
    "log",
    "out",
    "toc",
    "bbl",
    "blg",
    "fls",
    "fdb_latexmk",
    "nav",
    "snm",
    "lof",
    "lot",
    "bcf",
    "xdv",
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
    /// What the document is called on screen (S17.3).
    pub title: String,
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
    /// Set only when a project opens, and only when its folders could not all be watched for
    /// changes made by other programs (R8.7): the sentence the frontend shows as a notice. The
    /// project opens either way.
    pub watch_notice: Option<String>,
}

/// An open project: an absolute folder path and its configuration.
#[derive(Debug)]
pub struct Project {
    pub root_dir: PathBuf,
    /// What the document is called on screen: the folder's name until someone reads a better one out of
    /// the main file (`start::record_opened`).
    pub title: String,
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
        Ok(meta) => FileStamp {
            exists: true,
            len: meta.len(),
            modified: meta.modified().ok(),
        },
        Err(_) => FileStamp {
            exists: false,
            len: 0,
            modified: None,
        },
    }
}

impl Project {
    /// Open a folder as a project. Creates `.abstract-tex/build/` and nothing else.
    pub fn open(dir: &Path) -> Result<Self> {
        // `absolute` cleans up `.` and `..` without touching the filesystem. We avoid
        // `canonicalize` on purpose: on Windows it returns `\\?\C:\...` paths, which are correct
        // but confuse every tool and every user who sees them.
        let root_dir =
            std::path::absolute(dir).with_context(|| format!("not a valid path: {}", dir.display()))?;
        if !root_dir.is_dir() {
            bail!("{} is not a folder", root_dir.display());
        }

        let config = load_config(&root_dir)?;

        let state_dir = root_dir.join(STATE_DIR);
        // `.abstract-tex` must be a folder of ours. A cloned repository can carry it as a link, and
        // everything below (the build folder, the `.gitignore`, the comparison folders cleared before
        // each comparison, the PDF pane's read access) would then happen at the link's target
        // (review finding R5.2). `symlink_metadata` looks at the name itself, not what it points at.
        if fs::symlink_metadata(&state_dir).is_ok_and(|found| found.file_type().is_symlink()) {
            bail!(
                "{} is a link, and Abstract-Tex keeps its build files there. Remove the link (or rename it) and open the project again.",
                state_dir.display()
            );
        }
        fs::create_dir_all(state_dir.join(BUILD_SUBDIR))
            .with_context(|| format!("could not create {}", state_dir.display()))?;
        // Belt and braces: even before the project is a Git repository with our .gitignore,
        // a `*` inside .abstract-tex/ keeps it out of any `git add .` the author might run.
        let keep_out = state_dir.join(".gitignore");
        // Asked about the name, not what it points to: `exists()` follows a link, and a dangling one
        // reads as "absent", so `fs::write` below would create its target, wherever that is (R8.10).
        if fs::symlink_metadata(&keep_out).is_err() {
            fs::write(&keep_out, "*\n")?;
        }

        Ok(Self {
            title: crate::recents::folder_name(&root_dir),
            root_dir,
            config,
            graph_cache: None,
            graph_builds: 0,
        })
    }

    pub fn build_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR).join(BUILD_SUBDIR)
    }

    /// Where one-chapter drafts are built (S9.9, DESIGN.md §5.8): beside `build/`, which
    /// `draft::prepare` insists on, and created by it, not here.
    pub fn draft_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR).join(DRAFT_SUBDIR)
    }

    /// Where a comparison of two commits is exported and built (S11.4c, DESIGN.md §5.8). Only
    /// ever one comparison's worth of files: `crate::latexdiff` clears the previous pair first.
    pub fn latexdiff_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR).join(LATEXDIFF_SUBDIR)
    }

    /// Everything the app writes for this project, and nothing the author wrote: the only folder
    /// the PDF pane is allowed to load from (`open_project`).
    pub fn state_dir(&self) -> PathBuf {
        self.root_dir.join(STATE_DIR)
    }

    /// The root `.tex`, relative to the project. Configured value first, detection second.
    pub fn root_file(&self) -> Option<PathBuf> {
        if let Some(configured) = &self.config.project.root {
            let candidate = PathBuf::from(configured);
            if self.root_dir.join(&candidate).is_file() {
                return Some(candidate);
            }
            tracing::warn!(
                root = configured,
                "configured root file does not exist; detecting instead"
            );
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
        if !self
            .config
            .project
            .extra_bib_files
            .iter()
            .any(|existing| existing == &normalised)
        {
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
        self.config
            .project
            .extra_bib_files
            .retain(|existing| existing != &normalised);
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

    /// [`Project::resolve`] for a path about to be *written* (a new file, a save, a restore): the
    /// same lexical rules, plus a look at the disk. The folder the file would go into must really be
    /// inside the project once links are followed, or the write is refused.
    ///
    /// Only the folder is checked, not the file name: a write is atomic (a temporary file renamed
    /// over the name), so it replaces a link that is itself the file and never writes through it,
    /// which keeps a `refs.bib` that is a link to a shared export working. A folder that is a link
    /// to somewhere else is the case that mattered: `create_dir_all` and the write beneath it landed
    /// in that other place (review finding R3.12), and a cloned repository can carry such a link.
    pub fn resolve_for_write(&self, relative: &str) -> Result<PathBuf> {
        let absolute = self.resolve(relative)?;
        let root = self
            .root_dir
            .canonicalize()
            .with_context(|| format!("could not resolve the project folder {}", self.root_dir.display()))?;

        // The nearest ancestor that exists (the folders to be created do not yet), looked at without
        // following a link at its last step, so a dangling link is seen as a link and not as "absent".
        let mut folder = absolute.parent();
        while let Some(candidate) = folder {
            if fs::symlink_metadata(candidate).is_ok() {
                let real = candidate.canonicalize().with_context(|| {
                    format!(
                        "{relative} is below a link that cannot be followed ({})",
                        candidate.display()
                    )
                })?;
                if !real.starts_with(&root) {
                    bail!(
                        "{relative} would be written outside the project, through a link at {}",
                        candidate.display()
                    );
                }
                return Ok(absolute);
            }
            folder = candidate.parent();
        }
        Ok(absolute)
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
            title: self.title.clone(),
            root_file: root_file.map(|p| to_forward_slashes(&p)),
            build_dir: self.build_dir().to_string_lossy().into_owned(),
            tree: list_tree(&self.root_dir),
            document_files,
            document_files_complete,
            engine_notice: None,
            watch_notice: None,
        }
    }

    /// The document's files and whether the graph is complete.
    fn document_graph(&mut self, root: &Path) -> (Vec<String>, bool) {
        let graph = self.graph(root);
        (
            graph.nodes.iter().map(|node| node.path.clone()).collect(),
            graph.is_complete(),
        )
    }

    /// The include graph: from the cache while every file it walked still looks the same, from a
    /// fresh walk otherwise.
    fn graph(&mut self, root: &Path) -> &IncludeGraph {
        let unchanged = self.graph_cache.as_ref().is_some_and(|cache| {
            cache.root == root
                && cache
                    .graph
                    .nodes
                    .iter()
                    .zip(&cache.stamps)
                    .all(|(node, then)| stamp(&self.root_dir.join(&node.path)) == *then)
        });
        if !unchanged {
            self.graph_builds += 1;
            let graph = abstract_tex_includes::build_graph(&self.root_dir, root);
            let stamps = graph
                .nodes
                .iter()
                .map(|node| stamp(&self.root_dir.join(&node.path)))
                .collect();
            self.graph_cache = Some(GraphCache {
                root: root.to_path_buf(),
                stamps,
                graph,
            });
        }
        &self
            .graph_cache
            .as_ref()
            .expect("filled just above when it was empty or stale")
            .graph
    }

    /// The `\include` argument of the chapter `relative` belongs to, for a one-chapter draft
    /// (S9.9). `None` without a root, and for the root itself, its preamble, or any file no
    /// `\include` reaches: those have no chapter to draft.
    pub fn chapter_of(&mut self, relative: &str) -> Option<String> {
        let root = self.root_file()?;
        self.graph(&root)
            .chapter_of(relative)
            .map(|chapter| chapter.argument.clone())
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
///
/// Two limits on that rule (E4), both found by S4.1's review. An include only counts against a
/// candidate when the including file is no deeper than the candidate: a draft in `old/` that
/// `\input`s `paper.tex` is not a reason to drop `paper.tex` in favour of the draft. And when the
/// rule would exclude every candidate (two files that `\input` each other), it is ignored rather
/// than leaving the project with no root, which would mean nothing compiles at all.
pub fn detect_root(dir: &Path) -> Option<PathBuf> {
    let main = dir.join("main.tex");
    if main.is_file() {
        return Some(PathBuf::from("main.tex"));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    // Every include target seen anywhere in the walk, with the depth of the shallowest file that
    // includes it. Keyed by forward-slash strings rather than `PathBuf`s so comparing against
    // `candidates` below can never fall over a separator mismatch (`to_forward_slashes` exists
    // for exactly this reason; S3.4/S3.6 both hit it).
    let mut included: HashMap<String, usize> = HashMap::new();
    collect_documentclass_files(dir, dir, 0, &mut candidates, &mut included);

    let depth = |path: &PathBuf| path.components().count();
    let not_included_from_above: Vec<PathBuf> = candidates
        .iter()
        .filter(|candidate| {
            let shallowest_includer = included.get(&to_forward_slashes(candidate));
            !shallowest_includer.is_some_and(|includer_depth| *includer_depth <= depth(candidate))
        })
        .cloned()
        .collect();
    if !not_included_from_above.is_empty() {
        candidates = not_included_from_above;
    }
    candidates.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
    candidates.into_iter().next()
}

fn collect_documentclass_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    candidates: &mut Vec<PathBuf>,
    included: &mut HashMap<String, usize>,
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
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };

        // Resolved against the project root, not this file's own directory: at this point we do
        // not yet know which file the root even is (that is what this function is deciding), and
        // a document's root is overwhelmingly at the top level in practice — the `main.tex` fast
        // path above already covers the common case where it is not this file doing the
        // including. `build_graph` re-resolves properly once a root is actually chosen.
        // How deep this file sits, so a target can remember its shallowest includer.
        let includer_depth = path
            .strip_prefix(root)
            .map_or(0, |relative| relative.components().count());
        for directive in abstract_tex_includes::scan_includes(&text) {
            if let abstract_tex_includes::Directive::Include { argument, .. } = directive {
                if let Some(target) =
                    abstract_tex_includes::resolve_include_argument(root, Path::new(""), &argument)
                {
                    included
                        .entry(target)
                        .and_modify(|shallowest| *shallowest = (*shallowest).min(includer_depth))
                        .or_insert(includer_depth);
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
    list_tree_inner(dir, dir, &mut Vec::new())
}

/// `ancestors` holds the real (link-free) path of every folder above `dir` that is being listed, so a
/// folder link that leads back to one of them is listed but not walked (review finding R5.23): a
/// link back up the tree gives a walk with no end, and a repository can carry one. A link to a
/// folder that is *not* an ancestor — a `chapters` link to a shared folder — is walked as before.
fn list_tree_inner(root: &Path, dir: &Path, ancestors: &mut Vec<PathBuf>) -> Vec<TreeNode> {
    // A folder that cannot be resolved is walked under its own spelling: that can only lose the
    // protection for that one folder, never a file.
    let real = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if ancestors.contains(&real) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    ancestors.push(real);
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
                children: if is_dir {
                    list_tree_inner(root, &path, ancestors)
                } else {
                    Vec::new()
                },
                name,
                is_dir,
            })
        })
        .collect();
    ancestors.pop();
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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

/// The largest file the editor will open. LaTeX sources are kilobytes; a 100 MB data file or a PDF
/// clicked in the tree would otherwise be read whole, on the main thread, and sent through IPC.
pub const MAX_TEXT_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Read a project file as text for the editor, or say why it is not one: not a regular file (a
/// device, a pipe, a folder), too large, binary, or not UTF-8.
///
/// **A file that is not valid UTF-8 is refused, not decoded loosely** (review finding R7.8). It used
/// to open through `String::from_utf8_lossy`, which turns every byte it cannot read into U+FFFD. The
/// editor is editable, so the buffer then held the replacement marks and the first autosave wrote
/// them over the author's accents: a Latin-1 file lost every `é` and `ç` on the first keystroke. The
/// same reasoning as the NUL check below: opening it would show something that saving writes back
/// over the original. Nothing is written here; the sentence says where the first bad byte is.
pub fn read_text(path: &Path) -> Result<String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    // `metadata` follows a link, so this is about what the name points at. Asked before anything is
    // read: reading a device such as `/dev/zero` never ends, and a pipe never answers (review
    // finding R5.3).
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        bail!("{name} is not a regular file, so it cannot be opened in the editor.");
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        bail!(
            "{name} is {} MB, too large to open in the editor (the limit is {} MB).",
            metadata.len() / (1024 * 1024),
            MAX_TEXT_FILE_BYTES / (1024 * 1024)
        );
    }
    let bytes = fs::read(path)?;
    // LaTeX source has no NUL byte. A file that does is an image, a PDF, or UTF-16 text, and opening
    // it would show garbage that saving then writes back over the original.
    if bytes.contains(&0) {
        bail!(
            "{name} does not look like a plain text file (an image, a PDF or a UTF-16 file, for example), \
             so it is not opened in the editor."
        );
    }
    // `String::from_utf8` hands the bytes back on failure, which is how the position is found
    // without reading the file twice.
    String::from_utf8(bytes).map_err(|error| {
        let valid_up_to = error.utf8_error().valid_up_to();
        let line = error.as_bytes()[..valid_up_to]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1;
        anyhow::anyhow!(
            "{name} is not saved as UTF-8 (line {line} holds a byte that is not valid UTF-8, as in a Latin-1 \
             or Windows-1252 file), so Abstract-Tex does not edit it: writing it back would replace every \
             letter it cannot read with a replacement mark. Convert it to UTF-8 first, for example with \
             `iconv -f latin1 -t utf-8`."
        )
    })
}

/// A project file's text, to *look at* and never to write back: scanning a chapter for citations,
/// parsing a bibliography (R7.9). Decoded loosely, so a file saved as Latin-1 is still read (its
/// ASCII commands and keys are intact; only its accented letters become U+FFFD, which no scan
/// cares about), and bounded the way [`read_text`] is, so a huge or special file is skipped rather
/// than read whole. `None` for anything that is not a readable regular file.
///
/// This is deliberately the opposite of [`read_text`]: the editor must not open what saving would
/// damage, while an index that skipped such a file would tell the author their bibliography is
/// missing. A caller that might write the text back must use `read_text`.
pub fn read_for_scanning(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_TEXT_FILE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Windows paths use `\`; the frontend and `abstract-tex.toml` always use `/`.
pub fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Write a file so that a crash at any instant leaves either the old content or the new one,
/// never a truncated mix. Write to a sibling temp file, then rename over the target; rename is
/// atomic on every filesystem we care about (DESIGN.md §9, first row).
pub fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!("{file_name}.abstract-tex-tmp"));
    // `fs::write` opens with "create or truncate", and that follows a link: a cloned repository can
    // carry this very name as a link to any file the author may write, and a save would put the
    // document there (review finding R6.1). So whatever stands at the name is removed first
    // (`remove_file` removes a link itself, never what it points at; a leftover half-written file
    // from a crash goes the same way), and the new file is made with `create_new`, which refuses
    // to open anything that exists, a dangling link included.
    match fs::remove_file(&temp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    // `Write` is what puts `write_all` on `File`; the `use` is local so the one place that needs it says so.
    use std::io::Write;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(&temp); // a half-written temporary file helps nobody
        return Err(error);
    }
    drop(file); // closed before the rename, which Windows would refuse on an open file
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
        let dir = scaffold(&[
            ("deep/thesis.tex", "\\documentclass{book}"),
            ("z.tex", "\\documentclass{article}"),
        ]);
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

    // E4. Two files that each `\input` the other exclude each other, and detection used to return
    // nothing at all, so nothing compiled.
    #[test]
    fn two_files_that_include_each_other_still_yield_a_root() {
        let dir = scaffold(&[
            ("a.tex", "\\documentclass{article}\n\\input{b}\n"),
            ("b.tex", "\\documentclass{article}\n\\input{a}\n"),
        ]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("a.tex")));
    }

    // E4. A draft in a subfolder that `\input`s the real root is not a reason to drop the root.
    #[test]
    fn a_deeper_file_including_a_shallower_one_does_not_exclude_it() {
        let dir = scaffold(&[
            ("paper.tex", "\\documentclass{article}\n"),
            ("old/submission.tex", "\\documentclass{article}\n\\input{paper}\n"),
        ]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("paper.tex")));
    }

    #[test]
    fn a_chapter_in_a_subfolder_included_from_above_is_still_not_a_root() {
        let dir = scaffold(&[
            ("thesis.tex", "\\documentclass{book}\n\\input{chapters/one}\n"),
            ("chapters/one.tex", "\\documentclass{standalone}\n"),
        ]);
        assert_eq!(detect_root(dir.path()), Some(PathBuf::from("thesis.tex")));
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

    // Review finding R5.23. `list_tree` followed every folder link and recursed into it, and the
    // kernel only stops a path after 40 links: one link back to its own folder gave an 82-node tree,
    // two gave a tree of about 2^40 nodes, and opening such a project (a cloned repository can carry
    // the links) hung the window for good, because opening builds the tree on the main thread.
    #[cfg(unix)]
    #[test]
    fn a_folder_link_that_leads_back_up_the_tree_is_listed_once_and_not_followed() {
        let dir = scaffold(&[("main.tex", ""), ("sections/a.tex", "")]);
        std::os::unix::fs::symlink(".", dir.path().join("loop")).unwrap();
        std::os::unix::fs::symlink("..", dir.path().join("sections/up")).unwrap();

        let tree = list_tree(dir.path());

        let looped = tree
            .iter()
            .find(|node| node.name == "loop")
            .expect("the link is still listed");
        assert!(
            looped.children.is_empty(),
            "a link back to an ancestor must not be walked"
        );
        let sections = tree.iter().find(|node| node.name == "sections").unwrap();
        let up = sections
            .children
            .iter()
            .find(|node| node.name == "up")
            .expect("listed too");
        assert!(up.children.is_empty());
        let names: Vec<&str> = sections.children.iter().map(|node| node.name.as_str()).collect();
        assert!(names.contains(&"a.tex"));
    }

    #[cfg(unix)]
    #[test]
    fn several_links_back_to_the_same_folder_still_finish() {
        let dir = scaffold(&[("main.tex", "")]);
        for index in 0..3 {
            std::os::unix::fs::symlink(".", dir.path().join(format!("loop{index}"))).unwrap();
        }
        let path = dir.path().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        // A thread and a deadline, so that if the walk ever blows up again this fails in seconds
        // instead of hanging the test run.
        std::thread::spawn(move || {
            let _ = sender.send(list_tree(&path).len());
        });
        let top_level = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("list_tree did not finish: it is walking folder links in circles");
        assert_eq!(top_level, 4, "main.tex and the three links");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_folder_elsewhere_in_the_project_is_still_shown_with_its_files() {
        // The legitimate use: a chapters folder that is a link to a shared one. It is not an
        // ancestor, so it is walked as before.
        let dir = scaffold(&[("main.tex", ""), ("shared/chapter.tex", "")]);
        std::os::unix::fs::symlink("shared", dir.path().join("chapters")).unwrap();
        let tree = list_tree(dir.path());
        let chapters = tree.iter().find(|node| node.name == "chapters").unwrap();
        assert_eq!(chapters.children.len(), 1);
        assert_eq!(chapters.children[0].name, "chapter.tex");
    }

    // Review finding R5.2. `open` creates `.abstract-tex/build` and a `.gitignore` inside it, and the
    // app later clears folders under it and lets the PDF pane read from it; a cloned repository can
    // carry `.abstract-tex` as a link to anywhere, and every one of those then happens there.
    #[cfg(unix)]
    #[test]
    fn a_project_whose_state_folder_is_a_link_is_refused_and_nothing_is_written_through_it() {
        let dir = scaffold(&[("main.tex", "")]);
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(STATE_DIR)).unwrap();

        let Err(error) = Project::open(dir.path()) else {
            panic!("a linked state folder is refused");
        };

        assert!(error.to_string().contains(".abstract-tex"), "{error}");
        assert_eq!(
            fs::read_dir(elsewhere.path()).unwrap().count(),
            0,
            "nothing may be written into the link's target"
        );
    }

    // Review finding R6.1. A save writes to `<name>.abstract-tex-tmp` and renames it over `<name>`;
    // a cloned repository can carry that temporary name as a link, and `fs::write` followed it, so a
    // save wrote the document into the link's target (and the rename then made `<name>` the link).
    #[cfg(unix)]
    #[test]
    fn a_save_never_writes_through_a_link_standing_at_the_temporary_name() {
        let dir = scaffold(&[("main.tex", "the old text")]);
        let elsewhere = tempfile::tempdir().unwrap();
        let victim = elsewhere.path().join("victim.txt");
        fs::write(&victim, "not the author's to change").unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join("main.tex.abstract-tex-tmp")).unwrap();

        write_atomically(&dir.path().join("main.tex"), "the new text").unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "not the author's to change");
        assert_eq!(
            fs::read_to_string(dir.path().join("main.tex")).unwrap(),
            "the new text"
        );
        assert!(
            !fs::symlink_metadata(dir.path().join("main.tex"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "main.tex must be the file that was written, not the link"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_save_through_a_dangling_link_at_the_temporary_name_creates_nothing_elsewhere() {
        let dir = scaffold(&[("main.tex", "the old text")]);
        let elsewhere = tempfile::tempdir().unwrap();
        let would_be_created = elsewhere.path().join("created-by-the-save");
        std::os::unix::fs::symlink(&would_be_created, dir.path().join("main.tex.abstract-tex-tmp")).unwrap();

        write_atomically(&dir.path().join("main.tex"), "the new text").unwrap();

        assert!(!would_be_created.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("main.tex")).unwrap(),
            "the new text"
        );
    }

    #[test]
    fn a_leftover_temporary_file_from_a_crash_does_not_stop_the_next_save() {
        let dir = scaffold(&[
            ("main.tex", "the old text"),
            ("main.tex.abstract-tex-tmp", "half of a write"),
        ]);
        write_atomically(&dir.path().join("main.tex"), "the new text").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("main.tex")).unwrap(),
            "the new text"
        );
        assert!(!dir.path().join("main.tex.abstract-tex-tmp").exists());
    }

    // Review finding R5.3. `read_file` read whatever the name pointed at, whole, on the main thread.
    #[cfg(unix)]
    #[test]
    fn read_text_refuses_what_is_not_a_regular_file() {
        let dir = scaffold(&[("main.tex", "hello")]);
        std::os::unix::fs::symlink("/dev/zero", dir.path().join("zero.tex")).unwrap();
        let error = read_text(&dir.path().join("zero.tex")).unwrap_err();
        assert!(error.to_string().contains("not a regular file"), "{error}");
        let folder_error = read_text(dir.path()).unwrap_err();
        assert!(
            folder_error.to_string().contains("not a regular file"),
            "{folder_error}"
        );
    }

    #[test]
    fn read_text_refuses_a_file_too_large_to_open_without_reading_it() {
        let dir = scaffold(&[]);
        let big = dir.path().join("data.csv");
        // A sparse file: it claims the length without the disk or the memory for it.
        fs::File::create(&big)
            .unwrap()
            .set_len(MAX_TEXT_FILE_BYTES + 1)
            .unwrap();
        let error = read_text(&big).unwrap_err();
        assert!(error.to_string().contains("too large"), "{error}");
    }

    #[test]
    fn read_text_refuses_a_binary_file() {
        let dir = scaffold(&[]);
        fs::write(dir.path().join("figure.png"), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
        let error = read_text(&dir.path().join("figure.png")).unwrap_err();
        assert!(error.to_string().contains("text file"), "{error}");
        // R8 note: the sentence used to say "(UTF-8 or Latin-1)", offering the author an encoding the
        // editor refuses (see the test below). It names what a NUL byte really means instead.
        assert!(!error.to_string().contains("Latin-1"), "{error}");
        assert!(error.to_string().contains("UTF-16"), "{error}");

        fs::write(dir.path().join("main.tex"), "plain").unwrap();
        assert_eq!(read_text(&dir.path().join("main.tex")).unwrap(), "plain");
    }

    // R7.8. A Latin-1 file used to open with each accented letter replaced by U+FFFD, and the first
    // autosave then wrote the replacements over the original bytes. Refusing to open it is the only
    // answer that cannot lose the author's text; the sentence says which line and what to do.
    #[test]
    fn read_text_refuses_a_file_that_is_not_utf_8_instead_of_opening_it_with_its_letters_replaced() {
        let dir = scaffold(&[]);
        let path = dir.path().join("old.tex");
        let bytes: &[u8] = b"first line is fine\ncaf\xe9 au lait\n";
        fs::write(&path, bytes).unwrap();

        let error = read_text(&path).unwrap_err().to_string();

        assert!(error.contains("old.tex"), "{error}");
        // The controller recognises this sentence by this phrase (`reconcileOpenDocument`).
        assert!(error.contains("not saved as UTF-8"), "{error}");
        assert!(
            error.contains("line 2"),
            "the sentence says where the first bad byte is: {error}"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes, "reading must not touch the file");
    }

    #[test]
    fn read_text_still_opens_utf_8_with_accents_and_a_byte_order_mark() {
        let dir = scaffold(&[]);
        fs::write(
            dir.path().join("accents.tex"),
            "caf\u{e9} \u{4e2d}\u{6587} \u{1F600}",
        )
        .unwrap();
        assert_eq!(
            read_text(&dir.path().join("accents.tex")).unwrap(),
            "caf\u{e9} \u{4e2d}\u{6587} \u{1F600}"
        );
        fs::write(dir.path().join("bom.tex"), b"\xef\xbb\xbfbody").unwrap();
        assert_eq!(read_text(&dir.path().join("bom.tex")).unwrap(), "\u{feff}body");
    }

    #[test]
    fn open_creates_state_dir_and_reads_config() {
        let dir = scaffold(&[
            ("paper.tex", "\\documentclass{article}"),
            ("abstract-tex.toml", "[project]\nroot = \"paper.tex\"\n"),
        ]);
        let project = Project::open(dir.path()).unwrap();
        assert!(project.build_dir().is_dir());
        assert!(dir.path().join(".abstract-tex/.gitignore").is_file());
        assert_eq!(project.config.project.root.as_deref(), Some("paper.tex"));
        assert_eq!(project.root_file(), Some(PathBuf::from("paper.tex")));
    }

    #[test]
    fn missing_configured_root_falls_back_to_detection() {
        let dir = scaffold(&[
            ("main.tex", ""),
            ("abstract-tex.toml", "[project]\nroot = \"gone.tex\"\n"),
        ]);
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
        assert!(fs::read_to_string(dir.path().join(CONFIG_FILE))
            .unwrap()
            .contains("root = \"b.tex\""));
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
            vec![
                "main.tex",
                "preamble.tex",
                "sections/intro.tex",
                "sections/fig.tex"
            ]
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

    // Review finding R3.12. `resolve` is lexical: it refuses `..` and absolute paths but never looks at
    // the disk, so a symlinked folder inside a project (a cloned repository can carry one) led out of
    // it, and `create_file`'s `create_dir_all` and a write beneath it landed in somebody else's
    // folder. Writes now also check where the folder they would write into really is. Reads still
    // follow links: a shared `refs.bib` or `figures/` that is a link to one folder up is ordinary.
    #[cfg(unix)]
    #[test]
    fn a_write_beneath_a_link_that_leaves_the_project_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        let dir = scaffold(&[("main.tex", "")]);
        std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
        let project = Project::open(dir.path()).unwrap();

        assert!(project.resolve_for_write("linked/new.tex").is_err());
        assert!(project.resolve_for_write("linked/deeper/new.tex").is_err());
        // Reading through it is still allowed.
        assert!(project.resolve("linked/new.tex").is_ok());
        // An ordinary write, a new folder inside the project, and a link that is the file itself.
        assert!(project.resolve_for_write("main.tex").is_ok());
        assert!(project.resolve_for_write("chapters/one.tex").is_ok());
        std::os::unix::fs::symlink(outside.path().join("shared.bib"), dir.path().join("refs.bib")).unwrap();
        assert!(
            project.resolve_for_write("refs.bib").is_ok(),
            "a write replaces the link itself (it is atomic), so a linked file is fine"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_folder_inside_the_project_is_fine_to_write_beneath() {
        let dir = scaffold(&[("main.tex", ""), ("real/a.tex", "")]);
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        let project = Project::open(dir.path()).unwrap();
        assert!(project.resolve_for_write("alias/b.tex").is_ok());
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
        assert!(Project::open(dir.path())
            .unwrap()
            .config
            .project
            .extra_bib_files
            .is_empty());
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
        let dir = scaffold(&[
            ("main.tex", "\\input{intro}\n"),
            ("intro.tex", "Hello."),
            ("notes.txt", ""),
        ]);
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
        assert_eq!(
            info.document_files,
            vec!["main.tex", "intro.tex", "fig.tex"],
            "named, though not on disk yet"
        );

        // The missing file appears, with an include of its own. It was stamped as missing, so its
        // arrival is a change, and the new walk follows it one level further.
        fs::write(dir.path().join("fig.tex"), "\\input{table}\n").unwrap();
        let info = project.info();
        assert_eq!(project.graph_builds, 3);
        assert_eq!(
            info.document_files,
            vec!["main.tex", "intro.tex", "fig.tex", "table.tex"]
        );
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
        assert_eq!(
            project.chapter_of("chapters/one.tex").as_deref(),
            Some("chapters/one")
        );
        assert_eq!(
            project.chapter_of("figures/plot.tex").as_deref(),
            Some("chapters/one"),
            "through its \\input"
        );
        assert_eq!(
            project.chapter_of("preamble.tex"),
            None,
            "the preamble has no chapter to draft"
        );
        assert_eq!(project.chapter_of("main.tex"), None);
        assert_eq!(project.graph_builds, 1, "four questions, one walk");
        assert!(project.draft_dir().ends_with(".abstract-tex/draft"));
        assert_eq!(
            project.draft_dir().parent(),
            project.build_dir().parent(),
            "beside build/, as prepare requires"
        );
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

    /// R8.10: `.abstract-tex/.gitignore` was written `if !exists()`, and `exists()` follows a link, so a
    /// dangling link made `fs::write` create the target (anywhere the author can write) holding `*`.
    #[cfg(unix)]
    #[test]
    fn a_dangling_link_where_the_gitignore_goes_creates_nothing_behind_it() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let would_be_created = elsewhere.path().join("created-by-the-app");
        fs::create_dir_all(dir.path().join(STATE_DIR)).unwrap();
        std::os::unix::fs::symlink(&would_be_created, dir.path().join(STATE_DIR).join(".gitignore")).unwrap();

        Project::open(dir.path()).unwrap();

        assert!(
            !would_be_created.exists(),
            "a file was created through a dangling link"
        );
    }
}
