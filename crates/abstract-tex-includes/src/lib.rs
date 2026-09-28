//! The `\input`/`\include`/`\subfile` graph of a LaTeX project (DESIGN.md §5.3's "document map"
//! bullet; §7 v0.2, "multi-file projects: root detection, `\input`/`\include` graph").
//!
//! Two modules, so the pure half is visibly separate from the filesystem half: `scan` finds
//! directives in a string and never touches disk (exhaustively unit-tested with string literals,
//! no temp directory needed); `graph` walks disk breadth-first from a root file and resolves
//! each directive's argument to a project-relative path. The same split `abstract-tex-reconcile`
//! (diff) and `abstract-tex-synctex` (parse) already use.
//!
//! This crate must never know about Tauri, a CodeMirror buffer, or `\includeonly` — pruning a
//! *build* is not the same as knowing what the *document* is made of, and a file `\includeonly`
//! excludes from one build is still part of the document (S4.2 owns any UI for this data). It
//! does say which files are `\include`d and which chapter a file belongs to (S9.7); what a build
//! does with that is `abstract-tex-engine`'s `draft` module.

pub mod graph;
pub mod scan;

pub use graph::{build_graph, resolve_include_argument, Chapter, IncludeGraph, Node, Unresolved};
pub use scan::{scan_includes, Directive};
