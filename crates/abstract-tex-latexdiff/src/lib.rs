//! Renders a `latexdiff`-marked-up document between any two Git revisions (DESIGN.md §5.7: "pick
//! any two points in history... and get a compiled PDF with insertions underlined and deletions
//! struck through, via `latexdiff`"), S11.4b.
//!
//! This crate owns exactly one step of that: given two commits already readable through
//! `abstract-tex-git`, produce a single `.tex` file carrying the markup, ready for
//! `abstract-tex-engine` to compile. It knows nothing about compiling, nothing about Tauri, and
//! nothing about the Graph list that will eventually call it (S11.4c, S11.4d).
//!
//! **What it must never do:**
//!
//! - Never bundle `latexdiff`. Unlike Tectonic and TexLab (`abstract-tex-sidecar`), it is a
//!   CTAN/TeX-Live tool this app does not ship — the zero-setup promise is about *this app*, not
//!   about every tool a power feature might reach for. A machine without it gets a sentence
//!   naming where to get one, never a silent failure pretending to have compared anything.
//! - Never diff a glob of files it invented. `--flatten` inlines whatever `\input`/`\include`
//!   the two exported trees already have; this crate adds no pattern of its own.

use std::path::{Path, PathBuf};
use std::process::Command;

use abstract_tex_git::{GitError, Oid, Repository};

#[derive(Debug, thiserror::Error)]
pub enum LatexdiffError {
    #[error("Git reported: {0}")]
    Git(#[from] GitError),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// S11.4b: `latexdiff` is a separate install (TeX Live and MiKTeX both ship it; CTAN has it
    /// on its own) — named here rather than left as a bare "command not found" from the shell.
    #[error("latexdiff isn't installed on this machine. It ships with TeX Live and MiKTeX, or install it separately from ctan.org/pkg/latexdiff.")]
    NotInstalled,

    /// S11.4b: `latexdiff` ran and refused — malformed LaTeX in one of the two revisions, most
    /// often — and its own stderr is the only sentence that actually says why.
    #[error("latexdiff reported: {0}")]
    Failed(String),
}

/// How much markup `latexdiff` is asked for (design interview A7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Markup {
    /// `latexdiff`'s own defaults. The first attempt always uses these.
    #[default]
    Standard,
    /// The retry after a comparison failed to compile. `latexdiff`'s markup is known to break some
    /// documents — inside maths, tables, around citations and some custom macros — and these two
    /// switches are what the design interview named for that case: maths is marked up as a whole
    /// rather than token by token, and figures are not marked at all. Less detail, more documents
    /// that still compile.
    Simpler,
}

impl Markup {
    /// The switches this setting adds to `--flatten`, which is always passed.
    fn switches(self) -> &'static [&'static str] {
        match self {
            Markup::Standard => &[],
            Markup::Simpler => &["--math-markup=whole", "--graphics-markup=none"],
        }
    }
}

/// The two folders the revisions are exported into. A struct and not two bare `&Path`s: they sit
/// side by side in every call, and swapping them would mark up the wrong direction without a
/// compile error.
#[derive(Debug, Clone, Copy)]
pub struct ExportDirs<'a> {
    /// Where the older revision is exported.
    pub old: &'a Path,
    /// Where the newer revision is exported, and where the marked-up document is written.
    pub new: &'a Path,
}

/// Export `old` and `new` to `dirs.old` and `dirs.new`, then run `latexdiff --flatten` between
/// their copies of `root_file`, writing the result back over `new_dir`'s copy and returning its
/// path — ready for `abstract-tex-engine` to compile exactly as it would compile any other root
/// file, because every other file `root_file` depends on (figures, a bibliography, a document
/// class) is already sitting beside it in `new_dir`, exported along with everything else in that
/// revision's tree.
pub fn render(
    repository: &Repository,
    old: Oid,
    new: Oid,
    root_file: &str,
    dirs: ExportDirs<'_>,
    markup: Markup,
) -> Result<PathBuf, LatexdiffError> {
    render_with(
        Path::new("latexdiff"),
        repository,
        old,
        new,
        root_file,
        dirs,
        markup,
    )
}

/// Same, naming the `latexdiff` binary to run. `pub` only for `tests/latexdiff.rs`, which points
/// this at one of the `fake-latexdiff-*` binaries (`src/bin/`) — the same reason
/// `src-tauri/src/lfs.rs`'s `track_with` is public: it proves this crate's own logic against real
/// subprocess behaviour, a real exit code and real stderr, without this development machine (or
/// most CI runners) needing a real `latexdiff` install to make the test suite pass.
pub fn render_with(
    latexdiff: &Path,
    repository: &Repository,
    old: Oid,
    new: Oid,
    root_file: &str,
    dirs: ExportDirs<'_>,
    markup: Markup,
) -> Result<PathBuf, LatexdiffError> {
    if !is_installed(latexdiff) {
        return Err(LatexdiffError::NotInstalled);
    }

    abstract_tex_git::export_tree(repository, old, dirs.old)?;
    abstract_tex_git::export_tree(repository, new, dirs.new)?;
    render_exported(latexdiff, root_file, dirs, markup)
}

/// Mark up `root_file` between two folders that already hold the files, writing the result over
/// the new folder's copy and returning its path. [`render_with`] is this after exporting two
/// commits; the app calls it directly when the newer side is the working copy (A11), which is
/// copied into place rather than exported from Git.
pub fn render_exported(
    latexdiff: &Path,
    root_file: &str,
    dirs: ExportDirs<'_>,
    markup: Markup,
) -> Result<PathBuf, LatexdiffError> {
    if !is_installed(latexdiff) {
        return Err(LatexdiffError::NotInstalled);
    }
    let old_root = dirs.old.join(root_file);
    let new_root = dirs.new.join(root_file);
    let output = Command::new(latexdiff)
        .arg("--flatten")
        .args(markup.switches())
        .arg(&old_root)
        .arg(&new_root)
        .output()?;
    if !output.status.success() {
        return Err(LatexdiffError::Failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    std::fs::write(&new_root, &output.stdout)?;
    Ok(new_root)
}

/// Whether `latexdiff` answers at all, checked fresh on every call — the same reasoning
/// `src-tauri/src/lfs.rs` gives for Git LFS: an install can change between one call and the
/// next, and the only honest way to answer "is it here right now" is to ask right now.
///
/// `pub` since S11.4c, which asks before it clears the previous comparison's folder: a machine
/// with no `latexdiff` should lose nothing to a comparison that could never have run.
pub fn is_installed(latexdiff: &Path) -> bool {
    Command::new(latexdiff)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}
