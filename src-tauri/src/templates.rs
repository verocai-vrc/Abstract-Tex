//! The New project window's side of the starter templates (S11.11; DESIGN.md §6, §10).
//!
//! The crate (`abstract-tex-templates`) owns the catalog and the write; this module turns the
//! catalog into what the window can draw and a folder choice into a sentence when it cannot be
//! used.
//!
//! **What it must never do:**
//!
//! - Never write into a folder that already has something in it. A taken name is a sentence the
//!   author can act on ("pick another title"), not a failure to report.
//! - Never open the new project itself. It answers with the folder; the window opens it as it
//!   opens any other, so a template project and an opened one go through the same code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use abstract_tex_templates::{Catalog, Category, Field, Template, TemplateError};
// `as _` brings the trait's methods (`encode`) into scope without giving the trait a name here,
// which is all a trait import is for when nothing refers to the trait itself.
use base64::Engine as _;
use serde::Serialize;

/// One template as the window draws it: a card (preview, name, one line) and the questions the
/// form asks once it is chosen.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    /// `essay`, `cv`, …: the filter's vocabulary, spelled as the manifest spells it.
    pub category: String,
    pub description: String,
    /// The preview as a `data:` URL, so the picker needs no file path or protocol to show it.
    pub preview_url: String,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldInfo {
    pub id: String,
    pub label: String,
    /// Shown as the input's hint.
    pub example: String,
}

fn category_name(category: Category) -> &'static str {
    match category {
        Category::Essay => "essay",
        Category::Report => "report",
        Category::Paper => "paper",
        Category::Letter => "letter",
        Category::Cv => "cv",
        Category::Thesis => "thesis",
        Category::Slides => "slides",
        Category::Other => "other",
    }
}

fn describe(template: &Template) -> TemplateInfo {
    let encoded = base64::engine::general_purpose::STANDARD.encode(&template.preview);
    TemplateInfo {
        id: template.id.clone(),
        name: template.name.clone(),
        category: category_name(template.category).to_string(),
        description: template.description.clone(),
        preview_url: format!("data:image/png;base64,{encoded}"),
        fields: template
            .fields
            .iter()
            .map(|Field { id, label, example }| FieldInfo {
                id: id.clone(),
                label: label.clone(),
                example: example.clone(),
            })
            .collect(),
    }
}

/// Every template in the catalog, in a stable order: the order the catalog lists them in.
pub fn list() -> Result<Vec<TemplateInfo>, String> {
    let catalog = Catalog::embedded().map_err(|error| error.to_string())?;
    Ok(catalog.templates().iter().map(describe).collect())
}

/// Where a new project from `fields["title"]` goes inside `parent`, or why it cannot go there.
///
/// The name comes from the title; if that folder is already taken the answer says so by name, and
/// the author changes the title. An existing *empty* folder is fine: the crate writes into it.
pub fn destination_for(
    parent: &Path,
    fields: &BTreeMap<String, String>,
    template_id: &str,
) -> Result<PathBuf, String> {
    // A template with no title field (none ships today) is named after itself.
    let source = fields.get("title").map(String::as_str).unwrap_or(template_id);
    let name = abstract_tex_templates::folder_name_for_title(source);
    let destination = parent.join(&name);
    let is_empty_folder = std::fs::read_dir(&destination)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    if destination.exists() && !is_empty_folder {
        return Err(format!(
            "There is already something called \"{name}\" in {}. Change the title to give this project another folder name, or choose a different place.",
            parent.display()
        ));
    }
    Ok(destination)
}

/// Make a new project from template `template_id` inside `parent`, with `fields` filled in, and
/// answer with the folder.
pub fn create(
    parent: &Path,
    template_id: &str,
    fields: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let catalog = Catalog::embedded().map_err(|error| error.to_string())?;
    let destination = destination_for(parent, fields, template_id)?;
    catalog
        .instantiate(template_id, &destination, fields)
        .map_err(|error| match error {
            // Raced with something else creating the folder since `destination_for` looked.
            TemplateError::DestinationNotEmpty(path) => {
                format!(
                    "{} already has something in it, so nothing was written there.",
                    path.display()
                )
            }
            other => other.to_string(),
        })?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(title: &str, author: &str) -> BTreeMap<String, String> {
        [("title", title), ("author", author)]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_list_has_every_template_with_a_preview_the_webview_can_show() {
        let templates = list().unwrap();
        let ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        for expected in [
            "blank", "essay", "report", "letter", "paper", "cv", "thesis", "slides",
        ] {
            assert!(ids.contains(&expected), "{expected} is missing from {ids:?}");
        }
        let cv = templates.iter().find(|t| t.id == "cv").unwrap();
        assert_eq!(cv.category, "cv");
        assert!(cv.preview_url.starts_with("data:image/png;base64,"));
        assert_eq!(
            cv.fields.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["title", "author"]
        );
    }

    #[test]
    fn a_project_lands_in_a_folder_named_after_its_title() {
        let parent = tempfile::tempdir().unwrap();
        let folder = create(parent.path(), "essay", &answers("My Essay: v2", "Ada")).unwrap();
        assert_eq!(folder, parent.path().join("my-essay-v2"));
        let main = std::fs::read_to_string(folder.join("main.tex")).unwrap();
        assert!(main.contains(r"\title{My Essay: v2}") && main.contains(r"\author{Ada}"));
    }

    #[test]
    fn a_taken_name_is_a_sentence_and_nothing_is_touched() {
        let parent = tempfile::tempdir().unwrap();
        let taken = parent.path().join("my-essay");
        std::fs::create_dir(&taken).unwrap();
        std::fs::write(taken.join("notes.txt"), "mine").unwrap();

        let message = create(parent.path(), "essay", &answers("My essay", "Ada")).unwrap_err();

        assert!(
            message.contains("\"my-essay\"") && message.contains("Change the title"),
            "{message}"
        );
        let left: Vec<_> = std::fs::read_dir(&taken)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["notes.txt"]);
        // And no staging folder is left beside it.
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 1);
    }

    #[test]
    fn an_existing_empty_folder_is_used() {
        let parent = tempfile::tempdir().unwrap();
        std::fs::create_dir(parent.path().join("my-essay")).unwrap();
        assert!(create(parent.path(), "essay", &answers("My essay", "Ada")).is_ok());
    }

    #[test]
    fn an_unknown_template_is_a_sentence() {
        let parent = tempfile::tempdir().unwrap();
        let message = create(parent.path(), "nope", &answers("T", "A")).unwrap_err();
        assert!(message.contains("no template called `nope`"), "{message}");
    }
}
