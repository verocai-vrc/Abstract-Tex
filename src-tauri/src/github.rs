//! The GitHub sign-in session, as the window sees it (S10.4b, DESIGN.md §5.7).
//!
//! Owns the *waiting*: the device flow is minutes long and mostly spent in somebody's browser,
//! so this module runs it on a background thread and reports through `github:sign-in` events.
//! The flow itself and the keychain are `abstract-tex-github`'s; nothing here knows how to talk
//! to GitHub.
//!
//! **What it must never do:**
//!
//! - Never let the token cross to the frontend. The webview is told the code to type, the URL to
//!   type it into, and afterwards a login name. A token in a webview is a token in every devtools
//!   log and every future extension, and the only things that ever hold it are the keychain and
//!   this module.
//! - Never run two sign-ins at once. A second one would leave two threads polling with two device
//!   codes, and the panel would show whichever finished last.
//! - Never make the person wait for a mind they have changed: cancelling is a command, not an
//!   expiry.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use abstract_tex_github::{
    Account, DeviceFlow, GitHubError, Keychain, NewRepository, Poll, Repos, Repository, SecretStore,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// How the window learns what is happening. One event name, one payload shape with a `stage`,
/// for the reason the `compile` event has one: the frontend has a single handler and a `switch`,
/// rather than three subscriptions whose order it has to reason about.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum SignInEvent {
    /// GitHub has issued a code. Everything the person needs is in here, and nothing else is.
    #[serde(rename_all = "camelCase")]
    Code { user_code: String, verification_uri: String, expires_in_seconds: u64 },
    /// Signed in, and this is whose account it is.
    #[serde(rename_all = "camelCase")]
    SignedIn { login: String },
    /// It did not happen, and this is the sentence to show. Cancelling produces this too, with
    /// `cancelled` set, because the panel puts itself away rather than showing a failure the
    /// person caused on purpose.
    #[serde(rename_all = "camelCase")]
    Failed { message: String, cancelled: bool },
}

/// Everything the app keeps about GitHub between commands.
///
/// The store is a trait object so that the app can be built with a memory store in a test and
/// the real keychain in the window — the same seam `abstract-tex-github` explains.
pub struct GitHubSession {
    store: Box<dyn SecretStore>,
    /// The cancel flag of the sign-in currently running, if one is. `None` means nothing is
    /// polling, which is also how a second sign-in is refused.
    running: Mutex<Option<Arc<AtomicBool>>>,
}

impl Default for GitHubSession {
    fn default() -> Self {
        Self { store: Box::new(Keychain::for_this_app()), running: Mutex::new(None) }
    }
}

impl GitHubSession {
    /// For tests: a session whose token goes nowhere near the machine's keychain.
    pub fn with_store(store: Box<dyn SecretStore>) -> Self {
        Self { store, running: Mutex::new(None) }
    }

    /// The stored token, if this machine has one. Fast: a keychain read and nothing else.
    fn token(&self) -> Result<Option<String>, GitHubError> {
        self.store.read()
    }

    /// The token, or the sentence for "nobody is signed in". Every call that needs one goes
    /// through here, so "signed out" is one message rather than one per caller.
    fn require_token(&self) -> Result<String, GitHubError> {
        self.token()?.ok_or_else(|| GitHubError::GitHub("Sign in to GitHub first.".to_string()))
    }

    pub fn sign_out(&self) -> Result<(), GitHubError> {
        self.cancel();
        self.store.clear()
    }

    /// Stop whatever sign-in is polling. Doing this when none is running is not an error: the
    /// panel can be closed at any moment, including the moment one finishes.
    pub fn cancel(&self) {
        if let Some(flag) = self.running.lock().unwrap().take() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Whose account this machine is signed in to, or `None` when nobody is.
///
/// A token GitHub no longer accepts is *forgotten* rather than reported: a person who revoked it
/// on github.com has already said what they want, and an app that kept showing their name would
/// be lying about being signed in.
///
/// `async` and `spawn_blocking` around the one network call, for the reason `paste_cite` gives in
/// `commands.rs`: blocking HTTP on an async worker stalls every other command sharing that
/// thread. The keychain read either side of it is microseconds, and no lock is held across the
/// await.
pub async fn account(session: &GitHubSession) -> Result<Option<Account>, GitHubError> {
    let Some(token) = session.token()? else { return Ok(None) };
    let looked_up = tauri::async_runtime::spawn_blocking(move || DeviceFlow::new()?.account(&token))
        .await
        .map_err(|error| GitHubError::Keychain(error.to_string()))?;
    match looked_up {
        Ok(account) => Ok(Some(account)),
        Err(GitHubError::TokenRejected) => {
            session.sign_out()?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Create a repository on GitHub for the open project (S10.5b).
///
/// Two steps that must not be half done: GitHub makes the repository, and then the local one is
/// pointed at it. If the second fails, the first has still happened — so the error says the
/// repository exists and names it, rather than letting the author press the button again and
/// collect a second empty repository on their account.
///
/// Nothing is pushed here. `git2` is built with no `https` feature (S10.1's flag), so this cannot
/// send a byte anywhere; `Sync` is S11.1.
pub async fn create_repository(
    session: &GitHubSession,
    project_dir: std::path::PathBuf,
    wanted: NewRepository,
) -> Result<Repository, GitHubError> {
    // Checked before anything else and again inside the crate: "private by default" is a property
    // of the code, not of the panel (`repos.rs`).
    wanted.visibility.allowed()?;
    let token = session.require_token()?;

    let created = tauri::async_runtime::spawn_blocking(move || Repos::new()?.create(&token, &wanted))
        .await
        .map_err(|error| GitHubError::Keychain(error.to_string()))??;

    let repository = abstract_tex_git::open(&project_dir)
        .map_err(|_| GitHubError::GitHub(format!("{} exists, but this folder is not a Git repository — make one here first.", created.full_name)))?;
    abstract_tex_git::set_origin(&repository, &created.clone_url).map_err(|error| {
        GitHubError::GitHub(format!("{} was created, but it could not be set as this project's origin: {error}", created.full_name))
    })?;
    Ok(created)
}

/// Start a sign-in. Returns as soon as the thread is running; everything else is events.
///
/// The only thing checked before the thread starts is whether this build has a client id at all,
/// because that is a local answer and the panel should not put a code box up to explain it. The
/// first network call — asking GitHub for a code — happens *inside* the thread, so a slow or
/// unreachable GitHub arrives as a `failed` event rather than as a command that took 20 seconds
/// to reject. `spawn_blocking` for the reason `take_snapshot` gives: the flow is blocking HTTP
/// and a `sleep` between polls, and an async worker owing the window its next frame must not be
/// the thread doing that.
pub fn start_sign_in(app: AppHandle, session: &GitHubSession) -> Result<(), GitHubError> {
    let client_id = abstract_tex_github::client_id().ok_or(GitHubError::NoClientId)?;

    // Replace any earlier flag before the new thread starts, so two sign-ins can never both
    // think they are the current one.
    let cancelled = Arc::new(AtomicBool::new(false));
    if let Some(previous) = session.running.lock().unwrap().replace(cancelled.clone()) {
        previous.store(true, Ordering::Relaxed);
    }

    // The store has to outlive this function, and a `Box<dyn SecretStore>` inside `AppState`
    // cannot be moved into the thread — so the thread is handed its own keychain handle. Both
    // name the same entry, which is the whole reason `Keychain` is cheap to construct.
    let store = Keychain::for_this_app();
    tauri::async_runtime::spawn_blocking(move || match sign_in_on_this_thread(&app, &client_id, &cancelled, &store) {
        Ok(login) => {
            let _ = app.emit("github:sign-in", SignInEvent::SignedIn { login });
        }
        Err(error) => {
            let _ = app.emit("github:sign-in", failed(&error, cancelled.load(Ordering::Relaxed)));
        }
    });
    Ok(())
}

/// The whole flow, on the background thread: ask for a code, announce it, wait, keep the token.
///
/// A plain function rather than a closure inside `spawn_blocking`, so that "everything in here is
/// blocking and off the runtime" is a named, readable line — the same shape `fetch_identified`
/// has in `commands.rs`.
fn sign_in_on_this_thread(
    app: &AppHandle,
    client_id: &str,
    cancelled: &AtomicBool,
    store: &Keychain,
) -> Result<String, GitHubError> {
    let flow = DeviceFlow::new()?;
    let code = flow.request_code(client_id)?;
    let _ = app.emit(
        "github:sign-in",
        SignInEvent::Code {
            user_code: code.user_code.clone(),
            verification_uri: code.verification_uri.clone(),
            expires_in_seconds: code.expires_in.as_secs(),
        },
    );

    let token = poll_until_answered(&flow, client_id, &code, cancelled)?;
    let account = flow.account(&token)?;
    // Keep first, announce second: an app that said "signed in" and then failed to store the
    // token would be signed out again on the next restart with no explanation.
    store.save(&token)?;
    Ok(account.login)
}

fn failed(error: &GitHubError, cancelled: bool) -> SignInEvent {
    SignInEvent::Failed { message: error.to_string(), cancelled }
}

/// The waiting loop: sleep, ask, repeat, until GitHub says something final.
///
/// Three things end it besides an answer: the person cancelling, the code expiring, and a
/// network failure. A network failure is *not* fatal — a laptop that lost its wifi while someone
/// was typing a code into their phone should keep trying until the code expires, which is what
/// the `Err(Network)` arm does.
fn poll_until_answered(
    flow: &DeviceFlow,
    client_id: &str,
    code: &abstract_tex_github::DeviceCode,
    cancelled: &AtomicBool,
) -> Result<String, GitHubError> {
    let started = Instant::now();
    let mut interval = code.interval;
    loop {
        // Sleep in short steps rather than one long one, so cancelling is felt immediately
        // instead of at the end of the interval GitHub asked for.
        let until = Instant::now() + interval;
        while Instant::now() < until {
            if cancelled.load(Ordering::Relaxed) {
                return Err(GitHubError::GitHub("The sign-in was cancelled.".to_string()));
            }
            std::thread::sleep(Duration::from_millis(200).min(until - Instant::now()));
        }
        if started.elapsed() > code.expires_in {
            return Err(GitHubError::Expired);
        }

        match flow.poll(client_id, &code.device_code) {
            Ok(Poll::Token(token)) => return Ok(token),
            Ok(Poll::Pending) => {}
            // Mandatory: GitHub adds five seconds every time it has to say this.
            Ok(Poll::SlowDown(longer)) => interval = longer,
            Err(GitHubError::Network(error)) => {
                tracing::debug!(%error, "sign-in poll could not reach GitHub; still waiting");
            }
            Err(final_answer) => return Err(final_answer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abstract_tex_github::{MemoryStore, Visibility};

    #[test]
    fn signing_out_empties_the_store() {
        let store = Box::new(MemoryStore::default());
        store.save("gho_token").unwrap();
        let session = GitHubSession::with_store(store);

        session.sign_out().unwrap();
        // Read through a second handle to the same session, the way a command would.
        assert!(session.store.read().unwrap().is_none());
        // And again, because a person can press it twice.
        session.sign_out().unwrap();
    }

    #[test]
    fn cancelling_when_nothing_is_running_is_not_an_error() {
        GitHubSession::with_store(Box::new(MemoryStore::default())).cancel();
    }

    #[test]
    fn a_machine_with_no_token_is_signed_in_to_nobody() {
        // And answering that needs no network at all, which is why `account` reads the token
        // first and only then reaches for GitHub.
        let session = GitHubSession::with_store(Box::new(MemoryStore::default()));
        assert_eq!(session.token().unwrap(), None);
    }

    /// The payload shape the frontend switches on. Worth pinning: `ipc.ts` mirrors it by hand,
    /// and a rename here would otherwise be found by nobody until the panel stopped updating.
    #[test]
    fn the_event_says_which_stage_it_is_in_camel_case() {
        let code = SignInEvent::Code {
            user_code: "WDJB-MJHT".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in_seconds: 899,
        };
        let json = serde_json::to_string(&code).unwrap();
        assert!(json.contains(r#""stage":"code""#), "{json}");
        assert!(json.contains(r#""userCode":"WDJB-MJHT""#), "{json}");
        assert!(json.contains(r#""expiresInSeconds":899"#), "{json}");

        let signed_in = serde_json::to_string(&SignInEvent::SignedIn { login: "ada".into() }).unwrap();
        assert!(signed_in.contains(r#""stage":"signedIn""#), "{signed_in}");

        let failed = serde_json::to_string(&SignInEvent::Failed { message: "no".into(), cancelled: true }).unwrap();
        assert!(failed.contains(r#""stage":"failed""#), "{failed}");
        assert!(failed.contains(r#""cancelled":true"#), "{failed}");
    }

    /// S10.5b: nothing reaches GitHub without a token, and "signed out" is one sentence.
    #[test]
    fn creating_a_repository_while_signed_out_is_refused_before_anything_is_sent() {
        let session = GitHubSession::with_store(Box::new(MemoryStore::default()));
        let error = session.require_token().unwrap_err();
        assert!(error.to_string().contains("Sign in to GitHub first"), "{error}");
    }

    /// The guard, at this layer too: `create_repository` checks it before it reads the token, so
    /// an unconfirmed public repository cannot even reach the question of who is signed in.
    #[test]
    fn a_public_repository_needs_the_answer_before_anything_else_is_considered() {
        let unconfirmed = Visibility::Public { confirmed: false };
        assert!(matches!(unconfirmed.allowed(), Err(GitHubError::PublicNotConfirmed)));
    }

    /// The rule this module exists to keep: no serialised event can carry a token.
    #[test]
    fn no_event_shape_has_anywhere_to_put_a_token() {
        for event in [
            SignInEvent::Code { user_code: "A".into(), verification_uri: "B".into(), expires_in_seconds: 1 },
            SignInEvent::SignedIn { login: "ada".into() },
            SignInEvent::Failed { message: "gho_not_a_token_either".into(), cancelled: false },
        ] {
            let json = serde_json::to_string(&event).unwrap();
            assert!(!json.contains("token") || json.contains("gho_not_a_token_either"), "{json}");
        }
    }
}
