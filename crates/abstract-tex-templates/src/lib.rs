//! The starter templates bundled into the app, and turning one into a new project folder
//! (S11.9; DESIGN.md §6 "Start a document", §10 "Templates").
//!
//! This crate owns the catalog: reading `templates/<id>/` from the binary, checking every entry
//! against the rules in DESIGN.md §10 when it loads, and writing one out as a new project.
//!
//! **What it must never do:**
//!
//! - Never touch the network. The catalog is compiled in (`include_dir!`), so the picker works
//!   offline; "stealing from the web" is something a person does while building this repository.
//! - Never accept a template whose licence is not in [`ALLOWED_LICENCES`]. An author's thesis must
//!   not inherit a licence from the template it began as, so a bad entry is a load error, not a
//!   warning.
//! - Never write anything into the author's folder but the template's own listed files. The
//!   manifest and the preview stay in the binary (SPRINTS.md §5: plain `.tex`/`.bib` only).
//! - Never know about Tauri or windows. The app edge calls this; this crate does not call it.

mod instantiate;
mod render;

use std::collections::BTreeMap;
use std::path::{Component, Path};

use include_dir::{include_dir, Dir};
use serde::Deserialize;

/// The licences a template may carry (SPDX identifiers). MIT, BSD, CC0 and the Unlicense put no
/// condition on what an author does with their own document; LPPL governs *changing the class
/// files*, not documents written with them. Anything copyleft, share-alike or "personal use
/// only" is deliberately absent. Adding to this list is a design decision, not a code change:
/// edit DESIGN.md §10 first.
pub const ALLOWED_LICENCES: &[&str] =
    &["MIT", "BSD-2-Clause", "BSD-3-Clause", "CC0-1.0", "Unlicense", "LPPL-1.3c"];

/// Every template, as shipped: one folder per template under `templates/` at the repository
/// root. `include_dir!` reads them at compile time, so the folder does not exist for the
/// running app, and a `static` is the only way to hold what it produced.
static EMBEDDED_TEMPLATES: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

const MANIFEST_NAME: &str = "template.toml";

/// What kind of document a template is, for the picker's filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
// `rename_all` makes `Category::Cv` read as `category = "cv"` in a manifest.
#[serde(rename_all = "lowercase")]
pub enum Category {
    Essay,
    Report,
    Paper,
    Letter,
    Cv,
    Thesis,
    Slides,
    Other,
}

/// One thing the author is asked for when starting from a template, such as the title.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// The name used as `{{id}}` in the template's files.
    pub id: String,
    /// What the form calls it: "Title".
    pub label: String,
    /// A sample answer, shown as a hint and used to dry-run the template when it loads.
    pub example: String,
}

/// What `template.toml` says. Private: callers see [`Template`], which has been checked.
// `deny_unknown_fields` turns a misspelt key into an error instead of a silently ignored one.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    category: Category,
    description: String,
    licence: String,
    /// Where it came from: a URL, or "Written for Abstract-Tex". Recorded, never parsed.
    source: String,
    /// Every file that goes into the new project, relative to the template's folder.
    files: Vec<String>,
    /// The picture the picker shows. Stays in the binary.
    preview: String,
    #[serde(default)]
    fields: Vec<Field>,
}

/// A template that has passed every check in [`Catalog::from_raw`].
#[derive(Debug, Clone)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub description: String,
    pub licence: String,
    pub source: String,
    pub fields: Vec<Field>,
    /// The preview image's bytes (PNG).
    pub preview: Vec<u8>,
    /// What gets written: relative path and bytes, in the manifest's order.
    files: Vec<(String, Vec<u8>)>,
}

/// A template folder as plain data: its id and every file in it, by relative path (always `/`
/// separated). The shipped catalog and the tests both build these, and [`Catalog::from_raw`]
/// checks them the same way, so a test can hand it a broken template without touching a disk.
#[derive(Debug, Clone)]
pub struct RawTemplate {
    pub id: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("template `{id}` has no {MANIFEST_NAME}")]
    MissingManifest { id: String },
    #[error("template `{id}`: {MANIFEST_NAME} is not valid: {message}")]
    BadManifest { id: String, message: String },
    #[error("template `{id}` is under the licence `{licence}`, which the catalog does not allow")]
    LicenceNotAllowed { id: String, licence: String },
    #[error("template `{id}`: `{field}` is not a usable field name (lowercase letters, digits and underscores, starting with a letter, and used once)")]
    BadFieldId { id: String, field: String },
    #[error("template `{id}`: `{path}` is not a safe relative path")]
    UnsafePath { id: String, path: String },
    #[error("template `{id}` lists `{path}`, but it is not in the folder")]
    ListedFileMissing { id: String, path: String },
    #[error("template `{id}` contains `{path}`, which its {MANIFEST_NAME} does not list")]
    UnlistedFile { id: String, path: String },
    #[error("template `{id}` has no preview image at `{path}`")]
    PreviewMissing { id: String, path: String },
    #[error("template `{id}`: `{file}` has a placeholder `{name}` that is not a declared field")]
    UnknownPlaceholder { id: String, file: String, name: String },
    #[error("there is no template called `{0}`")]
    NoSuchTemplate(String),
    #[error("the field `{0}` was not given")]
    MissingField(String),
    #[error("`{0}` is not a field of this template")]
    UnknownField(String),
    #[error("`{}` already has something in it", .0.display())]
    DestinationNotEmpty(std::path::PathBuf),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// The checked set of templates.
#[derive(Debug, Clone)]
pub struct Catalog {
    templates: Vec<Template>,
}

impl Catalog {
    /// The templates compiled into this binary.
    pub fn embedded() -> Result<Catalog, TemplateError> {
        let raw = EMBEDDED_TEMPLATES
            .dirs()
            .map(|folder| {
                let id = folder.path().to_string_lossy().replace('\\', "/");
                let mut files = BTreeMap::new();
                collect_files(folder, &id, &mut files);
                RawTemplate { id, files }
            })
            .collect();
        Catalog::from_raw(raw)
    }

    /// Check each raw template and keep the ones that pass; the first that fails is the error.
    pub fn from_raw(raw: Vec<RawTemplate>) -> Result<Catalog, TemplateError> {
        let templates = raw.into_iter().map(check_template).collect::<Result<Vec<_>, _>>()?;
        Ok(Catalog { templates })
    }

    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    pub fn get(&self, id: &str) -> Option<&Template> {
        self.templates.iter().find(|template| template.id == id)
    }

    /// Write template `id` into `destination` as a new project, with `fields` filled in.
    /// See [`Template::instantiate`] for what it promises.
    pub fn instantiate(
        &self,
        id: &str,
        destination: &Path,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), TemplateError> {
        self.get(id)
            .ok_or_else(|| TemplateError::NoSuchTemplate(id.to_string()))?
            .instantiate(destination, fields)
    }
}

/// The name of the folder a new project goes in, taken from what the author called it: "My
/// thesis: draft 2" becomes `my-thesis-draft-2`.
///
/// Letters and digits (of any script) are kept, lowercased; every run of anything else becomes one
/// hyphen. That is safe on every filesystem this app runs on, and easy to type in a terminal. A
/// title with no letters or digits at all falls back to `new-project`, so the answer is always a
/// usable name and never empty.
pub fn folder_name_for_title(title: &str) -> String {
    let mut name = String::new();
    for character in title.chars() {
        if character.is_alphanumeric() {
            name.extend(character.to_lowercase());
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_end_matches('-');
    // Sixty characters is plenty of title; cutting by `chars` never splits a character.
    let name: String = name.chars().take(60).collect();
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        "new-project".to_string()
    } else {
        name.to_string()
    }
}

/// Add every file under `folder` to `files`, keyed by its path relative to the template's own
/// folder. `include_dir` reports paths from the root of `templates/`, so the id is cut off.
fn collect_files(folder: &Dir, id: &str, files: &mut BTreeMap<String, Vec<u8>>) {
    for file in folder.files() {
        let path = file.path().to_string_lossy().replace('\\', "/");
        let relative = path.strip_prefix(&format!("{id}/")).unwrap_or(&path).to_string();
        files.insert(relative, file.contents().to_vec());
    }
    for subfolder in folder.dirs() {
        collect_files(subfolder, id, files);
    }
}

/// Parse and check one raw template. Every rule in DESIGN.md §10 "Templates" that a machine can
/// check is checked here, so the shipped catalog cannot hold a template that breaks it.
fn check_template(raw: RawTemplate) -> Result<Template, TemplateError> {
    let id = raw.id;
    let manifest_bytes = raw.files.get(MANIFEST_NAME).ok_or_else(|| TemplateError::MissingManifest { id: id.clone() })?;
    let bad_manifest = |message: String| TemplateError::BadManifest { id: id.clone(), message };
    let manifest_text = std::str::from_utf8(manifest_bytes).map_err(|e| bad_manifest(e.to_string()))?;
    let manifest: Manifest = toml::from_str(manifest_text).map_err(|e| bad_manifest(e.to_string()))?;

    if !ALLOWED_LICENCES.contains(&manifest.licence.as_str()) {
        return Err(TemplateError::LicenceNotAllowed { id, licence: manifest.licence });
    }

    let mut seen_fields = Vec::new();
    for field in &manifest.fields {
        if !render::is_valid_field_id(&field.id) || seen_fields.contains(&field.id) {
            return Err(TemplateError::BadFieldId { id, field: field.id.clone() });
        }
        seen_fields.push(field.id.clone());
    }

    for path in manifest.files.iter().chain(std::iter::once(&manifest.preview)) {
        if !is_safe_relative_path(path) || path == MANIFEST_NAME {
            return Err(TemplateError::UnsafePath { id, path: path.clone() });
        }
    }
    let preview = raw.files.get(&manifest.preview).ok_or_else(|| TemplateError::PreviewMissing { id: id.clone(), path: manifest.preview.clone() })?;
    let mut files = Vec::new();
    for path in &manifest.files {
        let bytes = raw.files.get(path).ok_or_else(|| TemplateError::ListedFileMissing { id: id.clone(), path: path.clone() })?;
        files.push((path.clone(), bytes.clone()));
    }
    for path in raw.files.keys() {
        let known = path == MANIFEST_NAME || *path == manifest.preview || manifest.files.contains(path);
        if !known {
            return Err(TemplateError::UnlistedFile { id, path: path.clone() });
        }
    }

    let template = Template {
        id,
        name: manifest.name,
        category: manifest.category,
        description: manifest.description,
        licence: manifest.licence,
        source: manifest.source,
        fields: manifest.fields,
        preview: preview.clone(),
        files,
    };
    // A dry run with each field's example proves every placeholder in every file is declared,
    // now, rather than the first time an author picks this template.
    template.render_files(&template.example_values())?;
    Ok(template)
}

/// A relative path that stays inside the folder it is joined to: no `..`, no root, no drive.
fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path).components().all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_MANIFEST: &str = r#"
name = "Test"
category = "essay"
description = "A test."
licence = "MIT"
source = "Written for Abstract-Tex"
files = ["main.tex"]
preview = "preview.png"
[[fields]]
id = "title"
label = "Title"
example = "T"
"#;

    /// A passing raw template with `main.tex` containing `body`; tests change one thing at a time.
    fn raw(body: &str) -> RawTemplate {
        let mut files = BTreeMap::new();
        files.insert(MANIFEST_NAME.to_string(), GOOD_MANIFEST.as_bytes().to_vec());
        files.insert("main.tex".to_string(), body.as_bytes().to_vec());
        files.insert("preview.png".to_string(), vec![1, 2, 3]);
        RawTemplate { id: "test".to_string(), files }
    }

    fn load(raw: RawTemplate) -> Result<Catalog, TemplateError> {
        Catalog::from_raw(vec![raw])
    }

    #[test]
    fn a_good_template_loads() {
        let catalog = load(raw(r"\title{{{title}}}")).unwrap();
        let template = catalog.get("test").unwrap();
        assert_eq!((template.name.as_str(), template.category, template.fields.len()), ("Test", Category::Essay, 1));
    }

    #[test]
    fn a_licence_outside_the_list_is_refused() {
        let mut template = raw("x");
        let manifest = GOOD_MANIFEST.replace(r#"licence = "MIT""#, r#"licence = "GPL-3.0-only""#);
        template.files.insert(MANIFEST_NAME.to_string(), manifest.into_bytes());
        assert!(matches!(load(template), Err(TemplateError::LicenceNotAllowed { .. })));
    }

    #[test]
    fn a_missing_licence_is_a_bad_manifest() {
        let mut template = raw("x");
        let manifest = GOOD_MANIFEST.replace(r#"licence = "MIT""#, "");
        template.files.insert(MANIFEST_NAME.to_string(), manifest.into_bytes());
        assert!(matches!(load(template), Err(TemplateError::BadManifest { .. })));
    }

    #[test]
    fn an_absent_preview_a_missing_file_and_an_unlisted_file_are_each_refused() {
        let mut no_preview = raw("x");
        no_preview.files.remove("preview.png");
        assert!(matches!(load(no_preview), Err(TemplateError::PreviewMissing { .. })));

        let mut no_main = raw("x");
        no_main.files.remove("main.tex");
        assert!(matches!(load(no_main), Err(TemplateError::ListedFileMissing { .. })));

        let mut extra = raw("x");
        extra.files.insert("notes.txt".to_string(), vec![]);
        assert!(matches!(load(extra), Err(TemplateError::UnlistedFile { .. })));
    }

    #[test]
    fn an_undeclared_placeholder_is_refused_when_the_catalog_loads() {
        assert!(matches!(load(raw("{{author}}")), Err(TemplateError::UnknownPlaceholder { .. })));
    }

    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        let mut template = raw("x");
        let manifest = GOOD_MANIFEST.replace(r#"["main.tex"]"#, r#"["../main.tex"]"#);
        template.files.insert(MANIFEST_NAME.to_string(), manifest.into_bytes());
        assert!(matches!(load(template), Err(TemplateError::UnsafePath { .. })));
    }

    /// The walk the card asks for: whatever is in `templates/` right now passes every check above.
    #[test]
    fn every_shipped_template_passes_every_check() {
        let catalog = Catalog::embedded().expect("a template in templates/ breaks a catalog rule");
        assert!(!catalog.templates().is_empty());
    }

    #[test]
    fn a_title_becomes_one_plain_folder_name() {
        assert_eq!(folder_name_for_title("My thesis: draft 2"), "my-thesis-draft-2");
        assert_eq!(folder_name_for_title("  Rate limiting -- without a coordinator!  "), "rate-limiting-without-a-coordinator");
        assert_eq!(folder_name_for_title("Über die Elektrodynamik"), "über-die-elektrodynamik");
        assert_eq!(folder_name_for_title("论文"), "论文");
    }

    #[test]
    fn a_title_with_no_letters_gets_a_fallback_and_a_long_one_is_cut() {
        assert_eq!(folder_name_for_title(""), "new-project");
        assert_eq!(folder_name_for_title("?!.."), "new-project");
        assert_eq!(folder_name_for_title("../../etc"), "etc");
        let long = folder_name_for_title(&"word ".repeat(40));
        assert!(long.chars().count() <= 60 && !long.ends_with('-'));
    }

    /// S11.10a: a starter that does not say what to replace is a document the author has to read
    /// all of before they dare touch it. Every shipped template carries at least one marker in
    /// one of its source files (the `.bib` of `paper` does not count, and is not where it goes).
    #[test]
    fn every_shipped_template_says_what_to_fill_in() {
        let catalog = Catalog::embedded().unwrap();
        for template in catalog.templates() {
            let has_marker = template
                .files
                .iter()
                .filter(|(path, _)| path.ends_with(".tex"))
                .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("% FILL IN:"));
            assert!(has_marker, "template `{}` has no `% FILL IN:` marker in a .tex file", template.id);
        }
    }

    /// The first template that ships, end to end: the real files, the real manifest, a real folder.
    #[test]
    fn the_blank_template_makes_a_project() {
        let catalog = Catalog::embedded().unwrap();
        let answers: BTreeMap<String, String> =
            [("title", "My Thesis"), ("author", "Ada")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("my-thesis");

        catalog.instantiate("blank", &project, &answers).unwrap();

        let main = std::fs::read_to_string(project.join("main.tex")).unwrap();
        assert!(main.contains(r"\title{My Thesis}") && main.contains(r"\author{Ada}"));
        assert!(main.contains("% FILL IN:"), "a starter must say what to replace");
        assert!(!main.contains("{{"));
    }
}
