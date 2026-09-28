//! The external-change reconciler (DESIGN.md §5.6).
//!
//! When a file changes on disk while it is open — a `git checkout`, an edit in another editor —
//! we must not overwrite the session document with the new text. That would destroy undo
//! history and, at v0.8, every remote peer's view. Instead we compute *what changed* and apply
//! it as a sequence of small edits to the Yjs document, exactly as if the author had typed them.
//!
//! This crate owns that computation and nothing else. It never touches the filesystem and never
//! decides *whether* to reconcile (a dirty buffer plus a changed file is a conflict the app must
//! ask about, never merge).
//!
//! # Why indices are in UTF-16 code units
//!
//! Rust strings are UTF-8 and index by byte. JavaScript strings — and therefore Yjs `Y.Text` —
//! index by UTF-16 code unit, where `é` is one unit and `𝑥` (a maths italic x, common in TeX
//! documents) is two. If we sent byte or char offsets, every non-ASCII character before an edit
//! would shift it. So this crate does the conversion and the frontend can apply ops verbatim.

use similar::{Algorithm, ChangeTag, TextDiff};

/// One edit: at `index`, delete `delete` UTF-16 units, then insert `insert`.
///
/// Ops are meant to be applied *in order*, each against the document as the previous ops left
/// it. That is the natural order for `ytext.delete(index, n); ytext.insert(index, s)` calls.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextOp {
    pub index: usize,
    pub delete: usize,
    pub insert: String,
}

/// Compute the ops that turn `old` into `new`.
///
/// Two passes: strip the common prefix and suffix first, then run a character-level diff on
/// what remains. Almost every real change is one paragraph in a long file, so the diff usually
/// sees a few hundred characters instead of a hundred thousand. Both steps must respect UTF-8
/// character boundaries, which is why the prefix/suffix code walks `chars()` rather than bytes.
pub fn diff_ops(old: &str, new: &str) -> Vec<TextOp> {
    if old == new {
        return Vec::new();
    }

    // Common prefix, measured in bytes but advanced one whole char at a time.
    let mut prefix_bytes = 0;
    let mut prefix_units = 0;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        prefix_bytes += a.len_utf8();
        prefix_units += a.len_utf16();
    }

    // Common suffix over the remainders, walking backwards.
    let old_rest = &old[prefix_bytes..];
    let new_rest = &new[prefix_bytes..];
    let mut suffix_bytes_old = 0;
    let mut suffix_bytes_new = 0;
    for (a, b) in old_rest.chars().rev().zip(new_rest.chars().rev()) {
        if a != b {
            break;
        }
        suffix_bytes_old += a.len_utf8();
        suffix_bytes_new += b.len_utf8();
    }
    let old_mid = &old_rest[..old_rest.len() - suffix_bytes_old];
    let new_mid = &new_rest[..new_rest.len() - suffix_bytes_new];

    // Fast path: a pure insertion or deletion needs no diff at all, and a *small* replacement
    // is better expressed as one op than as a character-level interleaving. Myers would happily
    // turn "cat" → "$ a a" into three tiny inserts around the shared letters, which is minimal
    // in characters but noisier for the CRDT than one replace. Below this size, one op.
    const SMALL_REPLACE_UNITS: usize = 64;
    let old_mid_units = utf16_len(old_mid);
    let new_mid_units = utf16_len(new_mid);
    if old_mid.is_empty()
        || new_mid.is_empty()
        || (old_mid_units <= SMALL_REPLACE_UNITS && new_mid_units <= SMALL_REPLACE_UNITS)
    {
        return vec![TextOp {
            index: prefix_units,
            delete: old_mid_units,
            insert: new_mid.to_string(),
        }];
    }

    // General case: character diff of the middle. Myers is the standard choice; `similar`
    // also offers Patience, which reads better for humans but we are not showing this to one.
    let diff = TextDiff::configure().algorithm(Algorithm::Myers).diff_chars(old_mid, new_mid);

    let mut ops: Vec<TextOp> = Vec::new();
    // Position in the document *as transformed so far*, in UTF-16 units.
    let mut cursor = prefix_units;
    // Deletes and inserts for the same spot arrive as separate changes; we fold them into one op.
    let mut pending: Option<TextOp> = None;

    for change in diff.iter_all_changes() {
        let text = change.value();
        match change.tag() {
            ChangeTag::Equal => {
                if let Some(op) = pending.take() {
                    cursor = op.index + utf16_len(&op.insert);
                    ops.push(op);
                }
                cursor += utf16_len(text);
            }
            ChangeTag::Delete => {
                pending.get_or_insert_with(|| TextOp { index: cursor, delete: 0, insert: String::new() }).delete +=
                    utf16_len(text);
            }
            ChangeTag::Insert => {
                pending
                    .get_or_insert_with(|| TextOp { index: cursor, delete: 0, insert: String::new() })
                    .insert
                    .push_str(text);
            }
        }
    }
    if let Some(op) = pending {
        ops.push(op);
    }
    ops
}

/// Apply ops to a string the way the frontend applies them to `Y.Text`. Used by tests to prove
/// `diff_ops` is correct; also handy for anyone reasoning about op semantics.
pub fn apply_ops(text: &str, ops: &[TextOp]) -> String {
    // Work in UTF-16 so indices mean what they mean on the JavaScript side.
    let mut units: Vec<u16> = text.encode_utf16().collect();
    for op in ops {
        let end = op.index + op.delete;
        assert!(end <= units.len(), "op {op:?} out of range for length {}", units.len());
        let insert: Vec<u16> = op.insert.encode_utf16().collect();
        units.splice(op.index..end, insert);
    }
    String::from_utf16(&units).expect("ops never split a surrogate pair")
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn identical_text_yields_no_ops() {
        assert!(diff_ops("abc", "abc").is_empty());
    }

    #[test]
    fn pure_insertion_is_one_op() {
        let ops = diff_ops("hello world", "hello brave world");
        assert_eq!(ops, vec![TextOp { index: 6, delete: 0, insert: "brave ".into() }]);
    }

    #[test]
    fn pure_deletion_is_one_op() {
        let ops = diff_ops("hello brave world", "hello world");
        assert_eq!(ops, vec![TextOp { index: 6, delete: 6, insert: String::new() }]);
    }

    #[test]
    fn replacement_in_the_middle_deletes_then_inserts() {
        let ops = diff_ops("The cat sat.", "The dog sat.");
        assert_eq!(apply_ops("The cat sat.", &ops), "The dog sat.");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].index, 4);
    }

    #[test]
    fn indices_count_utf16_units_not_bytes() {
        // 'é' is 2 bytes / 1 unit; '𝑥' is 4 bytes / 2 units.
        let old = "é𝑥 = 1";
        let new = "é𝑥 = 2";
        let ops = diff_ops(old, new);
        assert_eq!(ops, vec![TextOp { index: 6, delete: 1, insert: "2".into() }]);
        assert_eq!(apply_ops(old, &ops), new);
    }

    #[test]
    fn multiple_separated_edits_apply_in_order() {
        let old = "one two three four five";
        let new = "ONE two three FOUR five";
        let ops = diff_ops(old, new);
        assert_eq!(apply_ops(old, &ops), new);
    }

    #[test]
    fn large_changed_region_is_diffed_rather_than_replaced_wholesale() {
        // Two paragraphs edited far apart in a long middle: a wholesale replace would delete
        // and reinsert everything between them; a diff keeps the untouched text in place.
        let filler = "lorem ipsum dolor sit amet ".repeat(10);
        let old = format!("alpha {filler} omega");
        let new = format!("ALPHA {filler} OMEGA");
        let ops = diff_ops(&old, &new);
        assert_eq!(apply_ops(&old, &ops), new);
        let total_deleted: usize = ops.iter().map(|o| o.delete).sum();
        assert!(total_deleted < 20, "diff should not delete the filler: {ops:?}");
    }

    // The property that matters: for any two strings, applying the ops to the first yields the
    // second. `proptest` generates thousands of pairs, including ones with multi-byte
    // characters, and shrinks any failure to a minimal example.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2000))]

        #[test]
        fn ops_round_trip_arbitrary_strings(old in "\\PC{0,40}", new in "\\PC{0,40}") {
            let ops = diff_ops(&old, &new);
            prop_assert_eq!(apply_ops(&old, &ops), new);
        }

        #[test]
        fn ops_round_trip_small_edits_to_latex(
            base in "[a-z \\\\{}$_^\\n]{0,80}",
            at in 0usize..80,
            del in 0usize..5,
            ins in "[a-z $]{0,5}",
        ) {
            // Build `new` by editing `base` at a random point, which is what real changes look like.
            let chars: Vec<char> = base.chars().collect();
            let at = at.min(chars.len());
            let end = (at + del).min(chars.len());
            let mut edited: String = chars[..at].iter().collect();
            edited.push_str(&ins);
            edited.extend(chars[end..].iter());

            let ops = diff_ops(&base, &edited);
            prop_assert_eq!(apply_ops(&base, &ops), edited);
            // A single local edit must never explode into many ops.
            prop_assert!(ops.len() <= 2, "too many ops: {:?}", ops);
        }
    }
}
