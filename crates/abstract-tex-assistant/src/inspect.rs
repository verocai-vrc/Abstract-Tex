//! What a request would carry out of the machine, laid out for a person to read (S13.3, DESIGN.md
//! §5.5: "the exact payload is inspectable before it leaves the machine").
//!
//! Owns [`Payload`]: where a request is going, which model it names, every piece of text in it, the
//! header names it will send, and the body byte for byte. It is built from the *same*
//! [`HttpRequest`] that [`crate::client`] sends, by the same [`build_request`], so it cannot show
//! one thing and send another; the test at the bottom parses the body and fails if it holds any
//! text the parts list does not account for.
//!
//! **What it must never do:** send anything, or hold a credential. A credential header is listed by
//! name with its value replaced by a word, and the body never has one in it (a key travels in a
//! header only — see `provider.rs`).

use serde::Serialize;

use crate::provider::{build_request, HttpRequest, Prompt, Provider, Role};
use crate::AssistantError;

/// Shown in place of a credential's value.
const HIDDEN: &str = "(your key, sent, never shown)";

/// One piece of text in the request, in the order the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadPart {
    /// `system` (instructions, and any context the model reads first), `user` or `assistant`.
    pub role: &'static str,
    pub text: String,
    /// The provider is asked to remember everything up to and including this part for next time.
    pub cached: bool,
}

/// A header the request will carry. A credential's value is [`HIDDEN`], not the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayloadHeader {
    pub name: String,
    pub value: String,
}

/// The whole outgoing request, as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    /// The host the request goes to, with its port when it has one: what a person recognises.
    pub destination: String,
    pub url: String,
    pub model: String,
    pub parts: Vec<PayloadPart>,
    pub headers: Vec<PayloadHeader>,
    /// Exactly the bytes of the request body.
    pub body: String,
    /// Characters of text across all parts.
    pub characters: usize,
    /// A guess at the token count: a third of the characters rounded up, which over-counts
    /// English and under-counts nothing a person would be surprised by. A guess, and labelled one.
    pub approximate_tokens: usize,
    /// Whether the destination is this computer.
    pub stays_on_this_computer: bool,
}

/// What `prompt` would send to `provider`, or why it could not be sent.
///
/// `will_send_a_key` says whether a credential header will be on the real request. The key's value
/// is not needed (and is not asked for): a stand-in goes through [`build_request`] so the same
/// checks run, and the stand-in is never shown.
pub fn inspect(
    provider: &Provider,
    will_send_a_key: bool,
    prompt: &Prompt,
) -> Result<Payload, AssistantError> {
    let request = build_request(provider, will_send_a_key.then_some("stand-in"), prompt)?;
    Ok(payload_of(provider, prompt, request))
}

fn payload_of(provider: &Provider, prompt: &Prompt, request: HttpRequest) -> Payload {
    let mut parts: Vec<PayloadPart> = prompt
        .system
        .iter()
        .map(|part| PayloadPart {
            role: "system",
            text: part.text.clone(),
            cached: part.cache_breakpoint,
        })
        .collect();
    parts.extend(prompt.messages.iter().map(|message| PayloadPart {
        role: match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        },
        text: message.text.clone(),
        cached: false,
    }));

    let url = reqwest::Url::parse(&request.url).ok();
    let destination = url
        .as_ref()
        .and_then(|url| {
            let host = url.host_str()?;
            Some(match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            })
        })
        .unwrap_or_else(|| request.url.clone());
    let stays_on_this_computer = url.as_ref().is_some_and(crate::provider::is_on_this_computer);

    let headers = request
        .headers
        .iter()
        .map(|(name, value)| PayloadHeader {
            name: name.clone(),
            value: if crate::provider::is_secret_header(name) {
                HIDDEN.to_string()
            } else {
                value.clone()
            },
        })
        .collect();

    let characters = parts.iter().map(|part| part.text.chars().count()).sum();
    Payload {
        destination,
        url: request.url,
        model: provider.model().to_string(),
        parts,
        headers,
        body: request.body,
        characters,
        approximate_tokens: characters.div_ceil(3),
        stays_on_this_computer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Message, SystemPart};
    use serde_json::Value;

    fn prompt() -> Prompt {
        Prompt {
            system: vec![
                SystemPart::plain("You tighten prose."),
                SystemPart::cached("<document>\nThe whole \"manuscript\".\n</document>"),
            ],
            messages: vec![Message::user("<selection>\nTighten é𝒳 this.\n</selection>")],
            max_tokens: 300,
        }
    }

    fn every_string(value: &Value, into: &mut Vec<String>) {
        match value {
            Value::String(text) => into.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|item| every_string(item, into)),
            Value::Object(fields) => fields.values().for_each(|item| every_string(item, into)),
            _ => {}
        }
    }

    /// The point of the module. Any text that is in the body must be a part the person was shown,
    /// the model's name, or a word of the wire format. A field added to a request later — an
    /// account id, a hidden instruction, a tag — fails here until it is also shown.
    fn assert_nothing_unaccounted_for(provider: &Provider, will_send_a_key: bool) {
        let payload = inspect(provider, will_send_a_key, &prompt()).unwrap();
        let mut found = Vec::new();
        every_string(&serde_json::from_str(&payload.body).unwrap(), &mut found);

        // OpenAI-style requests join the system parts into one message.
        let joined_system = payload
            .parts
            .iter()
            .filter(|part| part.role == "system")
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let wire_words = ["system", "user", "assistant", "text", "ephemeral"];
        for text in &found {
            let shown = payload.parts.iter().any(|part| part.text == *text)
                || *text == joined_system
                || *text == payload.model
                || wire_words.contains(&text.as_str());
            assert!(
                shown,
                "the body carries text the inspector does not show: {text:?}"
            );
        }
        // And the other way: everything shown is really in the body.
        for part in &payload.parts {
            assert!(
                found
                    .iter()
                    .any(|text| text == &part.text || text.contains(&part.text)),
                "a shown part is not in the body: {part:?}"
            );
        }
    }

    #[test]
    fn both_wire_formats_carry_nothing_the_inspector_does_not_show() {
        assert_nothing_unaccounted_for(&Provider::anthropic("claude-x"), true);
        assert_nothing_unaccounted_for(
            &Provider::openai_compatible("https://api.example.com/v1", "m"),
            true,
        );
        assert_nothing_unaccounted_for(
            &Provider::openai_compatible("http://localhost:11434/v1", "m"),
            false,
        );
    }

    #[test]
    fn it_is_the_body_that_would_be_sent_byte_for_byte() {
        let provider = Provider::anthropic("claude-x");
        let shown = inspect(&provider, true, &prompt()).unwrap();
        let real = build_request(&provider, Some("sk-the-real-key"), &prompt()).unwrap();
        assert_eq!(shown.body, real.body);
        assert_eq!(shown.url, real.url);
        let names =
            |headers: &[(String, String)]| headers.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
        assert_eq!(
            shown
                .headers
                .iter()
                .map(|header| header.name.clone())
                .collect::<Vec<_>>(),
            names(&real.headers)
        );
    }

    #[test]
    fn a_key_is_named_but_never_shown() {
        let provider = Provider::anthropic("claude-x");
        let payload = inspect(&provider, true, &prompt()).unwrap();
        let key_header = payload
            .headers
            .iter()
            .find(|header| header.name == "x-api-key")
            .unwrap();
        assert_eq!(key_header.value, HIDDEN);
        let everything = serde_json::to_string(&payload).unwrap();
        assert!(!everything.contains("stand-in"), "{everything}");

        let bearer = inspect(
            &Provider::openai_compatible("https://api.example.com/v1", "m"),
            true,
            &prompt(),
        )
        .unwrap();
        assert!(bearer
            .headers
            .iter()
            .any(|header| header.name == "authorization" && header.value == HIDDEN));
    }

    #[test]
    fn a_local_model_with_no_key_sends_no_credential_header_and_says_it_stays_here() {
        let payload = inspect(
            &Provider::openai_compatible("http://localhost:11434/v1", "llama"),
            false,
            &prompt(),
        )
        .unwrap();
        assert!(payload
            .headers
            .iter()
            .all(|header| header.name != "authorization" && header.name != "x-api-key"));
        assert!(payload.stays_on_this_computer);
        assert_eq!(payload.destination, "localhost:11434");

        let remote = inspect(&Provider::anthropic("claude-x"), true, &prompt()).unwrap();
        assert!(!remote.stays_on_this_computer);
        assert_eq!(remote.destination, "api.anthropic.com");
    }

    #[test]
    fn the_parts_are_in_reading_order_with_the_cached_one_marked() {
        let payload = inspect(&Provider::anthropic("claude-x"), true, &prompt()).unwrap();
        let roles: Vec<&str> = payload.parts.iter().map(|part| part.role).collect();
        assert_eq!(roles, ["system", "system", "user"]);
        let cached: Vec<bool> = payload.parts.iter().map(|part| part.cached).collect();
        assert_eq!(cached, [false, true, false]);
        let expected: usize = payload.parts.iter().map(|part| part.text.chars().count()).sum();
        assert_eq!(payload.characters, expected);
        assert_eq!(payload.approximate_tokens, expected.div_ceil(3));
    }

    #[test]
    fn a_request_that_could_not_be_sent_cannot_be_inspected_either() {
        // The same checks, so the dialog never offers a Send that would be refused.
        let insecure = Provider::openai_compatible("http://models.example.com/v1", "m");
        assert!(matches!(
            inspect(&insecure, true, &prompt()),
            Err(AssistantError::InsecureAddress)
        ));
        assert!(matches!(
            inspect(&Provider::anthropic("claude-x"), false, &prompt()),
            Err(AssistantError::MissingKey { .. })
        ));
    }
}
