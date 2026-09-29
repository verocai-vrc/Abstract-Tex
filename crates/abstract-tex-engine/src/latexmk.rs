//! A system TeX distribution (TeX Live or MiKTeX) driven through `latexmk` (S9.4).
//!
//! Tectonic is the default because it needs nothing installed (DESIGN.md §2 rule 4). A project
//! switches here when it needs what the bundle lacks: Biber for biblatex, pdfLaTeX's fonts and
//! speed, a package the bundle does not carry. `latexmk` is the one program both distributions
//! ship that already knows how many passes a document needs and when to run BibTeX or Biber, so
//! this engine hands it the whole job rather than reimplementing that loop.
//!
//! It must never install, update or configure a distribution (DESIGN.md §1.3: not a TeX
//! distribution) — it finds one on `PATH` or reports that there is none. And it turns on
//! shell-escape only when the job says so, which only the person at the machine can make it say
//! (`BuildJob::shell_escape`, S9.8): a project file must never be able to grant that (S9.4).
//!
//! It also decides *where* a build runs, which S9.12 made a real decision: with shell escape on,
//! the process runs in the build folder rather than the project, so that the commands the
//! document runs cannot write into the source tree. `Latexmk::invocation` is the whole of that
//! decision and says why (it is private; `cargo doc --document-private-items` shows it).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::process::{self, hide_console_window};
use crate::{BuildJob, BuildOutcome, BuildSteps, Engine, EngineError, EngineInfo, ProgressSink};

/// Which TeX program `latexmk` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexProgram {
    PdfLatex,
    XeLatex,
    LuaLatex,
}

impl TexProgram {
    /// The program's own binary, which must be on `PATH` next to `latexmk` for a build to work.
    pub fn binary_name(self) -> &'static str {
        match self {
            TexProgram::PdfLatex => "pdflatex",
            TexProgram::XeLatex => "xelatex",
            TexProgram::LuaLatex => "lualatex",
        }
    }

    /// The `latexmk` switch that selects it.
    fn latexmk_flag(self) -> &'static str {
        match self {
            TexProgram::PdfLatex => "-pdf",
            TexProgram::XeLatex => "-xelatex",
            TexProgram::LuaLatex => "-lualatex",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Latexmk {
    binary: PathBuf,
    program: TexProgram,
}

impl Latexmk {
    /// Find `latexmk` and the program it would run on `PATH`, or `EngineError::NotFound`. Both,
    /// because a `latexmk` whose `pdflatex` is missing fails on the first build, with an error
    /// about the distribution rather than the document — better said now, once.
    pub fn locate(program: TexProgram) -> Result<Self, EngineError> {
        let binary = abstract_tex_sidecar::find_on_path(&abstract_tex_sidecar::exe_name("latexmk"))
            .ok_or(EngineError::NotFound)?;
        abstract_tex_sidecar::find_on_path(&abstract_tex_sidecar::exe_name(program.binary_name()))
            .ok_or(EngineError::NotFound)?;
        Ok(Self { binary, program })
    }

    /// Use a specific `latexmk`. Tests use this.
    pub fn at(binary: impl Into<PathBuf>, program: TexProgram) -> Self {
        Self { binary: binary.into(), program }
    }

    /// Where this build runs, what `latexmk` is told, and what is added to its environment.
    ///
    /// With shell escape off — which is nearly every build, since turning it on takes an answer
    /// from the person at the machine (S9.8) — this is exactly what S9.4 shipped: run in the
    /// project folder, name the root file the way the project spells it, add nothing.
    ///
    /// With shell escape on, all three move. `\write18` runs its commands in the process's own
    /// working directory and pays no attention to `-outdir`, so a document that shells out
    /// writes into the author's source tree. Confirmed by hand on 29 September 2026 with
    /// `\immediate\write18{pwd > marker}`, which left `marker` beside the `.tex` (ledger).
    /// Tectonic has `-Z shell-escape-cwd` for exactly this (S9.8); `latexmk` has no such switch,
    /// and the process's own working directory is the only lever there is. Moving it to the
    /// build folder keeps DESIGN.md §5.8's promise that nothing but the manuscript lives in the
    /// source tree.
    ///
    /// Only this branch moves. Moving *every* build's working folder would change what every log
    /// says about every file (below) in order to fix something that cannot happen unless shell
    /// escape is on, so the narrow change is the honest one.
    ///
    /// Two consequences, both measured against this machine's TeX Live rather than assumed:
    ///
    /// - **Everything the document reads still resolves**, because the three search paths in
    ///   [`shell_escape_environment`] put the project folder back in front of the `.` that the
    ///   move took away. `\input`, `\include`, a `.sty` beside the manuscript,
    ///   `\includegraphics` through `\graphicspath`, and BibTeX's own `.bib` were each checked.
    /// - **The log names files absolutely.** `(./main.tex` becomes `(/home/…/main.tex`, because
    ///   that is how kpathsea found it. `texlog` passes names on exactly as TeX printed them —
    ///   it is chartered never to know where the project is — so `compile.rs` makes them
    ///   project-relative again before the drawer sees them. SyncTeX needs nothing: it writes
    ///   absolute paths either way, which is what the comparison in `abstract-tex-synctex`
    ///   already expects.
    ///
    /// The price, in the ledger rather than hidden: a shell command handed a project-relative
    /// path no longer resolves it. `\write18{cat code/hello.py}` — and `\inputminted{python}
    /// {code/hello.py}`, which is the same thing — now looks under the build folder and finds
    /// nothing. Tectonic's `shell-escape-cwd` pays exactly the same price.
    fn invocation(&self, job: &BuildJob) -> Invocation {
        let mut args = vec![
            self.program.latexmk_flag().to_string(),
            // Never stop to ask on the terminal nobody is looking at; stop at the first error,
            // the same way the bundled engine does, so texlog sees the same shape of log.
            "-interaction=nonstopmode".to_string(),
            "-halt-on-error".to_string(),
            // `-outdir` keeps every artifact out of the source tree (DESIGN.md §5.8), and across
            // builds, which is what makes latexmk's own warm builds warm.
            format!("-outdir={}", job.out_dir.to_string_lossy()),
        ];
        if job.synctex {
            args.push("-synctex=1".to_string());
        }
        if !job.shell_escape {
            args.push(job.root_file.to_string_lossy().into_owned());
            return Invocation { working_dir: job.project_dir.clone(), args, env: Vec::new() };
        }
        args.push("-shell-escape".to_string());
        // From the build folder the root file's own name names nothing, so pass the whole path.
        // The full path rather than a `../..` walk out of the build folder: either would build,
        // but only one of them is readable in a log.
        args.push(job.project_dir.join(&job.root_file).to_string_lossy().into_owned());
        Invocation {
            working_dir: job.out_dir.clone(),
            args,
            env: shell_escape_environment(&job.project_dir, &job.out_dir),
        }
    }
}

/// Everything one run of `latexmk` needs: where to run it, what to pass it, what to add to its
/// environment. One value rather than three returns, because with S9.12 the three are one
/// decision — the working folder is only ever moved together with the search paths that make the
/// move survivable.
#[derive(Debug)]
struct Invocation {
    working_dir: PathBuf,
    args: Vec<String>,
    env: Vec<(&'static str, OsString)>,
}

/// How kpathsea separates the entries of a search path. Windows uses `;`, because `:` is already
/// the drive letter's own punctuation there.
const SEARCH_PATH_SEPARATOR: &str = if cfg!(windows) { ";" } else { ":" };

/// What a shell-escape build needs in its environment once its working folder has moved off the
/// project (see [`Latexmk::invocation`]).
///
/// Three search paths, not one: kpathsea keeps a separate variable per kind of file, and naming
/// the wrong one fails silently. `TEXINPUTS` covers `.tex` and `.sty`, and `\includegraphics`
/// through it; `BIBINPUTS` covers `.bib`; `BSTINPUTS` covers `.bst`. Each gets the project
/// folder put in front of whatever was already set.
///
/// `TEXMF_OUTPUT_DIRECTORY` is for `minted` v3 and anything else built on `latexrestricted`,
/// which refuses to write outside the folders it believes TeX is writing to. minted's error text
/// names MiKTeX, and the ledger recorded that as "MiKTeX may need a different mechanism";
/// reading `latexrestricted`'s own source (0.6.2, shipped with this TeX Live) settles it —
/// `tex_openout_roots` reads the variable with a plain `os.getenv` and has no distribution
/// branch at all. The name is not MiKTeX's, it is every distribution's. It is set here even
/// though the moved working folder already satisfies it, because saying what we mean costs one
/// line, and because it is what a MiKTeX machine — still unverified, there is none here — needs.
fn shell_escape_environment(project_dir: &Path, out_dir: &Path) -> Vec<(&'static str, OsString)> {
    let mut env: Vec<(&'static str, OsString)> = ["TEXINPUTS", "BIBINPUTS", "BSTINPUTS"]
        .into_iter()
        .map(|name| (name, prepend_search_path(project_dir, std::env::var_os(name))))
        .collect();
    env.push(("TEXMF_OUTPUT_DIRECTORY", out_dir.as_os_str().to_os_string()));
    env
}

/// Put `project_dir` at the front of one kpathsea search path without taking anything away.
///
/// kpathsea reads these as a separator-joined list in which an *empty* entry means "and the
/// distribution's own default list, here". So an unset variable becomes `<project>:`, whose
/// trailing separator leaves the defaults in place — without it a build stops finding
/// `article.cls`, which is how this was found. A variable someone had already set keeps every
/// entry they wrote, in their order, after the project folder.
fn prepend_search_path(project_dir: &Path, existing: Option<OsString>) -> OsString {
    let mut value = project_dir.as_os_str().to_os_string();
    value.push(SEARCH_PATH_SEPARATOR);
    if let Some(existing) = existing {
        value.push(existing);
    }
    value
}

/// The same job with its two folders resolved against this process's own working folder.
///
/// The app always passes absolute paths, so in the app this changes nothing. A test need not,
/// and a relative folder would quietly stop meaning the same thing the moment the build's own
/// working folder moves off the project — so it is resolved once, here, before anything reads
/// it. `Path::join` returns its argument unchanged when that argument is already absolute.
fn with_absolute_folders(job: &BuildJob) -> Result<BuildJob, EngineError> {
    let here = std::env::current_dir()?;
    Ok(BuildJob {
        project_dir: here.join(&job.project_dir),
        out_dir: here.join(&job.out_dir),
        root_file: job.root_file.clone(),
        synctex: job.synctex,
        shell_escape: job.shell_escape,
    })
}

#[async_trait::async_trait]
impl Engine for Latexmk {
    async fn probe(&self) -> Result<EngineInfo, EngineError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-v");
        hide_console_window(&mut cmd);
        let output = cmd.output().await.map_err(EngineError::Spawn)?;
        // `latexmk -v` prints a blank line, then "Latexmk, John Collins, <date>. Version 4.x".
        let text = String::from_utf8_lossy(&output.stdout);
        let version = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default().to_string();
        Ok(EngineInfo {
            name: format!("latexmk ({})", self.program.binary_name()),
            version,
            path: self.binary.clone(),
        })
    }

    async fn build(
        &self,
        job: &BuildJob,
        cancel: CancellationToken,
        progress: Option<ProgressSink>,
    ) -> Result<BuildOutcome, EngineError> {
        let job = &with_absolute_folders(job)?;
        tokio::fs::create_dir_all(&job.out_dir).await?;
        let started = Instant::now();
        let invocation = self.invocation(job);
        let (status, stderr) =
            process::run(&self.binary, &invocation.working_dir, &invocation.args, &invocation.env, &cancel, progress).await?;
        let duration = started.elapsed();

        let stem = job.root_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".to_string());
        let artifact = |ext: &str| {
            let path = job.out_dir.join(format!("{stem}.{ext}"));
            path.is_file().then_some(path)
        };
        info!(success = status.success(), ms = duration.as_millis(), program = self.program.binary_name(), "latexmk finished");
        Ok(BuildOutcome {
            success: status.success(),
            pdf: artifact("pdf"),
            log: artifact("log"),
            synctex: artifact("synctex.gz"),
            stderr,
            exit_code: status.code(),
            duration,
            // latexmk decides its own passes and does not report them; from here it is one step.
            steps: BuildSteps { single_passes: 0, full: true },
        })
    }
}

/// Where a project's builds run, as `abstract-tex.toml`'s `engine` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineChoice {
    /// The bundled Tectonic: the default, and what an absent or empty setting means.
    Tectonic,
    /// A system distribution's `latexmk`, running this program.
    System(TexProgram),
}

impl EngineChoice {
    /// Read the setting. An unknown value is an error in a sentence the app can show as-is,
    /// never a silent fall back to the default: an author who typed `pdftex` should be told.
    pub fn parse(setting: Option<&str>) -> Result<Self, String> {
        match setting.map(str::trim) {
            None | Some("") | Some("tectonic") => Ok(EngineChoice::Tectonic),
            Some("pdflatex") => Ok(EngineChoice::System(TexProgram::PdfLatex)),
            Some("xelatex") => Ok(EngineChoice::System(TexProgram::XeLatex)),
            Some("lualatex") => Ok(EngineChoice::System(TexProgram::LuaLatex)),
            Some(other) => Err(format!(
                "abstract-tex.toml asks for the engine \"{other}\", which is not one of \"tectonic\", \"pdflatex\", \"xelatex\" or \"lualatex\"."
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn job(synctex: bool) -> BuildJob {
        BuildJob {
            project_dir: PathBuf::from("proj"),
            root_file: PathBuf::from("thesis.tex"),
            out_dir: Path::new("proj").join(".abstract-tex").join("build"),
            synctex,
            shell_escape: false,
        }
    }

    /// The argument list only, for the assertions that do not care where the build runs.
    fn args_for(job: &BuildJob) -> Vec<String> {
        Latexmk::at("latexmk", TexProgram::LuaLatex).invocation(job).args
    }

    #[test]
    fn the_program_flag_comes_first_and_the_root_file_last() {
        let args = args_for(&job(true));
        assert_eq!(args.first().map(String::as_str), Some("-lualatex"));
        assert_eq!(args.last().map(String::as_str), Some("thesis.tex"));
        assert!(args.contains(&"-halt-on-error".to_string()));
        assert!(args.contains(&"-synctex=1".to_string()));
        assert!(args.iter().any(|a| a.starts_with("-outdir=") && a.ends_with("build")));
    }

    #[test]
    fn synctex_is_only_requested_when_the_job_asks() {
        let args = args_for(&job(false));
        assert!(!args.iter().any(|a| a.starts_with("-synctex")));
    }

    #[test]
    fn shell_escape_is_on_only_when_the_job_says_so() {
        for program in [TexProgram::PdfLatex, TexProgram::XeLatex, TexProgram::LuaLatex] {
            let engine = Latexmk::at("latexmk", program);
            let args = engine.invocation(&job(true)).args;
            assert!(!args.iter().any(|a| a.contains("shell-escape") || a == "-shell-restricted"), "{args:?}");
            let allowed = engine.invocation(&BuildJob { shell_escape: true, ..job(true) }).args;
            assert!(allowed.contains(&"-shell-escape".to_string()), "{allowed:?}");
            // The root file is still last, now named by its path from outside the project folder.
            assert_eq!(allowed.last().map(String::as_str), Some(Path::new("proj").join("thesis.tex").to_string_lossy().as_ref()));
        }
    }

    /// S9.12. Every build without shell escape runs where every build ran before it: a document
    /// that cannot run commands cannot write anywhere it should not, so there is nothing to buy
    /// with a moved working folder and a log full of absolute paths to pay for it with.
    #[test]
    fn without_shell_escape_the_build_runs_in_the_project_folder_and_touches_no_environment() {
        let invocation = Latexmk::at("latexmk", TexProgram::PdfLatex).invocation(&job(true));
        assert_eq!(invocation.working_dir, PathBuf::from("proj"));
        assert!(invocation.env.is_empty(), "{:?}", invocation.env);
    }

    /// S9.12, the loop's own claim: with shell escape on, the commands the document runs cannot
    /// land in the source tree, because the process is not standing in it.
    #[test]
    fn with_shell_escape_the_build_runs_in_the_build_folder_with_the_project_on_every_search_path() {
        let job = BuildJob { shell_escape: true, ..job(true) };
        let invocation = Latexmk::at("latexmk", TexProgram::PdfLatex).invocation(&job);
        assert_eq!(invocation.working_dir, job.out_dir);

        let named = |name: &str| {
            invocation.env.iter().find(|(key, _)| *key == name).map(|(_, value)| value.clone())
        };
        // All three, because kpathsea has one variable per kind of file and a missing one is a
        // silent failure: no `.sty` beside the manuscript, or no `.bib`, or no `.bst`.
        for name in ["TEXINPUTS", "BIBINPUTS", "BSTINPUTS"] {
            let value = named(name).unwrap_or_else(|| panic!("{name} must be set"));
            assert!(
                value.to_string_lossy().starts_with("proj"),
                "{name} must look in the project folder first: {value:?}"
            );
        }
        assert_eq!(named("TEXMF_OUTPUT_DIRECTORY"), Some(job.out_dir.as_os_str().to_os_string()));
    }

    /// The trailing separator is the whole reason `article.cls` is still found: to kpathsea an
    /// empty entry means "the distribution's own defaults here".
    #[test]
    fn a_search_path_ends_open_and_keeps_whatever_was_already_set() {
        let unset = prepend_search_path(Path::new("proj"), None);
        assert_eq!(unset.to_string_lossy(), format!("proj{SEARCH_PATH_SEPARATOR}"));

        let already = prepend_search_path(Path::new("proj"), Some(OsString::from("/opt/tex/local")));
        assert_eq!(already.to_string_lossy(), format!("proj{SEARCH_PATH_SEPARATOR}/opt/tex/local"));
    }

    #[test]
    fn the_setting_parses_to_a_choice_and_rejects_anything_else_in_a_sentence() {
        assert_eq!(EngineChoice::parse(None), Ok(EngineChoice::Tectonic));
        assert_eq!(EngineChoice::parse(Some("tectonic")), Ok(EngineChoice::Tectonic));
        assert_eq!(EngineChoice::parse(Some(" pdflatex ")), Ok(EngineChoice::System(TexProgram::PdfLatex)));
        assert_eq!(EngineChoice::parse(Some("lualatex")), Ok(EngineChoice::System(TexProgram::LuaLatex)));
        let error = EngineChoice::parse(Some("pdftex")).unwrap_err();
        assert!(error.contains("\"pdftex\"") && error.ends_with('.'), "{error}");
    }

    /// The real distribution, when this machine has one. Ignored like every real-engine test.
    #[tokio::test]
    #[ignore]
    async fn builds_the_minimal_fixture_with_a_system_pdflatex() {
        let engine = Latexmk::locate(TexProgram::PdfLatex).expect("needs latexmk and pdflatex on PATH");
        let tmp = tempfile::tempdir().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/main.tex");
        std::fs::copy(&src, tmp.path().join("main.tex")).unwrap();
        let job = BuildJob {
            project_dir: tmp.path().to_path_buf(),
            root_file: PathBuf::from("main.tex"),
            out_dir: tmp.path().join(".abstract-tex/build"),
            synctex: true,
            shell_escape: false,
        };
        let outcome = engine.build(&job, CancellationToken::new(), None).await.unwrap();
        assert!(outcome.success, "{}", outcome.stderr);
        assert!(outcome.pdf.is_some() && outcome.log.is_some() && outcome.synctex.is_some());
    }
}
