//! Asking the assistant to rewrite a selection, and applying the hunks the author accepts (S12.3b;
//! DESIGN.md §5.5): the app-edge half of `abstract_tex_assistant::edit`.
//!
//! Owns the one request that carries a manuscript out of the machine (built and shown first by
//! `assistant_prepare`, sent by `assistant_send` and by nothing else), the guard that is built fresh
//! for it from the `.bib` files as they are on disk now, and the single review in progress. The
//! window never sees a model's raw answer: it sees hunks, each with the sentence that refuses it if
//! the citation guard would, and it asks this module for the text of the hunks it chose. That text
//! is produced here, by `Review::apply`, which checks the guard against the exact string it returns
//! — so a window that mistakes which hunks are allowed still cannot put a fabricated citation in
//! the buffer.
//!
//! **What it must never do:**
//!
//! - Never send anything unless the person asked for exactly this, in a project they switched the
//!   assistant on for, with a provider chosen. The checks are made here, not trusted from the window.
//! - Never return text the guard refuses, nor an answer that was cut off.
//! - Never keep a manuscript: the review holds the selection and the proposal until the window
//!   applies or discards them, and one review at a time.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use abstract_tex_assistant::{
    build_prompt, citation_macros, inspect, proposed_selection, Action, ApplyError, Assistant,
    AssistantError, Guard, KeyStore, Payload, Reply, Review,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::assistant::{AssistantKeys, AssistantSettings};
use crate::bibliography::{self, BibliographyIndex};
use crate::AppState;

/// The longest selection sent, in characters. A paragraph or a few; past this the answer would not
/// fit the model's reply anyway, and a hunk-by-hunk review of a chapter is not a review.
const MAX_SELECTION_CHARS: usize = 12_000;

/// Which rewriting action, as the window names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActionRequest {
    Tighten,
    Clarify,
    MatchVoice,
    Translate { language: String },
}

impl ActionRequest {
    fn action(&self) -> Action {
        match self {
            Self::Tighten => Action::Tighten,
            Self::Clarify => Action::Clarify,
            Self::MatchVoice => Action::MatchVoice,
            Self::Translate { language } => Action::Translate {
                language: language.trim().to_string(),
            },
        }
    }
}

/// One change, positioned in UTF-16 units of the selection (what a JavaScript string counts in, so
/// the window can slice with it), with the reason it may not be accepted if there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkView {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    /// Sentences, one per finding. `None` for a hunk that may be accepted.
    pub refusal: Option<Vec<String>>,
}

/// A proposal, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalView {
    /// Names this review in `assistant_apply`; a newer proposal replaces the old one.
    pub id: u64,
    /// The selection as sent. The window checks the buffer still holds it before applying.
    pub original: String,
    pub hunks: Vec<HunkView>,
    /// Unknown keys the guard held back, each once: what a reference lookup is offered for.
    pub unknown_keys: Vec<String>,
}

/// The review in progress. `Default` is "none"; managed as Tauri state.
#[derive(Default)]
pub struct AssistantReviews {
    /// Built and shown, not yet sent.
    prepared: Mutex<Option<PreparedRequest>>,
    active: Mutex<Option<(u64, Review)>>,
    next_id: AtomicU64,
}

/// The `.bib` keys and the author's citation macros, as they are now: the guard for one request.
pub fn guard_for(index: &BibliographyIndex, preamble: &str) -> Guard {
    Guard::new(index.entries.iter().map(|entry| entry.key.clone()))
        .with_citation_commands(citation_macros(preamble))
}

/// A model's reply turned into a review: the answer cleaned, split into hunks, each asked of the
/// guard. Pure, so the whole path after the network is tested with a reply written by hand.
pub fn review_for_reply(selection: &str, reply: &Reply, guard: &Guard) -> Result<Review, AssistantError> {
    let proposed = proposed_selection(selection, reply)?;
    Ok(Review::new(selection, &proposed, guard))
}

/// The review as the window sees it.
pub fn view_of(id: u64, original: &str, review: &Review) -> ProposalView {
    let utf16 = |text: &str| text.encode_utf16().count();
    let mut unknown_keys: Vec<String> = Vec::new();
    let hunks = review
        .hunks()
        .iter()
        .enumerate()
        .map(|(index, hunk)| {
            let Range { start, end } = hunk.original.clone();
            let start16 = utf16(&original[..start]);
            let refusal = review.refusal(index).map(|findings| {
                for finding in findings {
                    if finding.kind == abstract_tex_assistant::FindingKind::UnknownKey
                        && !unknown_keys.contains(&finding.text)
                    {
                        unknown_keys.push(finding.text.clone());
                    }
                }
                findings.iter().map(|finding| finding.sentence()).collect()
            });
            HunkView {
                start: start16,
                end: start16 + utf16(&original[start..end]),
                replacement: hunk.replacement.clone(),
                refusal,
            }
        })
        .collect();
    ProposalView {
        id,
        original: original.to_string(),
        hunks,
        unknown_keys,
    }
}

/// The project's guard, built from disk: the index of the `.bib` files the document names, and the
/// preamble of the root file for the author's own citation commands.
fn project_guard(state: &AppState) -> Result<(Guard, PathBuf), String> {
    let (root_dir, root_file, extra_bib_files) = {
        let project = state.project.lock().unwrap();
        let project = project
            .as_ref()
            .ok_or_else(|| "No project is open.".to_string())?;
        let root_file = project
            .root_file()
            .ok_or_else(|| "This project has no root .tex file.".to_string())?;
        (
            project.root_dir.clone(),
            root_file,
            project.config.project.extra_bib_files.clone(),
        )
    };
    let index = bibliography::build_index(&root_dir, &root_file, &extra_bib_files);
    let root_text = std::fs::read_to_string(root_dir.join(&root_file)).unwrap_or_default();
    Ok((guard_for(&index, preamble_of(&root_text)), root_dir))
}

/// Everything before `\begin{document}`: where a document defines its own commands.
fn preamble_of(text: &str) -> &str {
    text.find("\\begin{document}").map_or(text, |end| &text[..end])
}

/// Refuse unless every condition for sending holds. Not trusted from the window.
fn ready_to_send(
    settings: &AssistantSettings,
    keys: &dyn KeyStore,
    project_dir: &Path,
) -> Result<(), String> {
    let status = crate::assistant::status(settings, keys, Some(project_dir))?;
    let provider = status
        .provider
        .ok_or_else(|| "Choose a provider in the Assistant view first.".to_string())?;
    if !status.enabled {
        return Err(
            "The assistant is not switched on for this project. Switch it on in the Assistant view."
                .to_string(),
        );
    }
    if !status.has_key
        && !matches!(
            provider,
            abstract_tex_assistant::Provider::OpenAiCompatible { .. }
        )
    {
        return Err("Add an API key in the Assistant view first.".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

type CommandResult<T> = Result<T, String>;

/// A request that has been built and shown and not yet sent. What `assistant_send` sends is this
/// prompt, not one built again, so the request a person looked at is the request that leaves.
struct PreparedRequest {
    id: u64,
    provider: abstract_tex_assistant::Provider,
    prompt: abstract_tex_assistant::Prompt,
    /// The selection the prompt was made from; the review is built against it.
    selection: String,
}

/// The prepared request, if it is the one named, removed from the slot so it can be sent once. A
/// different one stays where it is: a stale click must not cancel a newer request.
fn take_prepared(slot: &Mutex<Option<PreparedRequest>>, id: u64) -> Option<PreparedRequest> {
    let mut slot = slot.lock().unwrap();
    if slot.as_ref().is_some_and(|prepared| prepared.id == id) {
        slot.take()
    } else {
        None
    }
}

/// What the window is shown before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedView {
    /// Names this request in `assistant_send`; a newer one replaces it.
    pub id: u64,
    pub payload: Payload,
    /// The person asked to look first (a setting), so the window should stop and show it.
    pub inspect_first: bool,
}

/// Build the request for rewriting `selection`, and answer with exactly what it would carry.
/// **Sends nothing.** Every refusal that would stop a send is made here too, so the window never
/// offers a *Send* that would be turned down.
#[tauri::command]
pub fn assistant_prepare(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    reviews: State<'_, AssistantReviews>,
    action: ActionRequest,
    selection: String,
) -> CommandResult<PreparedView> {
    if selection.trim().is_empty() {
        return Err("Select some text first.".to_string());
    }
    if selection.chars().count() > MAX_SELECTION_CHARS {
        return Err(
            "That selection is too long to review hunk by hunk. Select a paragraph or two.".to_string(),
        );
    }
    let (_, project_dir) = project_guard(&state)?;
    ready_to_send(&settings, keys.0.as_ref(), &project_dir)?;
    let provider = settings
        .provider()
        .ok_or_else(|| "Choose a provider in the Assistant view first.".to_string())?;
    let has_key = keys
        .0
        .read(&provider.key_slot())
        .map_err(|error| error.to_string())?
        .is_some();

    let prompt = build_prompt(&action.action(), &selection, None).map_err(|error| error.to_string())?;
    let payload = inspect(&provider, has_key, &prompt).map_err(|error| error.to_string())?;
    let id = reviews.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    *reviews.prepared.lock().unwrap() = Some(PreparedRequest {
        id,
        provider,
        prompt,
        selection,
    });
    Ok(PreparedView {
        id,
        payload,
        inspect_first: settings.inspect_first(),
    })
}

/// Send the request `assistant_prepare` made, and answer with the changes as hunks. The one command
/// that sends text out of the machine, and it sends once: the prepared request is taken, so a
/// second call with the same id has nothing to send.
#[tauri::command]
pub async fn assistant_send(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    reviews: State<'_, AssistantReviews>,
    id: u64,
) -> CommandResult<ProposalView> {
    let (guard, project_dir) = project_guard(&state)?;
    // Checked again: the switch may have been turned off between looking and clicking Send.
    ready_to_send(&settings, keys.0.as_ref(), &project_dir)?;
    let prepared = take_prepared(&reviews.prepared, id)
        .ok_or_else(|| "That request is no longer waiting to be sent. Ask again.".to_string())?;
    let PreparedRequest {
        provider,
        prompt,
        selection: sent,
        ..
    } = prepared;

    let key_store: Arc<dyn KeyStore> = Arc::clone(&keys.0);
    // `spawn_blocking`: the HTTP client is blocking, as for the GitHub calls and the connection test.
    let reply = tauri::async_runtime::spawn_blocking(move || {
        Assistant::new().and_then(|assistant| assistant.complete(&provider, key_store.as_ref(), &prompt))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    let review = review_for_reply(&sent, &reply, &guard).map_err(|error| error.to_string())?;
    let view = view_of(id, &sent, &review);
    *reviews.active.lock().unwrap() = Some((id, review));
    Ok(view)
}

/// Forget a prepared request that was looked at and not sent. Not an error when there is none.
#[tauri::command]
pub fn assistant_cancel_prepared(reviews: State<'_, AssistantReviews>, id: u64) -> CommandResult<()> {
    let mut slot = reviews.prepared.lock().unwrap();
    if slot.as_ref().is_some_and(|prepared| prepared.id == id) {
        *slot = None;
    }
    Ok(())
}

/// The selection's new text for exactly the hunks in `accepted`, or the sentences the guard refused
/// it with. The guard is rebuilt from the `.bib` files as they are *now*.
#[tauri::command]
pub fn assistant_apply(
    state: State<'_, AppState>,
    reviews: State<'_, AssistantReviews>,
    id: u64,
    accepted: Vec<bool>,
) -> CommandResult<String> {
    let (guard, _) = project_guard(&state)?;
    let active = reviews.active.lock().unwrap();
    let Some((active_id, review)) = active.as_ref() else {
        return Err("There is no suggestion to apply any more. Ask again.".to_string());
    };
    if *active_id != id {
        return Err("That suggestion has been replaced by a newer one. Ask again.".to_string());
    }
    review.apply(&accepted, &guard).map_err(|error| match error {
        ApplyError::WrongNumberOfChoices { .. } => {
            "The choices do not match the suggestion. Ask again.".to_string()
        }
        ApplyError::Refused(verdict) => verdict
            .findings
            .iter()
            .map(|finding| finding.sentence())
            .collect::<Vec<_>>()
            .join(" "),
    })
}

/// Forget the review in progress, if it is `id`. Not an error when there is none.
#[tauri::command]
pub fn assistant_discard(reviews: State<'_, AssistantReviews>, id: u64) -> CommandResult<()> {
    let mut active = reviews.active.lock().unwrap();
    if active.as_ref().is_some_and(|(active_id, _)| *active_id == id) {
        *active = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibliography::EntrySummary;
    use abstract_tex_assistant::Usage;

    fn index_with(keys: &[&str]) -> BibliographyIndex {
        BibliographyIndex {
            entries: keys
                .iter()
                .map(|key| EntrySummary {
                    key: (*key).to_string(),
                    entry_type: "article".into(),
                    author: None,
                    year: None,
                    title: None,
                    doi: None,
                    eprint: None,
                    isbn: None,
                    file: "refs.bib".into(),
                    span: texbib::Span { start: 0, end: 0 },
                })
                .collect(),
            ..BibliographyIndex::default()
        }
    }

    fn reply(text: &str) -> Reply {
        Reply {
            text: text.to_string(),
            truncated: false,
            usage: Usage::default(),
        }
    }

    #[test]
    fn the_guard_knows_the_bib_keys_and_the_authors_citation_macros() {
        let guard = guard_for(
            &index_with(&["smith2020"]),
            "\\newcommand{\\see}[1]{(see \\cite{#1})}",
        );
        assert!(guard.check("", "\\cite{smith2020}").is_clean());
        assert!(!guard.check("", "\\cite{invented}").is_clean());
        assert!(
            !guard.check("", "\\see{invented}").is_clean(),
            "the author's own wrapper is a citation command"
        );
    }

    #[test]
    fn the_preamble_is_what_comes_before_begin_document() {
        assert_eq!(
            preamble_of("\\def\\a{b}\n\\begin{document}\nbody \\cite{x}"),
            "\\def\\a{b}\n"
        );
        assert_eq!(preamble_of("no document here"), "no document here");
    }

    #[test]
    fn a_reply_becomes_hunks_and_a_fabricated_citation_is_refused_with_its_sentence() {
        let guard = guard_for(&index_with(&["smith2020"]), "");
        let selection = "Prior work is clear and very very old.";
        let review = review_for_reply(
            selection,
            &reply("Prior work \\cite{invented2021} is clear and old."),
            &guard,
        )
        .unwrap();
        let view = view_of(1, selection, &review);

        let refused: Vec<&HunkView> = view.hunks.iter().filter(|hunk| hunk.refusal.is_some()).collect();
        assert_eq!(refused.len(), 1, "{view:?}");
        assert!(refused[0].refusal.as_ref().unwrap()[0].contains("invented2021"));
        assert_eq!(view.unknown_keys, vec!["invented2021".to_string()]);
        assert!(
            view.hunks.iter().any(|hunk| hunk.refusal.is_none()),
            "the honest change is still on offer"
        );
    }

    #[test]
    fn a_cut_off_reply_is_an_error_not_a_review() {
        let guard = guard_for(&index_with(&[]), "");
        let mut cut = reply("Half a sen");
        cut.truncated = true;
        assert!(matches!(
            review_for_reply("Whole sentence here.", &cut, &guard),
            Err(AssistantError::CutOff)
        ));
    }

    #[test]
    fn hunk_positions_are_utf16_units_of_the_selection_so_the_window_can_slice_with_them() {
        let guard = guard_for(&index_with(&[]), "");
        // An astral character (two UTF-16 units, four bytes) and an accented one (one unit, two bytes).
        let selection = "𝒳 naïve very good";
        let review = review_for_reply(selection, &reply("𝒳 naïve good"), &guard).unwrap();
        let view = view_of(7, selection, &review);
        assert_eq!(view.hunks.len(), 1);
        let hunk = &view.hunks[0];

        let units: Vec<u16> = selection.encode_utf16().collect();
        let removed = String::from_utf16(&units[hunk.start..hunk.end]).unwrap();
        assert_eq!(removed, "very ", "sliced the way JavaScript slices");
        assert_eq!(hunk.replacement, "");
    }

    #[test]
    fn nothing_is_sent_unless_the_provider_the_key_and_the_projects_switch_all_agree() {
        use abstract_tex_assistant::{MemoryKeyStore, Provider};
        let config = tempfile::tempdir().unwrap();
        let settings = AssistantSettings::at(config.path().join("assistant.json"));
        let keys = MemoryKeyStore::default();
        let project = Path::new("/work/thesis");

        assert!(ready_to_send(&settings, &keys, project)
            .unwrap_err()
            .contains("Choose a provider"));

        crate::assistant::save_provider(&settings, &keys, Provider::anthropic("claude-x"), None).unwrap();
        assert!(ready_to_send(&settings, &keys, project)
            .unwrap_err()
            .contains("not switched on"));

        crate::assistant::set_enabled(&settings, project, true).unwrap();
        assert!(ready_to_send(&settings, &keys, project)
            .unwrap_err()
            .contains("API key"));

        crate::assistant::save_provider(
            &settings,
            &keys,
            Provider::anthropic("claude-x"),
            Some("sk-x".into()),
        )
        .unwrap();
        assert!(ready_to_send(&settings, &keys, project).is_ok());
        assert!(
            ready_to_send(&settings, &keys, Path::new("/work/thesis-clone"))
                .unwrap_err()
                .contains("not switched on"),
            "a copy elsewhere starts off"
        );

        // A model on this computer needs no key.
        crate::assistant::save_provider(
            &settings,
            &keys,
            Provider::openai_compatible("http://localhost:1/v1", "m"),
            None,
        )
        .unwrap();
        assert!(ready_to_send(&settings, &keys, project).is_ok());
    }

    fn prepared(id: u64) -> PreparedRequest {
        PreparedRequest {
            id,
            provider: abstract_tex_assistant::Provider::anthropic("claude-x"),
            prompt: build_prompt(&Action::Tighten, "Some text.", None).unwrap(),
            selection: "Some text.".into(),
        }
    }

    #[test]
    fn a_prepared_request_can_be_sent_once_and_only_by_its_own_id() {
        let slot = Mutex::new(Some(prepared(4)));
        assert!(take_prepared(&slot, 3).is_none(), "an old id is not the request");
        assert!(
            slot.lock().unwrap().is_some(),
            "and does not discard the real one"
        );
        assert_eq!(take_prepared(&slot, 4).map(|request| request.id), Some(4));
        assert!(
            take_prepared(&slot, 4).is_none(),
            "a second send has nothing to send"
        );
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn what_is_shown_for_a_selection_is_the_request_that_is_sent_for_it() {
        // The inspector's body is the body `build_request` makes from the very prompt that is kept.
        let request = prepared(1);
        let shown = inspect(&request.provider, true, &request.prompt).unwrap();
        let real =
            abstract_tex_assistant::build_request(&request.provider, Some("sk-x"), &request.prompt).unwrap();
        assert_eq!(shown.body, real.body);
        assert!(shown.body.contains("Some text."));
        assert!(
            !shown.body.contains("<document>"),
            "a selection-only request carries no manuscript"
        );
    }

    #[test]
    fn an_action_request_reads_the_way_the_window_writes_it() {
        let tighten: ActionRequest = serde_json::from_str(r#"{"kind":"tighten"}"#).unwrap();
        assert_eq!(tighten.action(), Action::Tighten);
        let translate: ActionRequest =
            serde_json::from_str(r#"{"kind":"translate","language":" German "}"#).unwrap();
        assert_eq!(
            translate.action(),
            Action::Translate {
                language: "German".into()
            }
        );
        let voice: ActionRequest = serde_json::from_str(r#"{"kind":"matchVoice"}"#).unwrap();
        assert_eq!(voice.action(), Action::MatchVoice);
    }
}
