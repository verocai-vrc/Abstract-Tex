//! Spike, run before Sprint 14 (`0.1/SPRINTS.md`): can a comment anchored to quoted text survive
//! being written into a Git ref, pushed, fetched, and merged — the storage DESIGN.md §5.6 and
//! design-interview.md F5 proposed as the leading candidate for "where do comments go when a live
//! session ends"? This crate is the proof, not the feature: no Tauri command, no CRDT relative
//! position (that is the live session itself, S14.2/S14.3's job), no network credentials beyond
//! what a test needs to drive a `file://` remote. What it answers:
//!
//! 1. **Can a comment find its own text again after an edit it did not see?** [`anchor`] — quote
//!    plus a little surrounding context, never a line number or byte offset.
//! 2. **Can two authors who each commented offline merge without a hand-resolved conflict, over
//!    an ordinary Git push/fetch with an explicit refspec?** [`store`] — yes, by making every
//!    comment its own content-addressed blob, so the tree merge sees only additions. Read
//!    `store`'s module doc for what this does *not* yet solve (editing or resolving a comment).
//!
//! **What it must never do**, matching the other hidden-ref crate's own rule
//! (`abstract-tex-snapshot`): never touch the author's `HEAD`, index, working tree or branches.
//! Every operation here is scoped to [`store::COMMENTS_REF`] and nothing else.

pub mod anchor;
pub mod store;

pub use anchor::{Anchor, OrphanReason, Reanchored};
pub use store::{
    add, fetch, load, load_all, merge_from, push, AnchorStatus, CommentRecord, CommentsError, LoadedComment,
    MergeOutcome, COMMENTS_REF,
};
