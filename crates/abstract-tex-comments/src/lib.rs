//! Started as a spike, before Sprint 14 (`0.1/SPRINTS.md`): can a comment anchored to quoted text
//! survive being written into a Git ref, pushed, fetched, and merged — the storage DESIGN.md §5.6
//! and design-interview.md F5 proposed as the leading candidate for "where do comments go when a
//! live session ends"? It passed, and S14.3a/S14.3b added the two things it deliberately left
//! open. No Tauri command and no CRDT relative position live here (that is the live session
//! itself, S14.2/the rest of S14.3's job), no network credentials beyond what a test needs to
//! drive a `file://` remote. What this crate answers:
//!
//! 1. **Can a comment find its own text again after an edit it did not see?** [`anchor`] — quote
//!    plus a little surrounding context, never a line number or byte offset.
//! 2. **Can two authors who each commented offline merge without a hand-resolved conflict, over
//!    an ordinary Git push/fetch with an explicit refspec?** [`store`] — yes, by making every
//!    comment its own content-addressed blob, so the tree merge sees only additions.
//! 3. **Can resolving a comment merge the same way, without rewriting it (S14.3a)?** Yes: a
//!    resolve-or-reopen is its own content-addressed event, [`ResolutionRecord`], naming the
//!    comment rather than touching it, folded into [`LoadedComment::resolved`] by [`load`].
//! 4. **Can a caller that only has a project folder use any of this without learning what a
//!    `git2::Repository` is (S14.3b)?** [`project`] — the same `&Path`-based shape
//!    `abstract-tex-snapshot` already uses for its own hidden ref, so the eventual Tauri command
//!    is "thin by design: validate, delegate, emit", `commands.rs`'s own words for itself.
//!
//! **What it must never do**, matching the other hidden-ref crate's own rule
//! (`abstract-tex-snapshot`): never touch the author's `HEAD`, index, working tree or branches.
//! Every operation here is scoped to [`store::COMMENTS_REF`] and nothing else.

pub mod anchor;
pub mod project;
pub mod store;

pub use anchor::{Anchor, OrphanReason, Reanchored};
pub use store::{
    add, add_resolution, fetch, load, load_all, load_resolutions, merge_from, push, resolve, AnchorStatus,
    CommentRecord, CommentsError, LoadedComment, MergeOutcome, ResolutionRecord, COMMENTS_REF,
};
