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

use std::path::PathBuf;
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

    fn arguments(&self, job: &BuildJob) -> Vec<String> {
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
        if job.shell_escape {
            // Unlike Tectonic's, the commands run in the project folder: latexmk has no separate
            // working folder for them. Where minted then puts its cache is unverified (ledger).
            args.push("-shell-escape".to_string());
        }
        args.push(job.root_file.to_string_lossy().into_owned());
        args
    }
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
        tokio::fs::create_dir_all(&job.out_dir).await?;
        let started = Instant::now();
        let args = self.arguments(job);
        let (status, stderr) = process::run(&self.binary, &job.project_dir, &args, &cancel, progress).await?;
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

    #[test]
    fn the_program_flag_comes_first_and_the_root_file_last() {
        let args = Latexmk::at("latexmk", TexProgram::LuaLatex).arguments(&job(true));
        assert_eq!(args.first().map(String::as_str), Some("-lualatex"));
        assert_eq!(args.last().map(String::as_str), Some("thesis.tex"));
        assert!(args.contains(&"-halt-on-error".to_string()));
        assert!(args.contains(&"-synctex=1".to_string()));
        assert!(args.iter().any(|a| a.starts_with("-outdir=") && a.ends_with("build")));
    }

    #[test]
    fn synctex_is_only_requested_when_the_job_asks() {
        let args = Latexmk::at("latexmk", TexProgram::PdfLatex).arguments(&job(false));
        assert!(!args.iter().any(|a| a.starts_with("-synctex")));
    }

    #[test]
    fn shell_escape_is_on_only_when_the_job_says_so() {
        for program in [TexProgram::PdfLatex, TexProgram::XeLatex, TexProgram::LuaLatex] {
            let args = Latexmk::at("latexmk", program).arguments(&job(true));
            assert!(!args.iter().any(|a| a.contains("shell-escape") || a == "-shell-restricted"), "{args:?}");
            let allowed = Latexmk::at("latexmk", program).arguments(&BuildJob { shell_escape: true, ..job(true) });
            assert!(allowed.contains(&"-shell-escape".to_string()), "{allowed:?}");
            assert_eq!(allowed.last().map(String::as_str), Some("thesis.tex"));
        }
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
