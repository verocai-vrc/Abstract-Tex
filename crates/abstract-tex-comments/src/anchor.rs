//! Anchoring a comment to quoted text, and finding that text again later.
//!
//! A comment never stores a line number or a byte offset as its permanent address — both move
//! under an edit made while the commenter is away, which is the exact case DESIGN.md §5.6 asks
//! this spike to survive. Instead a comment remembers the words it was attached to, plus a little
//! of what came before and after them, and [`reanchor`] goes looking for that quote again every
//! time the comment is loaded.

use serde::{Deserialize, Serialize};

/// How many `char`s of surrounding text are kept on each side of the quote.
///
/// Characters, not bytes: slicing a `str` on a byte index that lands inside a multi-byte UTF-8
/// character panics, and a manuscript is never assumed to be ASCII. Forty is enough to tell two
/// occurrences of a short phrase apart in ordinary prose without the anchor itself becoming a
/// second copy of the paragraph.
const CONTEXT_CHARS: usize = 40;

/// Where a comment was attached, independent of any line or byte offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// The exact text the comment is about. Never empty — see [`Anchor::capture`].
    pub quote: String,
    /// Up to [`CONTEXT_CHARS`] characters immediately before the quote, as it was when the
    /// comment was made.
    pub context_before: String,
    /// Up to [`CONTEXT_CHARS`] characters immediately after the quote.
    pub context_after: String,
}

/// What [`reanchor`] found when it went looking for an anchor's quote in a file's current text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reanchored {
    /// The quote occurs exactly once, or occurs more than once but the context picks out exactly
    /// one of them. `start`/`end` are byte offsets into the text that was searched, so the caller
    /// can slice it directly.
    Found { start: usize, end: usize },
    /// The quote is nowhere in the text any more — the sentence was rewritten, the paragraph was
    /// cut, or the comment is on a file that no longer has this passage. DESIGN.md §5.6 calls
    /// this "orphaned" and asks that it be listed, never silently dropped.
    Orphaned(OrphanReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanReason {
    /// The quote does not occur in the text at all.
    TextNotFound,
    /// The quote occurs more than once, and the stored context does not pick out a single one of
    /// them — either none of the occurrences' surroundings match what was recorded, or (rarer)
    /// more than one still does, because the surrounding text repeats too. Guessing which
    /// occurrence was meant would put a comment on the wrong sentence with nothing to say so;
    /// listing it as orphaned keeps that decision with the person.
    Ambiguous { occurrences: usize },
}

impl Anchor {
    /// Build an anchor for the text found at the byte range `start..end` of `text`.
    ///
    /// `start` and `end` must land on `char` boundaries (whatever produced the selection already
    /// guarantees this: a CodeMirror selection and a byte range read back from this same text both
    /// do) and the quote itself must not be empty — an empty anchor would "find" its first match
    /// at every position in every file, which is not an anchor at all.
    pub fn capture(text: &str, start: usize, end: usize) -> Option<Anchor> {
        if start >= end || end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return None;
        }
        let quote = text[start..end].to_string();
        Some(Anchor {
            quote,
            context_before: last_chars(&text[..start], CONTEXT_CHARS),
            context_after: first_chars(&text[end..], CONTEXT_CHARS),
        })
    }
}

/// Look for `anchor`'s quote in `text` as it is now.
pub fn reanchor(text: &str, anchor: &Anchor) -> Reanchored {
    // `capture` never makes an empty quote, but a record read back from a remote can carry one, and
    // an empty string "occurs" at every position of every file.
    if anchor.quote.is_empty() {
        return Reanchored::Orphaned(OrphanReason::TextNotFound);
    }
    let matches: Vec<(usize, usize)> = text
        .match_indices(anchor.quote.as_str())
        .map(|(start, matched)| (start, start + matched.len()))
        .collect();

    match matches.len() {
        0 => Reanchored::Orphaned(OrphanReason::TextNotFound),
        1 => {
            let (start, end) = matches[0];
            Reanchored::Found { start, end }
        }
        occurrences => {
            let agreeing: Vec<(usize, usize)> = matches
                .into_iter()
                .filter(|&(start, end)| context_matches(text, start, end, anchor))
                .collect();
            match agreeing.as_slice() {
                [(start, end)] => Reanchored::Found {
                    start: *start,
                    end: *end,
                },
                _ => Reanchored::Orphaned(OrphanReason::Ambiguous { occurrences }),
            }
        }
    }
}

/// Does the text actually surrounding `start..end` match what the anchor recorded?
///
/// Compared at whatever length the anchor happens to hold (it is already at most
/// [`CONTEXT_CHARS`] characters, captured the same way [`last_chars`]/[`first_chars`] produce it
/// here, so the two sides are always measured the same way).
fn context_matches(text: &str, start: usize, end: usize, anchor: &Anchor) -> bool {
    let before_len = anchor.context_before.chars().count();
    let after_len = anchor.context_after.chars().count();
    last_chars(&text[..start], before_len) == anchor.context_before
        && first_chars(&text[end..], after_len) == anchor.context_after
}

/// The last `n` characters of `text`, found by walking back from the end. The first version
/// collected every character of `text` into a `Vec` to take the tail, which made a common word in
/// a long chapter quadratic (review finding R3.8).
fn last_chars(text: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let start = text
        .char_indices()
        .rev()
        .nth(n - 1)
        .map_or(0, |(byte, _character)| byte);
    text[start..].to_string()
}

fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_that_occurs_once_reanchors_at_the_same_text_after_an_edit_elsewhere() {
        let original = "The cat sat on the mat. It was comfortable.";
        let anchor = Anchor::capture(original, 4, 7).unwrap(); // "cat"
        assert_eq!(anchor.quote, "cat");

        let edited = "Yesterday, the cat sat on the mat. It was comfortable.";
        match reanchor(edited, &anchor) {
            Reanchored::Found { start, end } => assert_eq!(&edited[start..end], "cat"),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn a_rewritten_sentence_orphans_the_comment_instead_of_guessing() {
        let original = "The cat sat on the mat.";
        let anchor = Anchor::capture(original, 4, 7).unwrap(); // "cat"
        let rewritten = "The dog slept on the rug.";
        assert_eq!(
            reanchor(rewritten, &anchor),
            Reanchored::Orphaned(OrphanReason::TextNotFound)
        );
    }

    #[test]
    fn a_repeated_phrase_is_told_apart_by_its_context() {
        let text = "First draft: the result was significant. Later: the result was significant.";
        // The second occurrence, with "Later: " before it.
        let start = text.rfind("the result").unwrap();
        let end = start + "the result".len();
        let anchor = Anchor::capture(text, start, end).unwrap();

        let edited = format!("{} More text added at the end.", text);
        match reanchor(&edited, &anchor) {
            Reanchored::Found { start: found, .. } => assert_eq!(found, start),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn identical_context_on_both_sides_is_ambiguous_rather_than_a_guess() {
        // Built by hand rather than through `capture`, so the context is deliberately short and
        // identical around both occurrences of "needle" — exactly the case `capture`'s own
        // 40-character window is meant to make rare, not impossible.
        let text = "a needle b a needle b";
        let anchor = Anchor {
            quote: "needle".to_string(),
            context_before: "a ".to_string(),
            context_after: " b".to_string(),
        };
        assert_eq!(
            reanchor(text, &anchor),
            Reanchored::Orphaned(OrphanReason::Ambiguous { occurrences: 2 })
        );
    }

    #[test]
    fn capture_refuses_an_empty_or_out_of_range_selection() {
        let text = "short";
        assert!(Anchor::capture(text, 2, 2).is_none(), "empty selection");
        assert!(Anchor::capture(text, 0, 50).is_none(), "past the end");
        assert!(Anchor::capture(text, 5, 2).is_none(), "backwards");
    }

    #[test]
    fn context_is_captured_in_characters_not_bytes_so_multi_byte_text_never_panics() {
        // "café" has a two-byte 'é'; a byte-based slice of "the last 2 units" would land inside
        // it. Capturing and reanchoring across a multi-byte boundary must not panic.
        let text = "café résumé is a word";
        let start = text.find("résumé").unwrap();
        let end = start + "résumé".len();
        let anchor = Anchor::capture(text, start, end).unwrap();
        assert_eq!(anchor.quote, "résumé");
        assert_eq!(reanchor(text, &anchor), Reanchored::Found { start, end });
    }

    // Review finding R3.8. `context_matches` collected the whole text before each occurrence into a
    // `Vec<char>` to take the last forty, so a quote that is a common word did quadratic work on
    // every list. 4 000 occurrences in 100 KB is about 5 s of that in a debug build.
    #[test]
    fn a_common_word_in_a_long_chapter_is_reanchored_quickly() {
        let text = "the quick brown fox jumps over a lazy dog. ".repeat(4_000);
        let start = text.len() / 2 + 4; // somewhere in the middle, on a word
        let start = text[start..].find("quick").unwrap() + start;
        let anchor = Anchor::capture(&text, start, start + "quick".len()).unwrap();

        let started = std::time::Instant::now();
        let found = reanchor(&text, &anchor);
        let took = started.elapsed();

        assert!(
            took < std::time::Duration::from_secs(1),
            "re-anchoring took {took:?}"
        );
        // The surroundings repeat too, so this is honestly ambiguous; what matters is the speed.
        assert!(matches!(
            found,
            Reanchored::Orphaned(OrphanReason::Ambiguous { .. })
        ));
    }

    #[test]
    fn the_surrounding_text_is_still_compared_exactly_after_the_speed_up() {
        let text = "alpha beta gamma. delta beta epsilon.";
        let first = text.find("beta").unwrap();
        let second = text.rfind("beta").unwrap();
        let anchor = Anchor::capture(text, second, second + 4).unwrap();
        assert_eq!(
            reanchor(text, &anchor),
            Reanchored::Found {
                start: second,
                end: second + 4
            }
        );
        assert_ne!(first, second);
        // Multi-byte characters around the quote are measured in characters, not bytes.
        let text = "ééé résultat ééé";
        let start = text.find("résultat").unwrap();
        let anchor = Anchor::capture(text, start, start + "résultat".len()).unwrap();
        assert!(matches!(reanchor(text, &anchor), Reanchored::Found { .. }));
    }

    // A record from outside can carry an empty quote, which `match_indices` finds at every position.
    #[test]
    fn an_empty_quote_is_orphaned_not_matched_everywhere() {
        let anchor = Anchor {
            quote: String::new(),
            context_before: String::new(),
            context_after: String::new(),
        };
        assert_eq!(
            reanchor("any text at all", &anchor),
            Reanchored::Orphaned(OrphanReason::TextNotFound)
        );
    }
}
