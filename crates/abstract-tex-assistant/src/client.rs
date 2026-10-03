//! The one place this crate touches the network (S12.1a, DESIGN.md §5.5, §8).
//!
//! Owns sending a built request and handing the answer to [`crate::provider::parse_response`].
//! It never builds a prompt, never chooses a provider and never retries: a model call costs
//! money and a person is watching, so a failure is reported once, in a sentence, and the person
//! decides whether to ask again.
//!
//! **It is only ever called because a person asked.** There is no constructor that starts
//! anything, no background task, no "warm-up" request. `Assistant::new` opens no connection.

use std::time::Duration;

use crate::keys::KeyStore;
use crate::provider::{build_request, parse_response, Prompt, Provider, Reply};
use crate::AssistantError;

/// Longest a model call may take. A rewrite of a long paragraph can take most of a minute; past
/// two the connection is more likely dead than thinking.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Assistant {
    http: reqwest::blocking::Client,
}

impl Assistant {
    pub fn new() -> Result<Self, AssistantError> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("abstract-tex/", env!("CARGO_PKG_VERSION")))
            .timeout(CALL_TIMEOUT)
            // A redirect would carry the key header to wherever it points. The providers this
            // talks to do not redirect an API call; one that does is misconfigured.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { http })
    }

    /// Ask, using the key saved for this provider (if any).
    pub fn complete(&self, provider: &Provider, keys: &dyn KeyStore, prompt: &Prompt) -> Result<Reply, AssistantError> {
        let key = keys.read(&provider.key_slot())?;
        self.complete_with_key(provider, key.as_deref(), prompt)
    }

    /// Ask, with this key. The request is built and checked first, so a refused address or a
    /// missing key fails before a socket is opened.
    pub fn complete_with_key(
        &self,
        provider: &Provider,
        key: Option<&str>,
        prompt: &Prompt,
    ) -> Result<Reply, AssistantError> {
        let request = build_request(provider, key, prompt)?;

        let mut call = self.http.post(&request.url).body(request.body);
        for (name, value) in &request.headers {
            call = call.header(name, value);
        }
        let response = call.send()?;
        let status = response.status().as_u16();
        let body = response.text()?;
        parse_response(provider, status, &body)
    }
}
