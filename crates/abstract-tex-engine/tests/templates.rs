//! S11.10a: every starter template, instantiated the way the app will and built for real.
//!
//! Ignored by default like every other real-engine test here: it needs the fetched Tectonic
//! sidecar and, on a cold cache, the network. `cargo test -p abstract-tex-engine --test templates
//! -- --ignored` runs it.
//!
//! "Compiles" means more than a PDF: the log, run through the same rule catalog the app uses,
//! must produce no diagnostic at all. A starter whose first build shows a warning card breaks
//! DESIGN.md §2's *zero setup to first PDF* for the person who has not written a word yet.
//!
//! The PDFs are also copied to `target/template-pdfs/<id>.pdf`, which is what
//! `scripts/template-previews.mjs` turns into each template's `preview.png`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use abstract_tex_engine::tectonic::Tectonic;
use abstract_tex_engine::{BuildJob, Engine};
use abstract_tex_templates::Catalog;
use tokio_util::sync::CancellationToken;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[tokio::test]
#[ignore]
async fn every_template_builds_clean_from_its_instantiated_folder() {
    let engine =
        Tectonic::at(abstract_tex_sidecar::in_repo_binaries("tectonic", &repo_root()).expect("run `pnpm fetch-engine` first"));
    let catalog = Catalog::embedded().unwrap();
    let pdf_folder = repo_root().join("target/template-pdfs");
    fs::create_dir_all(&pdf_folder).unwrap();

    for template in catalog.templates() {
        // The answers a person would give, as far as the manifest knows them.
        let answers: BTreeMap<String, String> =
            template.fields.iter().map(|field| (field.id.clone(), field.example.clone())).collect();
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("project");
        catalog.instantiate(&template.id, &project, &answers).unwrap();

        let job = BuildJob {
            project_dir: project.clone(),
            root_file: PathBuf::from("main.tex"),
            out_dir: project.join(".abstract-tex/build"),
            synctex: true,
            shell_escape: false,
        };
        let outcome = engine.build(&job, CancellationToken::new(), None).await.unwrap();
        assert!(outcome.success, "template `{}` failed to build:\n{}", template.id, outcome.stderr);

        let log = outcome.log.as_ref().and_then(|path| fs::read_to_string(path).ok()).unwrap_or_default();
        let diagnostics = texlog::diagnostics(&log);
        assert!(
            diagnostics.is_empty(),
            "template `{}` builds with diagnostics: {:?}",
            template.id,
            diagnostics.iter().map(|d| (&d.rule, &d.file, d.line)).collect::<Vec<_>>()
        );

        fs::copy(project.join(".abstract-tex/build/main.pdf"), pdf_folder.join(format!("{}.pdf", template.id)))
            .unwrap_or_else(|e| panic!("template `{}` built but left no main.pdf: {e}", template.id));
    }
}
