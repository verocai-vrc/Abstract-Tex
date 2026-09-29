//! Scoped drafts (DESIGN.md §5.1 rung 4, S9.7): one chapter of a multi-file document, typeset in
//! a single pass beside the full build so the author sees the page they are writing sooner.
//!
//! LaTeX already has the mechanism. `\includeonly{chapters/03-method}` before the document makes
//! every other `\include` read its chapter's `.aux` instead of typesetting the chapter, so page
//! numbers, cross-references and citations all come out as they are in the full document. A
//! draft is therefore a small wrapper file — `\includeonly{…}` then `\input` of the real root —
//! built into its own folder, seeded with a copy of the last full build's `.aux` files. On the
//! corpus thesis that is 2.1 s against 3.4 s for the full warm pass, and 1.9 s for a draft of
//! no chapter at all: the preamble is the floor, and a draft cannot go below it.
//!
//! This module lays the draft folder out; `tectonic.rs` runs the pass. The two are separate on
//! purpose: [`prepare`] copies the full build's `.aux` files, which that build removes its warm
//! marker from and rewrites the moment it starts, so the caller runs `prepare` first, then the
//! full build and the draft pass side by side. It must never write into the full build's folder
//! or the source tree, and never run a process.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::incremental;
use crate::BuildJob;

/// What to draft and where. The caller owns the folder layout (DESIGN.md §5.8 puts it at
/// `.abstract-tex/draft/`); this crate only checks that it is safe to use.
#[derive(Debug, Clone)]
pub struct DraftJob {
    /// The chapter to typeset, spelled exactly as its `\include` wrote it
    /// (`abstract_tex_includes::Chapter::argument`): `\includeonly` compares names, not files.
    pub chapter: String,
    /// The draft's folder. Must sit beside the full build's folder (`.abstract-tex/draft` next
    /// to `.abstract-tex/build`), inside the project: `prepare` deletes and rewrites what is in
    /// it, so it has to be a folder the app owns, never one of the author's. Not *inside* the
    /// build folder either, whose every `.aux` the warm-build comparison reads.
    pub dir: PathBuf,
}

/// Where [`prepare`] put things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftLayout {
    /// The wrapper file, relative to the project folder: what the engine is asked to build.
    pub wrapper: PathBuf,
    /// The draft's own build folder, seeded from the full build's.
    pub out_dir: PathBuf,
}

/// Characters that would change what the wrapper means if they appeared in a path it writes: a
/// space or `,` inside `\includeonly{…}` splits the list, and the rest are TeX specials. Paths
/// with any of them get no draft rather than an attempt at quoting — the full build still runs.
const UNSAFE_IN_TEX: &[char] = &[' ', ',', '%', '#', '{', '}', '\\', '~', '$', '&', '^'];

/// Write the wrapper and seed the draft's build folder. Call it before the full build starts
/// (module doc). `Ok(None)` when this document cannot have a draft right now, which is never an
/// error: the full build runs anyway.
pub fn prepare(job: &BuildJob, draft: &DraftJob) -> io::Result<Option<DraftLayout>> {
    let stem = job.root_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".to_string());

    // A draft borrows the full build's numbering. Without a successful full build to borrow from,
    // chapter 3 would start on page 1 as chapter 1: a draft that looks right and is not.
    if !incremental::can_start_warm(&job.out_dir, &stem) {
        return Ok(None);
    }
    // Beside the build folder or nowhere: this is the check that keeps `remove_dir_all` below
    // away from the author's files, whatever path a caller passes.
    if draft.dir == job.out_dir || draft.dir.parent() != job.out_dir.parent() {
        return Ok(None);
    }
    let Ok(dir_in_project) = draft.dir.strip_prefix(&job.project_dir) else {
        return Ok(None);
    };
    let Some(root_from_wrapper) = relative_path_up(dir_in_project, &job.root_file) else {
        return Ok(None);
    };
    if draft.chapter.is_empty() || draft.chapter.contains(UNSAFE_IN_TEX) || root_from_wrapper.contains(UNSAFE_IN_TEX) {
        return Ok(None);
    }

    // Rebuilt from the full build every time, so nothing a previous draft wrote (another
    // chapter's `.aux`, rewritten by that draft's pass) can leak into this one.
    let out_dir = build_folder(&draft.dir);
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir)?;
    }
    fs::create_dir_all(&out_dir)?;
    seed(&job.out_dir, &out_dir)?;

    // The wrapper takes the root's name because Tectonic has no `--jobname`: the job is named
    // after the file it builds, and the `.aux` and `.bbl` it must find are the root's.
    let wrapper_name = format!("{stem}.tex");
    let wrapper_text = format!(
        "% Written by Abstract-Tex for a one-chapter draft (S9.7). Disposable: rewritten every build.\n\
         \\includeonly{{{}}}\n\\input{{{root_from_wrapper}}}\n",
        draft.chapter
    );
    fs::write(draft.dir.join(&wrapper_name), wrapper_text)?;

    Ok(Some(DraftLayout { wrapper: dir_in_project.join(wrapper_name), out_dir }))
}

/// Where a draft in `draft_dir` writes its PDF, log and `.synctex.gz`: what [`prepare`] lays out
/// and [`DraftLayout::out_dir`] names, for a caller that needs it without a layout in hand
/// (SyncTeX against the draft on screen, S9.9).
pub fn build_folder(draft_dir: &Path) -> PathBuf {
    draft_dir.join("build")
}

/// The path from a folder inside the project back up to `root_file`, forward slashes, for
/// `\input`: `.abstract-tex/draft` and `thesis/main.tex` give `../../thesis/main.tex`. Relative on
/// purpose: an absolute path would carry the project folder's own name into TeX, spaces and all.
/// `None` if `dir_in_project` climbs with `..` itself, which no caller should pass.
fn relative_path_up(dir_in_project: &Path, root_file: &Path) -> Option<String> {
    let mut path = String::new();
    for component in dir_in_project.components() {
        match component {
            std::path::Component::Normal(_) => path.push_str("../"),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    path.push_str(&root_file.to_string_lossy().replace('\\', "/"));
    Some(path)
}

/// Copy every file under `from` into `to`, keeping subfolders (`\include` puts each chapter's
/// `.aux` beside its path), except outputs ([`incremental::OUTPUT_EXTENSIONS`]) and the warm marker.
fn seed(from: &Path, to: &Path) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target)?;
            seed(&source, &target)?;
            continue;
        }
        // Not the outputs, which the draft writes itself, nor the warm marker, which means "the
        // last *full* build succeeded" and nothing else. Everything else is copied, because
        // packages keep cross-reference state in files of their own (`.toc`, `.lof`, `.bbl`,
        // `-blx.bib`, `.nav`, …) and a list of what to keep would miss the next one.
        if incremental::is_output(&source) || entry.file_name() == incremental::WARM_MARKER {
            continue;
        }
        fs::copy(&source, &target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project whose last full build succeeded: the warm marker, the root and chapter `.aux`
    /// files, and the outputs a draft must not inherit.
    fn built_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let build = dir.path().join(".abstract-tex/build");
        fs::create_dir_all(build.join("chapters")).unwrap();
        for (name, text) in [
            ("main.aux", "\\@input{chapters/two.aux}\n"),
            ("chapters/two.aux", "\\setcounter{page}{12}\n"),
            ("main.toc", "\\contentsline{chapter}{Two}{12}\n"),
            ("main.pdf", "%PDF"),
            ("main.log", "log"),
            ("main.synctex.gz", "gz"),
            (incremental::WARM_MARKER, ""),
        ] {
            fs::write(build.join(name), text).unwrap();
        }
        dir
    }

    fn job(dir: &Path, root: &str) -> BuildJob {
        BuildJob {
            project_dir: dir.to_path_buf(),
            root_file: PathBuf::from(root),
            out_dir: dir.join(".abstract-tex/build"),
            synctex: true,
            shell_escape: false,
        }
    }

    fn draft(dir: &Path, chapter: &str) -> DraftJob {
        DraftJob { chapter: chapter.to_string(), dir: dir.join(".abstract-tex/draft") }
    }

    #[test]
    fn the_wrapper_names_the_chapter_and_inputs_the_root_by_a_relative_path() {
        let project = built_project();
        let layout = prepare(&job(project.path(), "main.tex"), &draft(project.path(), "chapters/two")).unwrap().unwrap();
        assert_eq!(layout.wrapper, Path::new(".abstract-tex/draft/main.tex"));
        let wrapper = fs::read_to_string(project.path().join(&layout.wrapper)).unwrap();
        assert!(wrapper.contains("\\includeonly{chapters/two}\n\\input{../../main.tex}\n"), "{wrapper}");
    }

    #[test]
    fn a_root_in_a_subfolder_is_reached_through_it_and_the_wrapper_takes_its_name() {
        let project = built_project();
        fs::rename(project.path().join(".abstract-tex/build/main.aux"), project.path().join(".abstract-tex/build/thesis.aux")).unwrap();
        let layout = prepare(&job(project.path(), "book/thesis.tex"), &draft(project.path(), "two")).unwrap().unwrap();
        assert_eq!(layout.wrapper, Path::new(".abstract-tex/draft/thesis.tex"));
        let wrapper = fs::read_to_string(project.path().join(&layout.wrapper)).unwrap();
        assert!(wrapper.contains("\\input{../../book/thesis.tex}"), "{wrapper}");
    }

    #[test]
    fn the_draft_folder_gets_the_cross_references_and_none_of_the_outputs() {
        let project = built_project();
        let layout = prepare(&job(project.path(), "main.tex"), &draft(project.path(), "chapters/two")).unwrap().unwrap();
        let has = |name: &str| layout.out_dir.join(name).is_file();
        assert!(has("main.aux") && has("chapters/two.aux") && has("main.toc"));
        assert!(!has("main.pdf") && !has("main.log") && !has("main.synctex.gz"));
        assert!(!has(incremental::WARM_MARKER), "the marker speaks for the full build only");
    }

    #[test]
    fn the_full_build_folder_is_left_exactly_as_it_was() {
        let project = built_project();
        let build = project.path().join(".abstract-tex/build");
        let before = crate::incremental::AuxSnapshot::read(&build);
        let files_before = fs::read_dir(&build).unwrap().count();
        prepare(&job(project.path(), "main.tex"), &draft(project.path(), "chapters/two")).unwrap().unwrap();
        assert_eq!(crate::incremental::AuxSnapshot::read(&build), before);
        assert_eq!(fs::read_dir(&build).unwrap().count(), files_before);
    }

    #[test]
    fn whatever_the_last_draft_left_behind_is_cleared() {
        let project = built_project();
        let (j, d) = (job(project.path(), "main.tex"), draft(project.path(), "chapters/two"));
        let layout = prepare(&j, &d).unwrap().unwrap();
        fs::write(layout.out_dir.join("chapters/two.aux"), "rewritten by the last draft").unwrap();
        fs::write(layout.out_dir.join("stale.aux"), "").unwrap();
        prepare(&j, &d).unwrap().unwrap();
        assert_eq!(fs::read_to_string(layout.out_dir.join("chapters/two.aux")).unwrap(), "\\setcounter{page}{12}\n");
        assert!(!layout.out_dir.join("stale.aux").exists());
    }

    #[test]
    fn no_draft_without_a_successful_full_build_to_borrow_numbering_from() {
        let project = built_project();
        fs::remove_file(project.path().join(".abstract-tex/build").join(incremental::WARM_MARKER)).unwrap();
        assert_eq!(prepare(&job(project.path(), "main.tex"), &draft(project.path(), "chapters/two")).unwrap(), None);
        assert!(!project.path().join(".abstract-tex/draft").exists(), "nothing is written when there is no draft");
    }

    #[test]
    fn no_draft_for_a_name_tex_would_read_differently() {
        let project = built_project();
        for chapter in ["my chapter", "a,b", "50%", "", "chapters/\\name"] {
            let result = prepare(&job(project.path(), "main.tex"), &draft(project.path(), chapter)).unwrap();
            assert_eq!(result, None, "{chapter:?}");
        }
        let spaced_root = prepare(&job(project.path(), "my thesis.tex"), &draft(project.path(), "two")).unwrap();
        assert_eq!(spaced_root, None);
    }

    #[test]
    fn no_draft_anywhere_but_beside_the_build_folder() {
        let project = built_project();
        fs::create_dir_all(project.path().join("chapters/build")).unwrap();
        fs::write(project.path().join("chapters/build/figure.pdf"), "the author's").unwrap();
        let j = job(project.path(), "main.tex");
        let elsewhere = tempfile::tempdir().unwrap();
        for dir in [
            j.out_dir.join("draft"),
            j.out_dir.clone(),
            project.path().to_path_buf(),
            project.path().join("chapters"),
            elsewhere.path().to_path_buf(),
        ] {
            let refused = DraftJob { chapter: "two".into(), dir: dir.clone() };
            assert_eq!(prepare(&j, &refused).unwrap(), None, "{}", dir.display());
        }
        assert!(project.path().join("chapters/build/figure.pdf").is_file(), "an author's folder named build is never cleared");
    }
}
