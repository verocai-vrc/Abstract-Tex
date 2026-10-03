//! S9.12: what a `latexmk` build is allowed to touch, against a real TeX distribution.
//!
//! Every test here is `#[ignore]`d, like every other real-engine test in this crate: they need
//! `latexmk` and `pdflatex` on `PATH`, which a CI runner installs and a developer machine may
//! not have. `cargo test -p abstract-tex-engine --test latexmk -- --ignored` runs them.
//!
//! The one claim they exist for is DESIGN.md §5.8's: nothing but the manuscript belongs in the
//! source tree. `latexmk` has no equivalent of Tectonic's `-Z shell-escape-cwd`, so the only way
//! to keep the commands a document runs out of the author's folder is to not stand the engine in
//! it — and the tests below check both halves of that: that the commands land in the build
//! folder, and that moving the engine did not cost the document anything it used to find.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use abstract_tex_engine::latexmk::{Latexmk, TexProgram};
use abstract_tex_engine::{BuildJob, Engine};
use tokio_util::sync::CancellationToken;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/shell-escape")
}

/// A copy of the fixture in a temporary folder, so a build cannot dirty the repository even if
/// this loop's whole premise turns out to be wrong.
fn project_copy(into: &Path) {
    copy_dir(&fixture(), into);
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every file the author would see, with its bytes: the whole source tree except `.abstract-tex/`,
/// which is ours. Bytes and not just names, because a `\write18` that *appended* to a file the
/// project already has would be just as wrong as one that created a new one.
fn source_tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect(dir, dir, &mut files);
    files
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|name| name == ".abstract-tex") {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                fs::read(&path).unwrap(),
            );
        }
    }
}

fn job(project_dir: &Path, shell_escape: bool) -> BuildJob {
    BuildJob {
        project_dir: project_dir.to_path_buf(),
        root_file: PathBuf::from("main.tex"),
        out_dir: project_dir.join(".abstract-tex/build"),
        synctex: true,
        shell_escape,
    }
}

fn engine() -> Latexmk {
    Latexmk::locate(TexProgram::PdfLatex).expect("needs latexmk and pdflatex on PATH")
}

/// The loop's own claim. With shell escape on, the document's two `\write18` commands run in the
/// build folder, so neither of them can reach the source tree — and everything the document
/// reads still resolves from there, which is the half that makes the move survivable.
#[tokio::test]
#[ignore]
async fn shell_escape_writes_into_the_build_folder_and_never_the_source_tree() {
    let tmp = tempfile::tempdir().unwrap();
    project_copy(tmp.path());
    let before = source_tree(tmp.path());
    let job = job(tmp.path(), true);

    let outcome = engine()
        .build(&job, CancellationToken::new(), None)
        .await
        .unwrap();
    assert!(outcome.success, "{}", outcome.stderr);
    assert!(outcome.pdf.is_some() && outcome.log.is_some() && outcome.synctex.is_some());

    assert_eq!(
        source_tree(tmp.path()),
        before,
        "a shell command wrote into the source tree"
    );

    // Where the commands actually ran, in their own words: `pwd` printed by the document.
    let marker = fs::read_to_string(job.out_dir.join("where-commands-run.txt"))
        .expect("the marker must be in the build folder");
    assert_eq!(
        fs::canonicalize(marker.trim()).unwrap(),
        fs::canonicalize(&job.out_dir).unwrap(),
        "the commands ran somewhere other than the build folder"
    );

    // The document `\input`s, `\include`s, loads a `.sty` beside the manuscript, finds a figure
    // through `\graphicspath` and cites a `.bib` — all through the search paths `latexmk.rs`
    // sets, since the `.` that used to answer them is now the build folder. A missing one is not
    // always fatal to pdfTeX, so check the two that would only warn.
    let log = String::from_utf8_lossy(&fs::read(outcome.log.as_ref().unwrap()).unwrap()).into_owned();
    assert!(
        !log.contains("Citation `knuth1984' on page"),
        "BibTeX did not find refs.bib through BIBINPUTS:\n{log}"
    );
    assert!(
        !log.contains("File `dot' not found"),
        "graphicx did not find the figure through TEXINPUTS:\n{log}"
    );
    assert!(
        job.out_dir.join("chapters/one.aux").is_file(),
        "the \\include'd chapter's own .aux is missing"
    );
    assert!(
        job.out_dir.join("main.bbl").is_file(),
        "no .bbl: BibTeX never ran, or ran somewhere else"
    );
}

/// The documented price of the move, pinned so the day it changes, someone is told. A command
/// handed a path relative to the *project* now resolves it against the build folder and finds
/// nothing — `\write18{cat code/hello.py}` here, and `\inputminted{python}{code/hello.py}` in a
/// real document, which is the same command with a friendlier name. Tectonic's
/// `-Z shell-escape-cwd` (S9.8) pays exactly this price too.
#[tokio::test]
#[ignore]
async fn a_shell_command_given_a_project_relative_path_finds_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    project_copy(tmp.path());
    let job = job(tmp.path(), true);

    engine()
        .build(&job, CancellationToken::new(), None)
        .await
        .unwrap();

    let read_back = fs::read(job.out_dir.join("read-a-relative-path.txt"))
        .expect("the command still ran, and still redirected");
    assert!(
        read_back.is_empty(),
        "a relative project path resolved from the build folder after all: {read_back:?}"
    );
    assert!(
        !tmp.path().join("read-a-relative-path.txt").exists(),
        "and it was not written into the source tree either"
    );
}

/// Only the shell-escape branch moves. A build that cannot run commands cannot write anywhere it
/// should not, so it keeps running in the project folder exactly as it did before S9.12 — which
/// is worth a test of its own because the two branches differ in what the *log* says: this one
/// names the manuscript `./main.tex`, the one above names it by its whole path. `compile.rs` is
/// where that difference is taken back out again before the drawer sees it.
#[tokio::test]
#[ignore]
async fn without_shell_escape_nothing_moves_and_the_log_still_names_files_relatively() {
    let tmp = tempfile::tempdir().unwrap();
    project_copy(tmp.path());
    let before = source_tree(tmp.path());
    let job = job(tmp.path(), false);

    let outcome = engine()
        .build(&job, CancellationToken::new(), None)
        .await
        .unwrap();
    assert!(outcome.success, "{}", outcome.stderr);
    assert_eq!(source_tree(tmp.path()), before);

    // Restricted mode refuses both `\write18`s, so neither file exists anywhere.
    assert!(!tmp.path().join("where-commands-run.txt").exists());
    assert!(!job.out_dir.join("where-commands-run.txt").exists());

    // TeX wraps the transcript at 79 columns, so an absolute path would be split across lines and
    // a naive search for it would miss. Unwrapping the way `texlog`'s tokenizer does is more than
    // this test needs: the relative spelling is short enough to survive on one line.
    let log = String::from_utf8_lossy(&fs::read(outcome.log.as_ref().unwrap()).unwrap()).into_owned();
    assert!(
        log.contains("(./main.tex"),
        "expected the relative spelling in the log:\n{log}"
    );
}
