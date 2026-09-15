//! Builds the `\input`/`\include`/`\subfile` graph of a project, breadth-first from its root
//! file, and answers whether a resolved path stays inside the project.
//!
//! This is the only module in the crate that touches disk (`scan.rs` is pure text-in/data-out).
//! It must never guess: a directive it cannot parse, a target outside the project, or a file it
//! cannot read all go into `unresolved` rather than being silently skipped — a document graph
//! that is quietly wrong is worse than one that admits what it does not know
//! (`IncludeGraph::is_complete`).
//!
//! Out of scope on purpose (S4.1's card): `\includeonly` pruning, `\graphicspath`, macro
//! expansion. A file `\includeonly` excludes from one build is still part of the document.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::scan::{scan_includes, Directive};

/// How deep the breadth-first walk may go before it stops expanding a branch. The visited set
/// already stops true cycles from looping forever; this is a second, paranoid guard against a
/// resolution bug feeding the walk something that keeps producing "new" paths. No real document
/// nests `\input` anywhere near this deep, so it is not expected to fire in practice.
const MAX_DEPTH: usize = 32;

/// One file in the include graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Project-relative, forward slashes, so it is a stable key on every platform.
    pub path: String,
    /// Every file elsewhere in the graph with a directive that resolved to this one. Usually one
    /// entry; more than one when two chapters both `\input` the same figure file.
    pub included_by: Vec<String>,
    /// Whether the file exists on disk. A missing target is the normal state of a document
    /// being written, not an error — `exists: false`, not a missing node.
    pub exists: bool,
}

/// Why one include directive could not be folded into the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// The directive itself could not be read as a plain path — see `Directive::Unparsed`.
    Unparsed { in_file: String, line: u32, raw: String },
    /// The literal argument resolves outside the project folder.
    OutsideProject { in_file: String, line: u32, argument: String },
    /// The target is inside the project and was reached, but its content could not be read
    /// (permissions, or bytes that are not valid text), so the graph cannot see what it includes.
    Unreadable { path: String },
}

/// The `\input`/`\include`/`\subfile` graph of one project, rooted at one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeGraph {
    /// The root file this graph was built from, project-relative.
    pub root: String,
    /// Discovery order, root first — the order `document_files` reports it in.
    pub nodes: Vec<Node>,
    pub unresolved: Vec<Unresolved>,
}

impl IncludeGraph {
    /// Whether this graph accounts for every include in the document. `false` means at least one
    /// directive could not be resolved, so a file this graph does not mention might still be
    /// part of the document — callers should fall back to treating every `.tex` file as in
    /// scope until the author fixes whatever `unresolved` is pointing at.
    pub fn is_complete(&self) -> bool {
        self.unresolved.is_empty()
    }
}

/// Walk the include graph of `project_dir`'s document, starting at `root_relative`.
///
/// Breadth-first, so `nodes` is root-first and shallow files come before deep ones. Every
/// argument resolves against the *root file's own directory*, not the including file's — that is
/// LaTeX's own rule (paths in `\input`/`\include` are relative to where the engine runs, which
/// for us is always the root file's directory), and it is why `sections/intro.tex`'s own
/// `\input{sections/fig}` still means `sections/fig.tex`, not `sections/sections/fig.tex`.
pub fn build_graph(project_dir: &Path, root_relative: &Path) -> IncludeGraph {
    let base_dir = root_relative.parent().map(Path::to_path_buf).unwrap_or_default();
    let root_path = to_forward_slashes(root_relative);
    let root_exists = project_dir.join(&root_path).is_file();

    let mut nodes: Vec<Node> = vec![Node { path: root_path.clone(), included_by: Vec::new(), exists: root_exists }];
    let mut unresolved: Vec<Unresolved> = Vec::new();
    // Keyed by resolved, forward-slash path, so `{intro}` and `{intro.tex}` collapse onto the
    // same node instead of becoming two, and a two-file cycle terminates instead of recursing.
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(root_path.clone());
    // `(path, depth)`: depth is carried alongside the path rather than recomputed, because a
    // node's depth is "how far the walk had gone when it was first found", not a property of
    // the path itself (the same file could sit at different depths depending on cycles).
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    if root_exists {
        queue.push_back((root_path, 0));
    }

    while let Some((current_path, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(text) = fs::read_to_string(project_dir.join(&current_path)) else {
            // Existing nodes only reach the queue when `exists` was true at creation time, so a
            // read failure here means the content could not be read as text, not that the file
            // is missing.
            unresolved.push(Unresolved::Unreadable { path: current_path });
            continue;
        };

        for directive in scan_includes(&text) {
            match directive {
                Directive::Include { argument, line } => {
                    match resolve_include_argument(project_dir, &base_dir, &argument) {
                        Some(resolved) => {
                            add_edge(&mut nodes, &mut queue, &mut visited, &current_path, &resolved, depth, project_dir);
                        }
                        None => unresolved.push(Unresolved::OutsideProject {
                            in_file: current_path.clone(),
                            line,
                            argument,
                        }),
                    }
                }
                Directive::Unparsed { raw, line } => {
                    unresolved.push(Unresolved::Unparsed { in_file: current_path.clone(), line, raw })
                }
            }
        }
    }

    IncludeGraph { root: to_forward_slashes(root_relative), nodes, unresolved }
}

/// Records that `parent` includes `resolved`: adds a new node the first time `resolved` is seen,
/// or notes `parent` as another one of an existing node's parents otherwise. Either way, a
/// newly-discovered node is queued for its own scan.
fn add_edge(
    nodes: &mut Vec<Node>,
    queue: &mut VecDeque<(String, usize)>,
    visited: &mut HashSet<String>,
    parent: &str,
    resolved: &str,
    parent_depth: usize,
    project_dir: &Path,
) {
    if visited.insert(resolved.to_string()) {
        let exists = project_dir.join(resolved).is_file();
        nodes.push(Node { path: resolved.to_string(), included_by: vec![parent.to_string()], exists });
        if exists {
            queue.push_back((resolved.to_string(), parent_depth + 1));
        }
    } else if let Some(node) = nodes.iter_mut().find(|n| n.path == resolved) {
        if !node.included_by.iter().any(|p| p == parent) {
            node.included_by.push(parent.to_string());
        }
    }
}

/// Resolves one literal `\input`/`\include`/`\subfile` argument to a project-relative,
/// forward-slash path, or `None` if it names something outside the project.
///
/// `base_dir` is the directory the argument is relative to (the root file's directory when
/// called from `build_graph`; the project root itself when `project.rs` uses this during root
/// detection, before a root has been chosen — see that call site for why). `.tex` is appended
/// when the literal path — as written, with no extension added — is not a file on disk and does
/// not already end in `.tex`; that is what makes `{sections/intro}` and `{sections/intro.tex}`
/// resolve to the same node whether or not the file exists yet.
pub fn resolve_include_argument(project_dir: &Path, base_dir: &Path, argument: &str) -> Option<String> {
    let literal = normalize_relative(&base_dir.join(argument))?;
    let already_tex = literal.extension().is_some_and(|ext| ext == "tex");

    let candidate = if already_tex || project_dir.join(&literal).is_file() {
        literal
    } else {
        append_tex_extension(&literal)
    };

    Some(to_forward_slashes(&candidate))
}

/// Appends `.tex` to a path's file name — `data.2024` becomes `data.2024.tex`.
///
/// `PathBuf::set_extension` was tried here first and is wrong: Rust considers everything after
/// the *last* dot in a file name to be "the extension" regardless of whether it looks like one,
/// so `set_extension("tex")` on `data.2024` *replaces* `2024`, producing `data.tex` and silently
/// losing the `2024` — reported by reviewer on S4.1. Building the new name as a string and
/// reattaching it with `with_file_name` appends instead of replacing.
fn append_tex_extension(path: &Path) -> PathBuf {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{file_name}.tex"))
}

/// Resolves `.` and `..` components lexically, without touching the filesystem (the target may
/// not exist yet). Returns `None` if a `..` would climb above where `path` started — i.e. the
/// argument names something outside the project — the same shape of check
/// `src-tauri/src/project.rs`'s `Project::resolve` does for paths the frontend sends.
fn normalize_relative(path: &Path) -> Option<PathBuf> {
    let mut normal_parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(normal_parts.last(), Some(Component::Normal(_))) {
                    normal_parts.pop();
                } else {
                    return None;
                }
            }
            Component::Normal(part) => normal_parts.push(Component::Normal(part)),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(normal_parts.into_iter().collect())
}

/// Windows paths use `\`; every path this crate hands back uses `/`, so a node's `path` is a
/// stable key regardless of which platform built the graph.
fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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

    /// The card's own done-when fixture: a four-file document, discovered in order, plus a
    /// fifth file (`figures/plot.tex`) that carries its own `\documentclass` but that nothing
    /// in the document actually includes.
    fn done_when_scaffold() -> TempDir {
        scaffold(&[
            ("main.tex", "\\input{preamble}\n\\include{sections/intro}\n"),
            ("preamble.tex", "% just the preamble, nothing to include\n"),
            ("sections/intro.tex", "\\input{sections/fig}\n"),
            ("sections/fig.tex", "A figure.\n"),
            ("figures/plot.tex", "\\documentclass{standalone}\n"),
        ])
    }

    #[test]
    fn walks_the_done_when_fixture_in_order() {
        let dir = done_when_scaffold();
        let graph = build_graph(dir.path(), Path::new("main.tex"));

        let paths: Vec<&str> = graph.nodes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, vec!["main.tex", "preamble.tex", "sections/intro.tex", "sections/fig.tex"]);
        assert!(graph.nodes.iter().all(|n| n.exists));
        assert!(graph.is_complete());
        // Nothing in the document includes figures/plot.tex, so it is not a node even though it
        // has its own \documentclass — that question is detect_root's, not this graph's.
        assert!(!paths.contains(&"figures/plot.tex"));
    }

    #[test]
    fn dedupes_the_bare_and_tex_suffixed_spelling_of_the_same_target() {
        let dir = scaffold(&[
            ("main.tex", "\\input{intro}\n\\include{intro.tex}\n"),
            ("intro.tex", "hello\n"),
        ]);
        let graph = build_graph(dir.path(), Path::new("main.tex"));
        let paths: Vec<&str> = graph.nodes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, vec!["main.tex", "intro.tex"]);
        assert_eq!(graph.nodes[1].included_by, vec!["main.tex"]);
    }

    #[test]
    fn a_two_file_cycle_terminates_with_each_file_listed_once() {
        let dir = scaffold(&[("a.tex", "\\input{b}\n"), ("b.tex", "\\input{a}\n")]);
        let graph = build_graph(dir.path(), Path::new("a.tex"));

        let paths: Vec<&str> = graph.nodes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, vec!["a.tex", "b.tex"]);
        // b's \input{a} closes the cycle: a is now recorded as included by b too.
        assert_eq!(graph.nodes[0].included_by, vec!["b.tex"]);
    }

    #[test]
    fn a_missing_target_is_a_node_marked_not_existing_not_an_error() {
        let dir = scaffold(&[("main.tex", "\\input{sections/intro}\n")]);
        let graph = build_graph(dir.path(), Path::new("main.tex"));

        assert!(graph.is_complete());
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[1].path, "sections/intro.tex");
        assert!(!graph.nodes[1].exists);
    }

    #[test]
    fn an_unresolvable_directive_marks_the_graph_incomplete() {
        let dir = scaffold(&[("main.tex", "\\input{\\chapdir/x}\n")]);
        let graph = build_graph(dir.path(), Path::new("main.tex"));

        assert!(!graph.is_complete());
        assert_eq!(graph.unresolved.len(), 1);
        assert!(matches!(&graph.unresolved[0], Unresolved::Unparsed { raw, .. } if raw == "\\input{\\chapdir/x}"));
    }

    #[test]
    fn a_target_outside_the_project_is_unresolved_not_a_node() {
        let dir = scaffold(&[("main.tex", "\\input{../outside}\n")]);
        let graph = build_graph(dir.path(), Path::new("main.tex"));

        assert!(!graph.is_complete());
        assert!(matches!(&graph.unresolved[0], Unresolved::OutsideProject { argument, .. } if argument == "../outside"));
        assert_eq!(graph.nodes.len(), 1); // just the root
    }

    #[test]
    fn resolve_include_argument_appends_tex_only_when_the_literal_is_missing() {
        let dir = scaffold(&[("sections/intro.tex", "")]);
        assert_eq!(
            resolve_include_argument(dir.path(), Path::new(""), "sections/intro"),
            Some("sections/intro.tex".to_string())
        );
        assert_eq!(
            resolve_include_argument(dir.path(), Path::new(""), "sections/intro.tex"),
            Some("sections/intro.tex".to_string())
        );
    }

    #[test]
    fn resolve_include_argument_appends_tex_without_swallowing_an_existing_dotted_suffix() {
        // `data.2024` already has a dot in it; appending `.tex` must not replace `2024`.
        let dir = scaffold(&[("data.2024.tex", "")]);
        assert_eq!(
            resolve_include_argument(dir.path(), Path::new(""), "data.2024"),
            Some("data.2024.tex".to_string())
        );
    }

    #[test]
    fn a_macro_bodys_input_with_a_parameter_placeholder_marks_the_graph_incomplete() {
        // Reviewer's S4.1 scenario: the scanner reads raw text, so it sees straight through a
        // macro *definition* to the `\input` in its body, with `#1` still a placeholder.
        let dir = scaffold(&[
            ("main.tex", "\\newcommand{\\loadchapter}[1]{\\input{chapters/#1}}\n\\loadchapter{intro}\n"),
            ("chapters/intro.tex", "hello\n"),
        ]);
        let graph = build_graph(dir.path(), Path::new("main.tex"));

        assert!(!graph.is_complete());
        assert!(graph.nodes.iter().all(|n| n.path != "chapters/#1.tex"));
    }

    #[test]
    fn resolve_include_argument_refuses_to_leave_the_project() {
        let dir = scaffold(&[("main.tex", "")]);
        assert_eq!(resolve_include_argument(dir.path(), Path::new(""), "../../etc/passwd"), None);
        // Climbing back down inside the project is fine.
        assert_eq!(
            resolve_include_argument(dir.path(), Path::new("sections"), "../main"),
            Some("main.tex".to_string())
        );
    }
}
