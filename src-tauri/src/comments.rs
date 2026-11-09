//! The app's side of comments (S14.3b): turns what the editor knows (a file, a selection in UTF-16
//! units, the buffer's text) into calls on `abstract-tex-comments`, and turns what that crate
//! returns into rows the frontend can draw.
//!
//! **What it must never do:** never write to the working tree, the index or any branch — comments
//! live only on `refs/abstract-tex/comments`; never invent a Git repository for a folder that has
//! none (a comment needs somewhere to be kept, and creating `.git` is Sync's decision, not ours);
//! and never hand the frontend a byte offset — CodeMirror counts UTF-16 units, so every position
//! leaving this module is converted to them.

use std::path::Path;

use abstract_tex_comments::{
    add, author_name, load, record_resolution, AnchorStatus, CommentRecord, OrphanReason, ResolutionEvent,
};
use serde::Serialize;

/// What went wrong, in a sentence an author can read.
#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    #[error("Comments are kept in the project's Git history, and this folder is not a Git repository yet.")]
    NotARepository,
    #[error("Select some text first; a comment has to be about something.")]
    EmptySelection,
    #[error("{0}")]
    Store(#[from] abstract_tex_comments::CommentsError),
}

/// One comment as the comments panel draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentRow {
    pub id: String,
    pub file: String,
    pub author: String,
    pub body: String,
    /// The words the comment was attached to, so an orphaned comment can still say what it was about.
    pub quote: String,
    pub created: i64,
    pub resolved: bool,
    /// `anchored`, `orphaned` (the text was rewritten or is now ambiguous) or `fileMissing`.
    pub state: &'static str,
    /// UTF-16 offsets into the file as it is on disk; `None` unless `state` is `anchored`.
    pub from: Option<usize>,
    pub to: Option<usize>,
}

fn open(root: &Path) -> Result<abstract_tex_comments::Repository, CommentError> {
    abstract_tex_comments::open(root).map_err(|error| match error {
        abstract_tex_comments::CommentsError::NotARepository => CommentError::NotARepository,
        other => CommentError::Store(other),
    })
}

/// UTF-16 units before byte offset `byte` of `text`.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// The byte offset of the `unit`th UTF-16 unit of `text`, if it falls on a character boundary.
fn byte_offset(text: &str, unit: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units == unit {
            return Some(byte);
        }
        units += character.len_utf16();
    }
    (units == unit).then_some(text.len())
}

/// Every comment in the project at `root`, reanchored against the files as they are on disk.
pub fn list(root: &Path) -> Result<Vec<CommentRow>, CommentError> {
    let repository = open(root)?;
    // Each file is read from disk once per comment on it; comment counts are small and the
    // alternative is a cache that can go stale when the watcher fires.
    let read_file = |relative: &str| std::fs::read_to_string(root.join(relative)).ok();
    let mut rows: Vec<CommentRow> = load(&repository, read_file)?
        .into_iter()
        .map(|loaded| {
            let (state, from, to) = match loaded.status {
                AnchorStatus::Anchored { start, end } => {
                    let text = read_file(&loaded.record.file).unwrap_or_default();
                    (
                        "anchored",
                        Some(utf16_offset(&text, start)),
                        Some(utf16_offset(&text, end)),
                    )
                }
                AnchorStatus::Orphaned(OrphanReason::TextNotFound | OrphanReason::Ambiguous { .. }) => {
                    ("orphaned", None, None)
                }
                AnchorStatus::FileMissing => ("fileMissing", None, None),
            };
            CommentRow {
                id: loaded.id,
                file: loaded.record.file,
                author: loaded.record.author,
                body: loaded.record.body,
                quote: loaded.record.anchor.quote,
                created: loaded.record.created,
                resolved: loaded.resolved,
                state,
                from,
                to,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
    Ok(rows)
}

/// Add a comment on `from..to` (UTF-16 units) of `text`, the buffer as the author sees it.
pub fn add_comment(
    root: &Path,
    file: &str,
    text: &str,
    from: usize,
    to: usize,
    body: &str,
    now: i64,
) -> Result<String, CommentError> {
    let repository = open(root)?;
    let (Some(start), Some(end)) = (byte_offset(text, from), byte_offset(text, to)) else {
        return Err(CommentError::EmptySelection);
    };
    let anchor =
        abstract_tex_comments::Anchor::capture(text, start, end).ok_or(CommentError::EmptySelection)?;
    let author = author_name(&repository);
    let record = CommentRecord {
        file: file.to_string(),
        anchor,
        author,
        body: body.to_string(),
        created: now,
    };
    Ok(add(&repository, &record)?)
}

/// Resolve or reopen one comment.
pub fn set_resolved(root: &Path, comment: &str, resolved: bool, now: i64) -> Result<(), CommentError> {
    let repository = open(root)?;
    let author = author_name(&repository);
    record_resolution(
        &repository,
        &ResolutionEvent {
            comment: comment.to_string(),
            resolved,
            author,
            created: now,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(text: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("main.tex"), text).unwrap();
        tmp
    }

    #[test]
    fn a_folder_that_is_not_a_repository_says_so_instead_of_creating_one() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(list(tmp.path()), Err(CommentError::NotARepository)));
        assert!(!tmp.path().join(".git").exists());
    }

    #[test]
    fn positions_are_utf16_units_even_after_multi_byte_text() {
        // 𝑥 is one char, four bytes in UTF-8 and two UTF-16 units.
        let text = "𝑥 the result stands";
        let tmp = project_with(text);
        let from = "𝑥 the ".encode_utf16().count();
        let to = from + "result".len();
        let id = add_comment(tmp.path(), "main.tex", text, from, to, "source?", 5).unwrap();

        let rows = list(tmp.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].state, "anchored");
        assert_eq!((rows[0].from, rows[0].to), (Some(from), Some(to)));
        assert_eq!(rows[0].quote, "result");
    }

    #[test]
    fn a_selection_inside_a_surrogate_pair_or_empty_is_refused() {
        let text = "𝑥 y";
        let tmp = project_with(text);
        assert!(add_comment(tmp.path(), "main.tex", text, 1, 3, "x", 0).is_err());
        assert!(add_comment(tmp.path(), "main.tex", text, 2, 2, "x", 0).is_err());
    }

    #[test]
    fn resolving_shows_in_the_list_and_a_rewrite_turns_a_comment_orphaned() {
        let text = "the result stands";
        let tmp = project_with(text);
        let id = add_comment(tmp.path(), "main.tex", text, 4, 10, "?", 1).unwrap();
        set_resolved(tmp.path(), &id, true, 2).unwrap();
        assert!(list(tmp.path()).unwrap()[0].resolved);

        std::fs::write(tmp.path().join("main.tex"), "something else entirely").unwrap();
        let row = &list(tmp.path()).unwrap()[0];
        assert_eq!((row.state, row.from), ("orphaned", None));
    }
}
