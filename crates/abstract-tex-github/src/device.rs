//! The OAuth device flow: three calls and a state machine.
//!
//! Owns the exchange with GitHub and the rules for waiting. It never stores anything — the token
//! it produces is handed back to the caller, who puts it in a [`crate::store::SecretStore`] —
//! and it never decides what the person sees.
//!
//! The flow, as GitHub documents it:
//!
//! 1. `POST /login/device/code` with the client id and scope. Back comes a short `user_code` for
//!    the person, a `device_code` for us, the URL to send them to, how often we may poll, and
//!    how long before all of it expires.
//! 2. `POST /login/oauth/access_token` every `interval` seconds until it answers something other
//!    than "not yet".
//! 3. `GET /user` once, with the token, so the app can say whose account it is.

use std::time::Duration;

use serde::Deserialize;

use crate::{Account, GitHubError, SCOPE};

/// Where the three calls go.
///
/// A struct rather than three constants so a test can point the whole flow at a fake GitHub on
/// localhost. There is no other reason for it to exist, and no caller in the app ever builds one:
/// [`Endpoints::github`] is the real thing.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub device_code: String,
    pub access_token: String,
    pub user: String,
    /// `POST` here creates a repository (S10.5b). Not part of the device flow, but the same
    /// struct, so a test points everything at one fake GitHub rather than two.
    pub repos: String,
}

impl Endpoints {
    pub fn github() -> Self {
        Self {
            device_code: "https://github.com/login/device/code".to_string(),
            access_token: "https://github.com/login/oauth/access_token".to_string(),
            user: "https://api.github.com/user".to_string(),
            repos: "https://api.github.com/user/repos".to_string(),
        }
    }

    /// The same three paths under some other origin — `http://127.0.0.1:PORT` in the tests.
    pub fn under(origin: &str) -> Self {
        Self {
            device_code: format!("{origin}/login/device/code"),
            access_token: format!("{origin}/login/oauth/access_token"),
            user: format!("{origin}/user"),
            repos: format!("{origin}/user/repos"),
        }
    }
}

/// What GitHub hands back at step 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    /// The short code the person types into their browser, as GitHub formats it (`WDJB-MJHT`).
    pub user_code: String,
    /// Where they type it.
    pub verification_uri: String,
    /// Ours, and never shown: it is the half of the pair that proves the poll is from this app.
    pub device_code: String,
    /// The shortest interval GitHub will accept polls at. Not a suggestion.
    pub interval: Duration,
    /// How long the code is good for — about fifteen minutes, in practice.
    pub expires_in: Duration,
}

/// What one poll means. Five answers, not two: a flow that collapses them gets rate-limited into
/// an expiry that looks like our own bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// Nobody has typed the code in yet. Wait the interval and ask again.
    Pending,
    /// We polled too fast. The new interval is **mandatory** — GitHub adds five seconds each
    /// time it has to say this, and keeps refusing until we obey.
    SlowDown(Duration),
    /// Signed in. This is the only value that carries a token, and it goes straight to the
    /// keychain.
    Token(String),
}

/// The flow itself. One `reqwest` client, reused: a new one per call would redo TLS setup each
/// time for no benefit.
pub struct DeviceFlow {
    client: reqwest::blocking::Client,
    endpoints: Endpoints,
}

impl DeviceFlow {
    /// Against the real GitHub.
    pub fn new() -> Result<Self, GitHubError> {
        Self::with_endpoints(Endpoints::github())
    }

    pub fn with_endpoints(endpoints: Endpoints) -> Result<Self, GitHubError> {
        Ok(Self {
            client: crate::http_client()?,
            endpoints,
        })
    }

    /// Step 1: ask for a code.
    pub fn request_code(&self, client_id: &str) -> Result<DeviceCode, GitHubError> {
        let response = self
            .client
            .post(&self.endpoints.device_code)
            // Without this GitHub answers in `application/x-www-form-urlencoded`, which is
            // valid, documented and needs a second parser for no reason.
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[("client_id", client_id), ("scope", SCOPE)])
            .send()?;

        // Not `error_for_status`: GitHub's own error is in the body even on a 404 (that is what
        // an unregistered client id answers with), and its words are more useful than the code.
        let body = response.text()?;
        let parsed: DeviceCodeBody = read(&body)?;
        if let Some(error) = parsed.error_sentence() {
            return Err(GitHubError::GitHub(error));
        }
        let (Some(user_code), Some(device_code), Some(verification_uri)) =
            (parsed.user_code, parsed.device_code, parsed.verification_uri)
        else {
            return Err(GitHubError::Unreadable(body));
        };
        Ok(DeviceCode {
            user_code,
            verification_uri,
            device_code,
            // GitHub's documented default is 5 seconds; a missing field is not a reason to poll
            // as fast as we like.
            interval: Duration::from_secs(parsed.interval.unwrap_or(5)),
            expires_in: Duration::from_secs(parsed.expires_in.unwrap_or(900)),
        })
    }

    /// Step 2: one poll. The caller sleeps for the interval between calls, and lengthens it when
    /// this answers [`Poll::SlowDown`].
    pub fn poll(&self, client_id: &str, device_code: &str) -> Result<Poll, GitHubError> {
        let response = self
            .client
            .post(&self.endpoints.access_token)
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                // The grant type is what makes this a device-flow token request rather than an
                // authorisation-code one, and GitHub rejects the request without it.
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()?;

        let body = response.text()?;
        let parsed: TokenBody = read(&body)?;
        if let Some(token) = parsed.access_token {
            return Ok(Poll::Token(token));
        }
        match parsed.error.as_deref() {
            Some("authorization_pending") => Ok(Poll::Pending),
            // The body's own interval when it gives one, and otherwise five seconds more, which
            // is what GitHub does to its own minimum each time it says this.
            Some("slow_down") => Ok(Poll::SlowDown(Duration::from_secs(parsed.interval.unwrap_or(10)))),
            Some("access_denied") => Err(GitHubError::Denied),
            Some("expired_token") => Err(GitHubError::Expired),
            Some(_) => Err(GitHubError::GitHub(
                parsed.error_sentence().unwrap_or_else(|| body.clone()),
            )),
            None => Err(GitHubError::Unreadable(body)),
        }
    }

    /// Step 3: whose account this is.
    ///
    /// Also the only way to find out that a token kept in the keychain has stopped working: a
    /// person can revoke it on github.com, and nothing tells the app. `401` is therefore its own
    /// error ([`GitHubError::TokenRejected`]) rather than an unreadable body, because the app's
    /// answer to it is to forget the token rather than to show a sentence about JSON.
    pub fn account(&self, token: &str) -> Result<Account, GitHubError> {
        let response = self
            .client
            .get(&self.endpoints.user)
            .bearer_auth(token)
            // The version header GitHub's documentation asks every caller to send, so that a
            // future default cannot change what this parses.
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GitHubError::TokenRejected);
        }
        let body = response.text()?;
        let account: Account = read(&body)?;
        if account.login.is_empty() {
            return Err(GitHubError::Unreadable(body));
        }
        Ok(account)
    }
}

/// `serde_json::from_str`, with the body kept in the error so a surprise is diagnosable.
fn read<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, GitHubError> {
    serde_json::from_str(body).map_err(|_| GitHubError::Unreadable(body.chars().take(200).collect()))
}

/// Step 1's body, with every field optional because an error answer has none of them.
#[derive(Debug, Deserialize)]
struct DeviceCodeBody {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    interval: Option<u64>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Step 2's body. The same shape, minus the code fields.
#[derive(Debug, Deserialize)]
struct TokenBody {
    access_token: Option<String>,
    interval: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// GitHub sends `error` (a slug) and usually `error_description` (a sentence for a person). The
/// sentence when there is one, because "incorrect_client_credentials" is not something to show
/// somebody; the slug when there is not, because it is better than nothing.
trait ErrorSentence {
    fn error_sentence(&self) -> Option<String>;
}

macro_rules! error_sentence_from_fields {
    ($type:ty) => {
        impl ErrorSentence for $type {
            fn error_sentence(&self) -> Option<String> {
                let error = self.error.as_deref()?;
                Some(
                    self.error_description
                        .clone()
                        .unwrap_or_else(|| error.to_string()),
                )
            }
        }
    };
}
error_sentence_from_fields!(DeviceCodeBody);
error_sentence_from_fields!(TokenBody);
