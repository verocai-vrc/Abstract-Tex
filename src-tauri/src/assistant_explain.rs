//! Asking the assistant what a compile error means, when no rule in the catalog does (S13.2;
//! DESIGN.md §5.5): the app-edge half of `abstract_tex_assistant::explain`.
//!
//! Owns finding the error's lines in the build log, taking the project's folder name out of them,
//! the answers remembered between runs, and the two commands: `assistant_prepare_explain` (builds
//! the request, or finds a remembered answer; sends nothing) and `assistant_send_explain`.
//!
//! **What it must never do:**
//!
//! - Never send automatically. The model fallback is offered on a card and is sent only by a click
//!   on *Send* in the payload dialog, **every time**, whatever the "show me each request" setting
//!   says: this request is made from a log the person did not choose to share.
//! - Never send more than the error's own log lines, with the project's folder removed. Not the
//!   document, not the rest of the log.
//! - Never keep a remembered answer anywhere but the project's own `.abstract-tex/cache/` (which is
//!   self-ignoring and deletable at any moment), and never let a failure to remember one fail the
//!   answer the person is waiting for.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use abstract_tex_assistant::{
    build_explain_prompt, cache_key, inspect, log_excerpt, signature, Assistant, KeyStore,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::assistant::{AssistantKeys, AssistantSettings};
use crate::assistant_review::{
    project_root, ready_to_send, take_prepared, AssistantReviews, PreparedRequest, PreparedView, Purpose,
};
use crate::project::{write_atomically, STATE_DIR};
use crate::AppState;

type CommandResult<T> = Result<T, String>;

/// What the window gets for "explain this error": a remembered answer (nothing is sent), or a
/// request to look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPrepared {
    /// An answer to this same problem from earlier on this computer. When present, no request
    /// exists and nothing will be sent.
    pub cached_answer: Option<String>,
    pub prepared: Option<PreparedView>,
}

/// One remembered answer. The whole signature is stored so a hash collision is a miss, not a
/// wrong answer.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    signature: String,
    answer: String,
}

/// Where answers are kept for this project.
fn cache_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(STATE_DIR).join("cache").join("explain")
}

fn cached_answer(dir: &Path, signature: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(format!("{}.json", cache_key(signature)))).ok()?;
    let entry: CacheEntry = serde_json::from_str(&text).ok()?;
    (entry.signature == signature).then_some(entry.answer)
}

/// Remember an answer. Returns whether it was kept; the caller does not fail on `false`.
fn remember_answer(dir: &Path, signature: &str, answer: &str) -> bool {
    let entry = CacheEntry {
        signature: signature.to_string(),
        answer: answer.to_string(),
    };
    let Ok(text) = serde_json::to_string(&entry) else {
        return false;
    };
    std::fs::create_dir_all(dir).is_ok()
        && write_atomically(&dir.join(format!("{}.json", cache_key(signature))), &text).is_ok()
}

/// The text with the project's folder replaced by `<project>`, in the spelling the log used and in
/// the other slash direction, so a path to the author's home folder or user name does not leave.
fn without_project_path(text: &str, project_dir: &Path) -> String {
    let native = project_dir.to_string_lossy().into_owned();
    let mut cleaned = text.replace(&native, "<project>");
    for spelling in [native.replace('\\', "/"), native.replace('/', "\\")] {
        if !spelling.is_empty() {
            cleaned = cleaned.replace(&spelling, "<project>");
        }
    }
    cleaned
}

/// The build log of the open project and its folder.
fn build_log(state: &AppState) -> Result<(String, PathBuf), String> {
    let root = project_root(state)?;
    let project_dir = root.dir;
    let stem = root
        .file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    let build_dir = {
        let project = state.project.lock().unwrap();
        project
            .as_ref()
            .ok_or_else(|| "No project is open.".to_string())?
            .build_dir()
    };
    let log = std::fs::read(build_dir.join(format!("{stem}.log")))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    Ok((log, project_dir))
}

/// The excerpt for one error as it would be sent: its log lines, the project folder removed.
fn excerpt_for(log: &str, raw_message: &str, project_dir: &Path) -> Option<String> {
    log_excerpt(log, raw_message).map(|excerpt| without_project_path(&excerpt, project_dir))
}

/// Build the request that would explain `raw_message`, or answer from memory. **Sends nothing.**
#[tauri::command]
pub fn assistant_prepare_explain(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    reviews: State<'_, AssistantReviews>,
    raw_message: String,
) -> CommandResult<ExplainPrepared> {
    let (log, project_dir) = build_log(&state)?;
    ready_to_send(&settings, keys.0.as_ref(), &project_dir)?;
    let provider = settings
        .provider()
        .ok_or_else(|| "Choose a provider in the Assistant view first.".to_string())?;
    let excerpt = excerpt_for(&log, &raw_message, &project_dir)
        .ok_or_else(|| "That error is not in the latest build log. Build again, then ask.".to_string())?;

    let signature = signature(&excerpt);
    if let Some(answer) = cached_answer(&cache_dir(&project_dir), &signature) {
        return Ok(ExplainPrepared {
            cached_answer: Some(answer),
            prepared: None,
        });
    }

    let has_key = keys
        .0
        .read(&provider.key_slot())
        .map_err(|error| error.to_string())?
        .is_some();
    let prompt = build_explain_prompt(&excerpt).map_err(|error| error.to_string())?;
    let payload = inspect(&provider, has_key, &prompt).map_err(|error| error.to_string())?;
    let id = reviews.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    *reviews.prepared.lock().unwrap() = Some(PreparedRequest {
        id,
        provider,
        prompt,
        purpose: Purpose::Explain { signature },
    });
    Ok(ExplainPrepared {
        cached_answer: None,
        prepared: Some(PreparedView {
            id,
            payload,
            // Always shown, whatever the setting: see the module doc.
            inspect_first: true,
            forced: !settings.inspect_first(),
        }),
    })
}

/// Send the prepared explanation request, once, and answer with the model's words. The answer is
/// remembered for the next time the same problem comes up.
#[tauri::command]
pub async fn assistant_send_explain(
    state: State<'_, AppState>,
    settings: State<'_, AssistantSettings>,
    keys: State<'_, AssistantKeys>,
    reviews: State<'_, AssistantReviews>,
    id: u64,
) -> CommandResult<String> {
    let project_dir = project_root(&state)?.dir;
    ready_to_send(&settings, keys.0.as_ref(), &project_dir)?;
    let prepared = take_prepared(&reviews.prepared, id, true)
        .ok_or_else(|| "That request is no longer waiting to be sent. Ask again.".to_string())?;
    let PreparedRequest {
        provider,
        prompt,
        purpose,
        ..
    } = prepared;
    let Purpose::Explain { signature } = purpose else {
        unreachable!("take_prepared(.., true) only returns an explanation");
    };

    let key_store: Arc<dyn KeyStore> = Arc::clone(&keys.0);
    let reply = tauri::async_runtime::spawn_blocking(move || {
        Assistant::new().and_then(|assistant| assistant.complete(&provider, key_store.as_ref(), &prompt))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    let mut answer = reply.text.trim().to_string();
    if answer.is_empty() {
        return Err("The assistant had nothing to say about that one.".to_string());
    }
    if reply.truncated {
        answer.push_str(" …");
    } else {
        // A cut-off answer is not remembered: asking again might finish it.
        let _ = remember_answer(&cache_dir(&project_dir), &signature, &answer);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "(/home/ada/thesis/main.tex\n! Package pgfplots Error: Could not read /home/ada/thesis/data/missing.dat.\n\nl.14 \\addplot table {data/missing.dat}\n\n";

    #[test]
    fn the_project_folder_is_taken_out_of_what_is_sent_in_either_slash_direction() {
        let dir = Path::new("/home/ada/thesis");
        let excerpt = excerpt_for(LOG, "Package pgfplots Error: Could not read", dir).unwrap();
        assert!(!excerpt.contains("/home/ada"), "{excerpt}");
        assert!(excerpt.contains("<project>/data/missing.dat"), "{excerpt}");

        let windows = Path::new("C:\\Users\\ada\\thesis");
        let log = "! Error in C:/Users/ada/thesis/x.tex and C:\\Users\\ada\\thesis\\y.tex\nl.1 z\n";
        let cleaned = excerpt_for(log, "Error in", windows).unwrap();
        assert!(!cleaned.contains("ada"), "{cleaned}");
    }

    #[test]
    fn an_answer_is_remembered_by_the_problem_and_not_by_the_line_it_was_on() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_dir(dir.path());
        let first = signature(&excerpt_for(LOG, "Package pgfplots Error", Path::new("/p")).unwrap());
        assert_eq!(cached_answer(&cache, &first), None);

        assert!(remember_answer(&cache, &first, "The data file is missing."));
        assert!(cache.starts_with(dir.path().join(".abstract-tex")), "{cache:?}");

        let moved_log = LOG.replace("l.14", "l.209");
        let again = signature(&excerpt_for(&moved_log, "Package pgfplots Error", Path::new("/p")).unwrap());
        assert_eq!(
            cached_answer(&cache, &again).as_deref(),
            Some("The data file is missing."),
            "same problem, other line"
        );
        assert_eq!(cached_answer(&cache, "some other problem"), None);
    }

    #[test]
    fn an_entry_whose_signature_differs_is_a_miss_not_a_wrong_answer() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_dir(dir.path());
        std::fs::create_dir_all(&cache).unwrap();
        // As if a different problem hashed to this file name.
        let entry = CacheEntry {
            signature: "problem A".into(),
            answer: "answer for A".into(),
        };
        std::fs::write(
            cache.join(format!("{}.json", cache_key("problem B"))),
            serde_json::to_string(&entry).unwrap(),
        )
        .unwrap();
        assert_eq!(cached_answer(&cache, "problem B"), None);
    }

    #[test]
    fn a_damaged_cache_file_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_dir(dir.path());
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join(format!("{}.json", cache_key("p"))), "{ not json").unwrap();
        assert_eq!(cached_answer(&cache, "p"), None);
    }

    #[test]
    fn what_is_sent_for_an_error_is_its_log_lines_and_nothing_else() {
        let log = format!("{LOG}UNRELATED LATER LINE\n(/home/ada/thesis/other.tex\n");
        let excerpt = excerpt_for(&log, "Package pgfplots Error", Path::new("/home/ada/thesis")).unwrap();
        let prompt = build_explain_prompt(&excerpt).unwrap();
        let provider = abstract_tex_assistant::Provider::anthropic("claude-x");
        let shown = inspect(&provider, true, &prompt).unwrap();
        assert!(shown.body.contains("Could not read"));
        assert!(!shown.body.contains("UNRELATED"), "stops after the l.NN context");
        assert!(
            !shown.body.contains("ada"),
            "and carries no part of the folder's path"
        );
        assert_eq!(shown.parts.len(), 2, "instructions and the excerpt, nothing more");
    }
}
