//! How the writing assistant talks to a model (S12.1a, DESIGN.md §5.5).
//!
//! Owns four things and no more: which providers exist and what their requests and answers look
//! like on the wire ([`provider`]), where their keys live ([`keys`]), the one function that
//! actually sends something ([`client`]), and the typed errors all three share. It knows nothing
//! about Tauri, nothing about a window, nothing about a manuscript, and nothing about what a
//! *good* edit is — choosing the text to send and judging what comes back is S12.2 and S12.3.
//!
//! **What this crate must never do:**
//!
//! - Never make a request on its own. The only code here that touches the network is
//!   [`client::Assistant::complete_with_key`], and its callers are a person's explicit action
//!   (DESIGN.md §5.5: "nothing is sent without an explicit action"; §8: "no silent data paths").
//!   Building a request ([`provider::build_request`]) and reading an answer
//!   ([`provider::parse_response`]) are pure, which is also what makes every wire detail testable
//!   with no socket.
//! - Never put a key anywhere but the [`keys::KeyStore`], and never send one anywhere but over
//!   `https` or to an address on this computer. Not into a log line, an error message, or `Debug`
//!   output either: [`provider::HttpRequest`] hides its credential headers when printed.
//! - Never depend on a provider being present. The assistant is an addition to an editor that is
//!   whole without it (§2 commitment 6: every AI feature has a non-AI path).
//!
//! **The error split**, as `crates/abstract-tex-engine/src/lib.rs` explains once: a library crate
//! returns a typed `thiserror` enum so a caller can tell a refused key from a rate limit; only the
//! app's edge flattens them into `anyhow`. Every message here is a sentence a person can act on.

pub mod client;
pub mod edit;
pub mod guard;
pub mod inspect;
pub mod keys;
pub mod provider;

pub use client::Assistant;
pub use edit::{build_prompt, hunks, proposed_selection, Action, ApplyError, Hunk, Review};
pub use guard::{citation_macros, Finding, FindingKind, Guard, Verdict};
pub use inspect::{inspect, Payload, PayloadHeader, PayloadPart};
pub use keys::{KeyStore, Keychain, MemoryKeyStore};
pub use provider::{
    build_request, parse_response, HttpRequest, Message, Prompt, Provider, Reply, Role, SystemPart, Usage,
};

/// Everything that can go wrong between "ask the model" and "here is its text".
#[derive(Debug, thiserror::Error)]
pub enum AssistantError {
    #[error("{provider} needs an API key, and none is saved on this machine. Add one in the assistant's settings.")]
    MissingKey { provider: String },

    #[error(
        "\"{0}\" is not a web address the assistant can use. It should look like https://api.example.com/v1."
    )]
    BadAddress(String),

    #[error(
        "That address is plain http, and an API key must not cross a network unencrypted. Use an https \
         address, or one on this computer (localhost)."
    )]
    InsecureAddress,

    #[error("There is nothing to send: the request has no message in it.")]
    EmptyPrompt,

    #[error("The provider did not accept the key. Check it in the assistant's settings.")]
    KeyRefused,

    #[error("The provider is limiting how fast this account can ask. Wait a minute and try again.")]
    RateLimited,

    #[error(
        "The provider is having trouble at the moment (it answered with status {status}). Try again shortly."
    )]
    ProviderTrouble { status: u16 },

    #[error("The provider turned the request down: {message}")]
    Rejected { message: String },

    #[error("The provider answered, but not in a shape this app understands. Is the address really a chat endpoint?")]
    UnreadableAnswer,

    #[error("The provider answered with no text.")]
    EmptyAnswer,

    #[error(
        "The model ran out of room before it finished, so nothing was changed. Try a shorter selection, or ask again."
    )]
    CutOff,

    #[error("Could not reach the provider: {0}")]
    Network(String),

    #[error("The operating system's keychain said: {0}")]
    Keychain(String),
}

impl From<reqwest::Error> for AssistantError {
    fn from(error: reqwest::Error) -> Self {
        // `without_url`: a request error normally prints the address it was sent to, and an
        // address is the one place a mistaken paste of "key in the URL" could end up in a message.
        Self::Network(error.without_url().to_string())
    }
}
