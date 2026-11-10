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
    add, author_name, fetch_refspec, has_comments, load, merge_from, push_refspec, resolve, AnchorStatus,
    CommentRecord, MergeOutcome, OrphanReason,
};
use serde::Serialize;

/// What went wrong, in a sentence an author can read.
#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    #[error("Comments are kept in the project's Git history, and this folder is not a Git repository yet.")]
    NotARepository,
    #[error("Select some text first; a comment has to be about something.")]
    EmptySelection,
    #[error("\"{0}\" is not a file inside this project, so a comment cannot be about it.")]
    OutsideProject(String),
    #[error("{0}")]
    Store(#[from] abstract_tex_comments::CommentsError),
    /// The remote could not be reached or refused: the git crate's own sentence.
    #[error("{0}")]
    Remote(#[from] abstract_tex_git::GitError),
    #[error(
        "Your comments and your coauthor's could not be combined automatically ({0}). Nothing was sent."
    )]
    Conflicted(String),
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

/// Whether `relative` is a project-relative path that stays inside the project: not empty, not
/// absolute, no drive prefix, and no `..` anywhere. Comments are stored with forward slashes, so
/// this reads the name the way a Git tree would.
fn is_inside_project(relative: &str) -> bool {
    use std::path::Component;
    !relative.is_empty()
        && Path::new(relative)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        && !relative.contains('\\')
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
    list_with(root, &|relative| {
        std::fs::read_to_string(root.join(relative)).ok()
    })
}

/// [`list`] with the file reader passed in, so a test can watch what is read and when.
fn list_with(
    root: &Path,
    read_file: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<CommentRow>, CommentError> {
    let repository = open(root)?;
    // Each file is read once per listing, and every comment on it is placed against that one copy.
    // Reading again to convert offsets meant the offsets (found in the first copy) were sliced out
    // of a second one that could have changed in between, which panics (review finding R3.7). A name
    // that leads outside the project is answered "no such file" without being read at all (R3.9).
    let cache: std::cell::RefCell<std::collections::HashMap<String, Option<String>>> = Default::default();
    let read_file = |relative: &str| -> Option<String> {
        cache
            .borrow_mut()
            .entry(relative.to_string())
            .or_insert_with(|| {
                if is_inside_project(relative) {
                    read_file(relative)
                } else {
                    None
                }
            })
            .clone()
    };
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
    if !is_inside_project(file) {
        return Err(CommentError::OutsideProject(file.to_string()));
    }
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
    resolve(&repository, comment, &author, resolved, now)?;
    Ok(())
}

/// What a comments sync did, for the sentence the app shows and for knowing whether to redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsSync {
    /// Comments from the remote were folded into ours: the list should be read again.
    pub received: bool,
    /// Our comments were sent to the remote.
    pub sent: bool,
}

/// Exchange comments with `origin`: fetch theirs, fold them into ours, send ours.
///
/// Run after a successful *Sync* so one click moves the text and the margin notes together. The
/// network part is `abstract_tex_git`'s (it holds the credentials and is the one crate allowed to
/// talk to a remote); the combining is `abstract_tex_comments`'s and happens strictly between
/// the fetch and the push, so what goes out already contains what came in.
///
/// Two quiet cases are not errors: a remote nobody has commented on (nothing to fetch), and a
/// project with no comments of its own yet (nothing to push).
pub fn sync(root: &Path, token: Option<&str>) -> Result<CommentsSync, CommentError> {
    let repository = open(root)?;

    abstract_tex_git::fetch_refspec(&repository, &fetch_refspec(abstract_tex_git::ORIGIN), token)?;
    let received = match merge_from(&repository, abstract_tex_git::ORIGIN)? {
        MergeOutcome::NothingToMerge => false,
        MergeOutcome::FastForwarded | MergeOutcome::Merged => true,
        MergeOutcome::Conflicted(paths) => return Err(CommentError::Conflicted(paths.join(", "))),
    };

    let sent = has_comments(&repository);
    if sent {
        abstract_tex_git::push_refspec(&repository, &push_refspec(), token)?;
    }
    Ok(CommentsSync { received, sent })
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
    // Review finding R3.7. `list` read each anchored file twice, once inside `load` and once more to
    // turn byte offsets into UTF-16 ones, and sliced the second copy at offsets found in the
    // first. A file that changed in between (an autosave, a branch switch) made `text[..byte]`
    // panic. It is now read once per listing.
    #[test]
    fn a_listing_reads_each_file_once_so_a_change_in_between_cannot_misplace_an_offset() {
        let text = "the result stands, and the result holds";
        let tmp = project_with(text);
        add_comment(tmp.path(), "main.tex", text, 4, 10, "first?", 1).unwrap();
        add_comment(tmp.path(), "main.tex", text, 24, 30, "second?", 2).unwrap();

        let reads = std::cell::Cell::new(0);
        let rows = list_with(tmp.path(), &|relative| {
            assert_eq!(relative, "main.tex");
            reads.set(reads.get() + 1);
            // The first read sees the whole file; any later one would see it cut short.
            Some(if reads.get() == 1 {
                text.to_string()
            } else {
                "the".to_string()
            })
        })
        .unwrap();

        assert_eq!(reads.get(), 1, "one read serves every comment on the file");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.state == "anchored"), "{rows:?}");
        assert_eq!((rows[0].from, rows[0].to), (Some(4), Some(10)));
    }

    // Review finding R3.9. A comment's `file` comes from whoever wrote the record, a coauthor or a
    // remote. It was joined onto the project folder as it stood, so an absolute path or `..` made the
    // app read any file the user can and answer (anchored, orphaned, with offsets) about its contents.
    #[test]
    fn a_comment_that_names_a_file_outside_the_project_is_never_read() {
        let text = "the result stands";
        let tmp = project_with(text);
        let repository = git2::Repository::open(tmp.path()).unwrap();
        for file in ["../outside.txt", "/etc/hostname", "sub/../../outside.txt", ""] {
            let record = CommentRecord {
                file: file.to_string(),
                anchor: abstract_tex_comments::Anchor::capture(text, 4, 10).unwrap(),
                author: "mallory".to_string(),
                body: "probe".to_string(),
                created: 1,
            };
            add(&repository, &record).unwrap();
        }

        let asked = std::cell::RefCell::new(Vec::new());
        let rows = list_with(tmp.path(), &|relative| {
            asked.borrow_mut().push(relative.to_string());
            Some(text.to_string())
        })
        .unwrap();

        assert!(
            asked.borrow().is_empty(),
            "nothing outside the project was asked for: {asked:?}"
        );
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.state == "fileMissing"), "{rows:?}");
    }

    #[test]
    fn a_comment_cannot_be_made_on_a_file_outside_the_project() {
        let text = "the result stands";
        let tmp = project_with(text);
        for file in ["../x.tex", "/etc/passwd", "a/../../x.tex", ""] {
            let error = add_comment(tmp.path(), file, text, 4, 10, "?", 1).unwrap_err();
            assert!(
                matches!(error, CommentError::OutsideProject(_)),
                "{file:?}: {error:?}"
            );
        }
        assert!(list(tmp.path()).unwrap().is_empty());
    }

    /// A bare repository standing in for the shared GitHub repository, and a function to make a
    /// working copy of the project pointed at it.
    fn shared_remote() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(tmp.path()).unwrap();
        tmp
    }

    fn working_copy(remote: &Path, text: &str) -> tempfile::TempDir {
        let tmp = project_with(text);
        let repository = git2::Repository::open(tmp.path()).unwrap();
        repository.remote("origin", remote.to_str().unwrap()).unwrap();
        tmp
    }

    #[test]
    fn syncing_with_nothing_anywhere_is_quiet() {
        let remote = shared_remote();
        let ada = working_copy(remote.path(), "the result stands");
        assert_eq!(
            sync(ada.path(), None).unwrap(),
            CommentsSync {
                received: false,
                sent: false
            }
        );
    }

    /// The point of the loop: Ada comments and syncs, Bea syncs and sees it, Bea answers with a
    /// comment of her own, and after one more sync each of them has both.
    #[test]
    fn two_authors_end_up_with_each_others_comments_after_syncing() {
        let text = "the result stands";
        let remote = shared_remote();
        let ada = working_copy(remote.path(), text);
        let bea = working_copy(remote.path(), text);

        add_comment(ada.path(), "main.tex", text, 4, 10, "source?", 1).unwrap();
        assert_eq!(
            sync(ada.path(), None).unwrap(),
            CommentsSync {
                received: false,
                sent: true
            }
        );

        assert!(list(bea.path()).unwrap().is_empty());
        let received = sync(bea.path(), None).unwrap();
        assert!(received.received);
        assert_eq!(list(bea.path()).unwrap().len(), 1);

        add_comment(bea.path(), "main.tex", text, 0, 3, "which result?", 2).unwrap();
        sync(bea.path(), None).unwrap();
        sync(ada.path(), None).unwrap();

        let bodies = |root: &Path| {
            let mut bodies: Vec<String> = list(root).unwrap().into_iter().map(|row| row.body).collect();
            bodies.sort();
            bodies
        };
        assert_eq!(bodies(ada.path()), vec!["source?", "which result?"]);
        assert_eq!(bodies(bea.path()), bodies(ada.path()));
    }

    /// Comments sync must not move the author's branch: it is a separate ref on purpose.
    #[test]
    fn syncing_comments_sends_no_branch() {
        let text = "the result stands";
        let remote = shared_remote();
        let ada = working_copy(remote.path(), text);
        add_comment(ada.path(), "main.tex", text, 4, 10, "?", 1).unwrap();
        sync(ada.path(), None).unwrap();

        let bare = git2::Repository::open_bare(remote.path()).unwrap();
        let heads: Vec<String> = bare
            .references_glob("refs/heads/*")
            .unwrap()
            .flatten()
            .map(|reference| reference.name().unwrap().to_string())
            .collect();
        assert!(heads.is_empty(), "{heads:?}");
    }
}
