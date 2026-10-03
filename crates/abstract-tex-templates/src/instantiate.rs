//! Writing a template out as a new project folder.
//!
//! This module owns the one write the crate makes. The rule it keeps: the destination either ends
//! up complete or is left exactly as it was found. It must never overwrite anything, and never
//! copy the manifest or the preview.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::render::fill;
use crate::{Template, TemplateError};

impl Template {
    /// Write this template into `destination` as a new project, with `fields` filled in.
    ///
    /// `destination` must not exist yet, or be an empty folder. Every field the template declares
    /// must be in `fields`, and nothing else may be. Everything is rendered in memory first, so a
    /// bad field fails before a byte is written; the files are then written into a hidden
    /// folder beside the destination and renamed into place, so a failure part-way leaves no
    /// half-built project behind (the temp-and-rename rule of SPRINTS.md §4, applied to a folder).
    pub fn instantiate(
        &self,
        destination: &Path,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), TemplateError> {
        for declared in &self.fields {
            if !fields.contains_key(&declared.id) {
                return Err(TemplateError::MissingField(declared.id.clone()));
            }
        }
        for given in fields.keys() {
            if !self.fields.iter().any(|declared| &declared.id == given) {
                return Err(TemplateError::UnknownField(given.clone()));
            }
        }
        let rendered = self.render_files(fields)?;

        if destination.exists() {
            // `read_dir` on a file is an error and on an empty folder yields nothing; either
            // way, only "an empty folder" passes.
            let is_empty_folder = fs::read_dir(destination)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if !is_empty_folder {
                return Err(TemplateError::DestinationNotEmpty(destination.to_path_buf()));
            }
        }

        // A bare name like `essay` has the parent "", which means "the current folder".
        let parent = match destination.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        fs::create_dir_all(parent)?;
        // Dropping a `TempDir` deletes it, so every early `?` below cleans the staging folder up.
        let staging = tempfile::Builder::new()
            .prefix(".abstract-tex-template-")
            .tempdir_in(parent)?;
        for (path, bytes) in &rendered {
            let target = staging.path().join(path);
            if let Some(folder) = target.parent() {
                fs::create_dir_all(folder)?;
            }
            fs::write(target, bytes)?;
        }

        // `keep` stops the automatic deletion: from here the folder is the project.
        let staged = staging.keep();
        let moved = fs::remove_dir(destination)
            .or_else(ignore_not_found)
            .and_then(|()| fs::rename(&staged, destination));
        if let Err(error) = moved {
            let _ = fs::remove_dir_all(&staged);
            return Err(error.into());
        }
        Ok(())
    }

    /// Every file's path and bytes with the placeholders filled. A file that is not valid UTF-8
    /// (an image) is passed through untouched.
    pub(crate) fn render_files(
        &self,
        fields: &BTreeMap<String, String>,
    ) -> Result<Vec<(String, Vec<u8>)>, TemplateError> {
        let mut rendered = Vec::new();
        for (path, bytes) in &self.files {
            let filled = match std::str::from_utf8(bytes) {
                Ok(text) => fill(text, fields)
                    .map_err(|name| TemplateError::UnknownPlaceholder {
                        id: self.id.clone(),
                        file: path.clone(),
                        name,
                    })?
                    .into_bytes(),
                Err(_) => bytes.clone(),
            };
            rendered.push((path.clone(), filled));
        }
        Ok(rendered)
    }

    /// Each field mapped to its example answer.
    pub(crate) fn example_values(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|field| (field.id.clone(), field.example.clone()))
            .collect()
    }
}

/// `remove_dir` on a destination that was never created is not a failure.
fn ignore_not_found(error: std::io::Error) -> std::io::Result<()> {
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Catalog, RawTemplate};

    fn catalog() -> Catalog {
        let manifest = r#"
name = "Test"
category = "essay"
description = "A test."
licence = "CC0-1.0"
source = "Written for Abstract-Tex"
files = ["main.tex", "sections/intro.tex"]
preview = "preview.png"
[[fields]]
id = "title"
label = "Title"
example = "T"
[[fields]]
id = "author"
label = "Author"
example = "A"
"#;
        let mut files = BTreeMap::new();
        files.insert("template.toml".to_string(), manifest.as_bytes().to_vec());
        files.insert(
            "main.tex".to_string(),
            br"\title{{{title}}}\author{{{author}}}\input{sections/intro}".to_vec(),
        );
        files.insert("sections/intro.tex".to_string(), b"By {{author}}.".to_vec());
        files.insert("preview.png".to_string(), vec![0x89, 0x50]);
        Catalog::from_raw(vec![RawTemplate {
            id: "test".to_string(),
            files,
        }])
        .unwrap()
    }

    fn answers(title: &str, author: &str) -> BTreeMap<String, String> {
        [("title", title), ("author", author)]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// The names directly inside `folder`, sorted.
    fn listing(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_project_is_written_with_its_fields_filled_and_nothing_else() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("My R&D");
        catalog()
            .instantiate("test", &project, &answers("R&D", "Ada"))
            .unwrap();

        assert_eq!(
            fs::read_to_string(project.join("main.tex")).unwrap(),
            r"\title{R\&D}\author{Ada}\input{sections/intro}"
        );
        assert_eq!(
            fs::read_to_string(project.join("sections/intro.tex")).unwrap(),
            "By Ada."
        );
        // No manifest, no preview, no staging folder left in the parent.
        assert_eq!(listing(&project), ["main.tex", "sections"]);
        assert_eq!(listing(parent.path()), ["My R&D"]);
    }

    #[test]
    fn an_empty_existing_folder_is_filled() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("empty");
        fs::create_dir(&project).unwrap();
        catalog()
            .instantiate("test", &project, &answers("T", "A"))
            .unwrap();
        assert_eq!(listing(&project), ["main.tex", "sections"]);
    }

    #[test]
    fn a_folder_with_something_in_it_is_refused_and_left_alone() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("busy");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("thesis.tex"), "mine").unwrap();

        let result = catalog().instantiate("test", &project, &answers("T", "A"));
        assert!(matches!(result, Err(TemplateError::DestinationNotEmpty(_))));
        assert_eq!(listing(&project), ["thesis.tex"]);
        assert_eq!(listing(parent.path()), ["busy"]);
    }

    #[test]
    fn a_missing_or_unknown_field_writes_nothing() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("new");

        let mut only_title = answers("T", "A");
        only_title.remove("author");
        assert!(
            matches!(catalog().instantiate("test", &project, &only_title), Err(TemplateError::MissingField(name)) if name == "author")
        );

        let mut extra = answers("T", "A");
        extra.insert("date".to_string(), "today".to_string());
        assert!(
            matches!(catalog().instantiate("test", &project, &extra), Err(TemplateError::UnknownField(name)) if name == "date")
        );

        assert!(listing(parent.path()).is_empty());
    }

    #[test]
    fn an_unknown_template_is_named() {
        let parent = tempfile::tempdir().unwrap();
        let result = catalog().instantiate("nope", &parent.path().join("x"), &answers("T", "A"));
        assert!(matches!(result, Err(TemplateError::NoSuchTemplate(name)) if name == "nope"));
    }
}
