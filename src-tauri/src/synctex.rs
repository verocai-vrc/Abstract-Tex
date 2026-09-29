//! SyncTeX forward search (S3.4) and inverse search (S3.5), wired to the open project.
//!
//! `abstract-tex-synctex` knows nothing about a project layout — it takes a `.synctex.gz` path and
//! answers a position query. This module owns exactly the part that crate cannot: finding *which*
//! `.synctex.gz` belongs to the open project (the same `build_dir / stem` convention `commands
//! ::read_log` already uses for `.log`), and turning its typed errors into the plain sentences
//! this app shows instead of raw parser internals (DESIGN.md §2, "never show a raw log" — the
//! same rule applies to any other engine-internal detail, not only the `.log` file itself).
//!
//! It must never talk to Tauri directly; `commands.rs` is the only place that does that, the same
//! separation `compile.rs` and `lsp.rs` already keep.

use std::path::{Path, PathBuf};

use abstract_tex_synctex::{PdfPosition, SourceLocation, SyncTex, SyncTexError};
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
    /// Search the draft's `.synctex.gz`, because the PDF on screen is the draft (S9.9).
    /// `#[serde(default)]`: a query without the field means the full build, as before S9.9.
    #[serde(default)]
    pub draft: bool,
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
    /// As [`ForwardQuery::draft`].
    #[serde(default)]
    pub draft: bool,
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
        let path = synctex_path(Path::new("/proj/.abstract-tex/build"), Path::new("main.tex"));
        assert_eq!(path, PathBuf::from("/proj/.abstract-tex/build/main.synctex.gz"));
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

    /// The whole S3.5 chain against `abstract-tex-synctex`'s own real fixture: `open` finds the
    /// `.synctex.gz` this module resolves the path for, `inverse_search` answers from it, and
    /// `to_relative` turns the hit into the shape `synctex_inverse`'s caller gets back — the same
    /// three calls the Tauri command makes, minus the `State` plumbing `commands.rs` adds.
    #[test]
    fn inverse_search_end_to_end_against_the_real_fixture() {
        let project = tempfile::tempdir().unwrap();
        relocate_fixture_into(project.path());
        // The temp folder is both the project root and the build dir: the relocated `.gz` sits
        // there named after its `.tex`, the `<stem>.synctex.gz` convention `synctex_path` uses.
        let table = open(project.path(), Path::new("multi.tex")).expect("the relocated fixture should open");

        // Forward search first, to get a real point on page 1 to click "near" — the same
        // approach the crate's own real-fixture test uses, rather than guessing coordinates.
        let source = project.path().join("multi.tex");
        let forward = table.forward_search(&source, 3).expect("line 3 is on page 1");

        let position = abstract_tex_synctex::PdfPosition { page: forward.page, x: forward.x, y: forward.y };
        let hit = table.inverse_search(position).expect("a record exists at this exact point");
        let result = to_relative(project.path(), hit);

        assert_eq!(result.file.as_deref(), Some("multi.tex"));
        assert_eq!(result.line, 3);
    }

    /// S9.9's "a click in the draft lands on the right line", against the real engine and the
    /// corpus thesis: the project names the chapter, the draft is built as the orchestrator
    /// builds it, and both searches run on the draft's `.synctex.gz` the way `synctex_forward`
    /// and `synctex_inverse` do when the frontend says a draft is showing.
    /// `cargo test -p abstract-tex -- --ignored a_click_in_a_real_thesis_draft`
    #[tokio::test]
    #[ignore]
    async fn a_click_in_a_real_thesis_draft_lands_on_the_chapter_line() {
        use abstract_tex_engine::draft::{self, DraftJob};
        use abstract_tex_engine::tectonic::Tectonic;
        use abstract_tex_engine::{BuildJob, Engine};
        use tokio_util::sync::CancellationToken;

        let binaries = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let tectonic = std::fs::read_dir(&binaries)
            .expect("run `pnpm fetch-engine`")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("tectonic-")))
            .expect("run `pnpm fetch-engine`");
        let engine = Tectonic::at(tectonic);

        let folder = tempfile::tempdir().unwrap();
        copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/corpus/thesis"), folder.path());
        let mut project = crate::project::Project::open(folder.path()).unwrap();
        let root_file = project.root_file().unwrap();
        let job = BuildJob { project_dir: project.root_dir.clone(), root_file: root_file.clone(), out_dir: project.build_dir(), synctex: true, shell_escape: false };
        assert!(engine.build(&job, CancellationToken::new(), None).await.unwrap().success);

        let chapter = project.chapter_of("chapters/03-method.tex").expect("chapter 3 is an \\include");
        let draft_job = DraftJob { chapter, dir: project.draft_dir() };
        let layout = draft::prepare(&job, &draft_job).unwrap().expect("a warm thesis has a draft");
        let drafted = engine.build_draft(&job, &layout, CancellationToken::new()).await.unwrap().unwrap();
        assert!(drafted.success, "{}", drafted.stderr);

        let table = open(&draft::build_folder(&project.draft_dir()), &root_file).expect("the draft wrote SyncTeX");
        // Chapter 3, line 7: the first line of prose after the generated text.
        let chapter_file = project.root_dir.join("chapters/03-method.tex");
        let spot = table.forward_search(&chapter_file, 7).expect("line 7 is typeset in the draft");
        let hit = to_relative(&project.root_dir, table.inverse_search(spot).expect("and can be clicked"));
        assert_eq!(hit.file.as_deref(), Some("chapters/03-method.tex"));
        assert!((7..=9).contains(&hit.line), "the click landed on line {}, not the paragraph at 7–9", hit.line);

        // The root is recorded through `..` from the draft's folder; its front matter still maps.
        // Line 13, `\\tableofcontents`: `\\maketitle` on line 12 leaves no record in any build.
        let contents = table.forward_search(&project.root_dir.join("main.tex"), 13).expect("the contents are in the draft");
        let back = to_relative(&project.root_dir, table.inverse_search(contents).unwrap());
        assert_eq!(back.file.as_deref(), Some("main.tex"), "not .abstract-tex/draft/../../main.tex");
    }

    fn copy_dir(from: &Path, to: &Path) {
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// Copy `abstract-tex-synctex`'s committed fixture into `folder`, rewriting its `Input:1:` line to
    /// `folder/multi.tex`. SyncTeX records the absolute path Tectonic saw when it built the file —
    /// the original author's Windows folder — so the unmodified fixture only matched on that one
    /// machine, and `to_relative`'s `strip_prefix` cannot split a Windows path on Linux or macOS
    /// at all. Relocating it makes the chain below exercise this checkout's real paths, on every
    /// OS CI runs.
    fn relocate_fixture_into(folder: &Path) {
        // `flate2` is gzip: SyncTeX's `.gz` is a text file compressed, nothing more.
        use flate2::{read::GzDecoder, write::GzEncoder, Compression};
        use std::io::{Read, Write};

        let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/abstract-tex-synctex/fixtures/multi.synctex.gz");
        let mut text = String::new();
        GzDecoder::new(std::fs::File::open(committed).unwrap()).read_to_string(&mut text).unwrap();

        let relocated_input = format!("Input:1:{}", folder.join("multi.tex").display());
        let relocated: Vec<String> = text
            .lines()
            .map(|line| if line.starts_with("Input:1:") { relocated_input.clone() } else { line.to_string() })
            .collect();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(relocated.join("\n").as_bytes()).unwrap();
        std::fs::write(folder.join("multi.synctex.gz"), encoder.finish().unwrap()).unwrap();
    }
}
