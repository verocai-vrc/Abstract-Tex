//! SyncTeX forward search (S3.4) and inverse search (S3.5), wired to the open project.
//!
//! `preamble-synctex` knows nothing about a project layout — it takes a `.synctex.gz` path and
//! answers a position query. This module owns exactly the part that crate cannot: finding *which*
//! `.synctex.gz` belongs to the open project (the same `build_dir / stem` convention `commands
//! ::read_log` already uses for `.log`), and turning its typed errors into the plain sentences
//! this app shows instead of raw parser internals (DESIGN.md §2, "never show a raw log" — the
//! same rule applies to any other engine-internal detail, not only the `.log` file itself).
//!
//! It must never talk to Tauri directly; `commands.rs` is the only place that does that, the same
//! separation `compile.rs` and `lsp.rs` already keep.

use std::path::{Path, PathBuf};

use preamble_synctex::{PdfPosition, SourceLocation, SyncTex, SyncTexError};
use serde::{Deserialize, Serialize};

/// What the frontend sends for a forward search: a source position.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardQuery {
    /// Project-relative path, forward slashes — the same convention `Project::relative` produces
    /// and `Project::resolve` accepts.
    pub file: String,
    /// 1-based, matching the editor's own line numbers.
    pub line: u32,
}

/// What the frontend gets back from a forward search: where to scroll and highlight in the PDF.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardResult {
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

impl From<PdfPosition> for ForwardResult {
    fn from(position: PdfPosition) -> Self {
        Self { page: position.page, x: position.x, y: position.y }
    }
}

/// What the frontend sends for an inverse search: a click in the PDF.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InverseQuery {
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

/// What the frontend gets back from an inverse search: where to open the cursor.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverseResult {
    /// Project-relative path, forward slashes, ready for `openFile`/`jumpToLine` on the
    /// TypeScript side — `None` when the click resolved to a file outside the project (a package
    /// internal, say), which the editor has no tab for and should not try to open.
    pub file: Option<String>,
    pub line: u32,
}

/// Load the `.synctex.gz` for a build, given the directory it was built into and the root file's
/// name (the same two things `commands::compile` already gathers for a `BuildJob`).
///
/// Returns a clean sentence on every failure — "no build yet" and "TeX's own file is malformed"
/// both happen in the ordinary course of editing (before the first successful build; mid-way
/// through a failed one) and must read as status, not as an engine internals dump.
pub fn open(build_dir: &Path, root_file: &Path) -> Result<SyncTex, String> {
    let path = synctex_path(build_dir, root_file);
    SyncTex::open(&path).map_err(|error| match error {
        SyncTexError::Io { .. } => {
            "No SyncTeX data yet. Build the project first.".to_string()
        }
        SyncTexError::Gzip { .. } | SyncTexError::NotSyncTex { .. } => {
            "The SyncTeX data from the last build could not be read. Try building again.".to_string()
        }
    })
}

fn synctex_path(build_dir: &Path, root_file: &Path) -> PathBuf {
    let stem = root_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
    build_dir.join(format!("{stem}.synctex.gz"))
}

/// Turn an inverse-search hit's absolute file back into a project-relative path, the shape the
/// frontend's `DocumentManager` opens tabs by. `None` — not an error — when SyncTeX points
/// outside the project folder (a package's own `.sty`, say): there is no tab to open for that,
/// and the frontend's `InverseResult::file` being `None` says exactly that.
pub fn to_relative(project_root: &Path, hit: SourceLocation) -> InverseResult {
    let relative = hit.file.strip_prefix(project_root).ok().map(|p| p.to_string_lossy().replace('\\', "/"));
    InverseResult { file: relative, line: hit.line }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synctex_path_is_named_after_the_root_files_stem() {
        let path = synctex_path(Path::new("/proj/.preamble/build"), Path::new("main.tex"));
        assert_eq!(path, PathBuf::from("/proj/.preamble/build/main.synctex.gz"));
    }

    #[test]
    fn synctex_path_falls_back_to_main_for_a_stemless_root() {
        let path = synctex_path(Path::new("/proj/build"), Path::new(""));
        assert_eq!(path, PathBuf::from("/proj/build/main.synctex.gz"));
    }

    #[test]
    fn opening_before_any_build_reports_a_sentence_not_an_io_error() {
        let error = open(Path::new("/does/not/exist"), Path::new("main.tex")).unwrap_err();
        assert_eq!(error, "No SyncTeX data yet. Build the project first.");
    }

    #[test]
    fn a_hit_inside_the_project_becomes_a_relative_path() {
        let hit = SourceLocation { file: PathBuf::from("/proj/sections/intro.tex"), line: 12 };
        let result = to_relative(Path::new("/proj"), hit);
        assert_eq!(result.file.as_deref(), Some("sections/intro.tex"));
        assert_eq!(result.line, 12);
    }

    #[test]
    fn a_hit_outside_the_project_has_no_relative_path() {
        let hit = SourceLocation { file: PathBuf::from("/usr/share/texmf/article.cls"), line: 1 };
        let result = to_relative(Path::new("/proj"), hit);
        assert_eq!(result.file, None);
    }
}
