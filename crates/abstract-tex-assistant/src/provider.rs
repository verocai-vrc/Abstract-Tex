//! The two wire formats, as pure functions (S12.1a, DESIGN.md §5.5).
//!
//! Owns what a request to each provider looks like and how to read each provider's answer. It
//! opens no socket: [`build_request`] turns a [`Prompt`] into an [`HttpRequest`] value and
//! [`parse_response`] turns a status and a body into a [`Reply`], so every detail that breaks in
//! practice — which header carries the key, where the cache breakpoint goes, what "cut off" is
//! called — is tested by comparing values, not by standing up a server.
//!
//! **What it must never do:** send anything (that is [`crate::client`]); print a credential
//! (see [`HttpRequest`]'s `Debug`); or let a key travel over plain http to a machine that is not
//! this one.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AssistantError;

/// Where Anthropic's Messages API lives. A field of [`Provider`] rather than a constant used
/// directly, so a test (or a proxy a company runs in front of it) can point somewhere else.
pub const ANTHROPIC_ADDRESS: &str = "https://api.anthropic.com";

/// The Messages API's version header. Pinned: the response shape this file reads is that version's.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Longest provider error text we repeat to a person. A server's error page can be a screenful.
const MESSAGE_LIMIT: usize = 300;

/// Which model service a request goes to. Serialisable, because the app edge keeps the choice in
/// its own settings file — never in the project (DESIGN.md §5.5) and never with the key in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Provider {
    /// Anthropic's Messages API. Needs a key.
    Anthropic { model: String, address: String },
    /// Anything that speaks OpenAI's `/chat/completions`: OpenAI itself, a company gateway, or a
    /// model on this computer (Ollama, LM Studio, a llama.cpp server). The address is the
    /// version root, e.g. `http://localhost:11434/v1`. A key is optional — a local model has none.
    OpenAiCompatible { model: String, address: String },
}

impl Provider {
    pub fn anthropic(model: &str) -> Self {
        Self::Anthropic { model: model.to_string(), address: ANTHROPIC_ADDRESS.to_string() }
    }

    pub fn openai_compatible(address: &str, model: &str) -> Self {
        Self::OpenAiCompatible { model: model.to_string(), address: address.to_string() }
    }

    /// What to call this provider in a sentence.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Anthropic { .. } => "Anthropic",
            Self::OpenAiCompatible { .. } => "This provider",
        }
    }

    pub fn model(&self) -> &str {
        match self {
            Self::Anthropic { model, .. } | Self::OpenAiCompatible { model, .. } => model,
        }
    }

    fn address(&self) -> &str {
        match self {
            Self::Anthropic { address, .. } | Self::OpenAiCompatible { address, .. } => address,
        }
    }

    /// The keychain slot this provider's key is filed under: one per service, and for the
    /// open-ended kind one per host, so a key for OpenAI is not offered to a gateway at work.
    pub fn key_slot(&self) -> String {
        match self {
            Self::Anthropic { .. } => "anthropic".to_string(),
            Self::OpenAiCompatible { address, .. } => {
                let host = reqwest::Url::parse(address)
                    .ok()
                    .and_then(|url| {
                        let host = url.host_str()?.to_string();
                        Some(match url.port() {
                            Some(port) => format!("{host}:{port}"),
                            None => host,
                        })
                    })
                    .unwrap_or_else(|| address.clone());
                format!("openai-compatible@{host}")
            }
        }
    }
}

/// One block of the system prompt. A breakpoint after a block asks the provider to remember
/// everything up to there, so a second call against the same forty-page manuscript re-reads it at
/// a fraction of the cost (§5.5). Anthropic honours it explicitly; OpenAI-style endpoints cache
/// by themselves and ignore it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPart {
    pub text: String,
    pub cache_breakpoint: bool,
}

impl SystemPart {
    pub fn plain(text: &str) -> Self {
        Self { text: text.to_string(), cache_breakpoint: false }
    }

    pub fn cached(text: &str) -> Self {
        Self { text: text.to_string(), cache_breakpoint: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Self { role: Role::User, text: text.to_string() }
    }
}

/// What to ask. Provider-neutral: the same value builds either wire format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: Vec<SystemPart>,
    pub messages: Vec<Message>,
    /// The longest answer to accept, in tokens. Required by Anthropic, and a ceiling on cost
    /// everywhere else.
    pub max_tokens: u32,
}

/// A request, ready to send and not yet sent.
///
/// The body is exactly the payload that leaves the machine, which is what the payload inspector
/// (S13.3) will show; credentials travel only in `headers`, so the body can be shown, logged or
/// cached without containing a key.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Header names whose values are credentials, in lower case.
const SECRET_HEADERS: [&str; 3] = ["x-api-key", "authorization", "proxy-authorization"];

impl std::fmt::Debug for HttpRequest {
    /// A hand-written `Debug`, because the derived one would print the key into any test failure,
    /// log line or panic message that mentions a request.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let secret = SECRET_HEADERS.contains(&name.to_ascii_lowercase().as_str());
                (name.as_str(), if secret { "<hidden>" } else { value.as_str() })
            })
            .collect();
        formatter
            .debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &shown)
            .field("body", &self.body)
            .finish()
    }
}

/// What the model said, and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    /// The model ran out of room mid-answer. A diff built from a cut-off rewrite would silently
    /// drop the end of a paragraph, so the caller must be able to tell.
    pub truncated: bool,
    pub usage: Usage,
}

/// Token counts, normalised across the two formats: `input_tokens` never includes the cached
/// ones, so the three input figures add up to what was sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}

/// Turn a prompt into the request for this provider, or say why it must not be sent.
///
/// The checks run before anything is built, in the order a person would want to hear them: the
/// prompt has something in it, the address is usable, the key is there when it must be, and the
/// key is not about to cross a network in the clear.
pub fn build_request(provider: &Provider, key: Option<&str>, prompt: &Prompt) -> Result<HttpRequest, AssistantError> {
    if prompt.messages.is_empty() {
        return Err(AssistantError::EmptyPrompt);
    }
    let key = key.map(str::trim).filter(|key| !key.is_empty());
    if matches!(provider, Provider::Anthropic { .. }) && key.is_none() {
        return Err(AssistantError::MissingKey { provider: provider.label().to_string() });
    }
    let base = checked_address(provider.address(), key.is_some())?;

    match provider {
        Provider::Anthropic { model, .. } => {
            let system: Vec<Value> = prompt
                .system
                .iter()
                .map(|part| {
                    let mut block = json!({ "type": "text", "text": part.text });
                    if part.cache_breakpoint {
                        block["cache_control"] = json!({ "type": "ephemeral" });
                    }
                    block
                })
                .collect();
            let mut body = json!({
                "model": model,
                "max_tokens": prompt.max_tokens,
                "messages": prompt.messages.iter().map(|m| json!({ "role": role_name(m.role), "content": m.text })).collect::<Vec<_>>(),
            });
            if !system.is_empty() {
                body["system"] = Value::Array(system);
            }
            Ok(HttpRequest {
                url: format!("{base}/v1/messages"),
                headers: vec![
                    ("content-type".into(), "application/json".into()),
                    ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
                    ("x-api-key".into(), key.unwrap_or_default().to_string()),
                ],
                body: body.to_string(),
            })
        }
        Provider::OpenAiCompatible { model, .. } => {
            // This format has one system message; the blocks are joined, and the breakpoints
            // dropped because these endpoints cache a repeated prefix on their own.
            let mut messages = Vec::new();
            if !prompt.system.is_empty() {
                let joined: Vec<&str> = prompt.system.iter().map(|part| part.text.as_str()).collect();
                messages.push(json!({ "role": "system", "content": joined.join("\n\n") }));
            }
            messages.extend(prompt.messages.iter().map(|m| json!({ "role": role_name(m.role), "content": m.text })));
            let body = json!({ "model": model, "max_tokens": prompt.max_tokens, "messages": messages });

            let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
            if let Some(key) = key {
                headers.push(("authorization".into(), format!("Bearer {key}")));
            }
            Ok(HttpRequest { url: format!("{base}/chat/completions"), headers, body: body.to_string() })
        }
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

/// The address with no trailing slash, if it is a web address at all — and, when a key will be
/// sent with it, only if that key will not cross a network unencrypted.
fn checked_address(address: &str, sending_a_key: bool) -> Result<String, AssistantError> {
    let url = reqwest::Url::parse(address.trim()).map_err(|_| AssistantError::BadAddress(address.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(AssistantError::BadAddress(address.to_string()));
    }
    if sending_a_key && url.scheme() == "http" && !is_on_this_computer(&url) {
        return Err(AssistantError::InsecureAddress);
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// `localhost`, or an address that is this computer's own (`127.0.0.1`, `::1`). A name that merely
/// *resolves* there cannot be told from here, so it is not trusted.
fn is_on_this_computer(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else { return false };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // An IPv6 literal comes back in brackets: `[::1]`.
    host.trim_start_matches('[').trim_end_matches(']').parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

/// Read a provider's answer. `status` and `body` are what came back, whatever they were.
pub fn parse_response(provider: &Provider, status: u16, body: &str) -> Result<Reply, AssistantError> {
    match status {
        200..=299 => {}
        401 | 403 => return Err(AssistantError::KeyRefused),
        429 => return Err(AssistantError::RateLimited),
        500..=599 => return Err(AssistantError::ProviderTrouble { status }),
        _ => return Err(AssistantError::Rejected { message: error_text(body, status) }),
    }
    let answer: Value = serde_json::from_str(body).map_err(|_| AssistantError::UnreadableAnswer)?;
    let reply = match provider {
        Provider::Anthropic { .. } => read_anthropic(&answer)?,
        Provider::OpenAiCompatible { .. } => read_openai(&answer)?,
    };
    if reply.text.trim().is_empty() {
        return Err(AssistantError::EmptyAnswer);
    }
    Ok(reply)
}

fn read_anthropic(answer: &Value) -> Result<Reply, AssistantError> {
    let blocks = answer.get("content").and_then(Value::as_array).ok_or(AssistantError::UnreadableAnswer)?;
    // Only `text` blocks: a model may also return thinking or tool blocks, which are not prose.
    let text: String = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    let usage = answer.get("usage");
    Ok(Reply {
        text,
        truncated: answer.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
        usage: Usage {
            input_tokens: count(usage, "input_tokens"),
            output_tokens: count(usage, "output_tokens"),
            cache_read_tokens: count(usage, "cache_read_input_tokens"),
            cache_write_tokens: count(usage, "cache_creation_input_tokens"),
        },
    })
}

fn read_openai(answer: &Value) -> Result<Reply, AssistantError> {
    let choice = answer.get("choices").and_then(|c| c.get(0)).ok_or(AssistantError::UnreadableAnswer)?;
    // `content` is `null` when a model answered with a tool call or was filtered; that is "no
    // text", reported as such by the caller, not an unreadable shape.
    let text = choice.get("message").and_then(|m| m.get("content")).and_then(Value::as_str).unwrap_or_default().to_string();
    let usage = answer.get("usage");
    let cached = usage
        .and_then(|u| u.get("prompt_tokens_details"))
        .map(|details| count(Some(details), "cached_tokens"))
        .unwrap_or(0);
    Ok(Reply {
        text,
        truncated: choice.get("finish_reason").and_then(Value::as_str) == Some("length"),
        usage: Usage {
            input_tokens: count(usage, "prompt_tokens").saturating_sub(cached),
            output_tokens: count(usage, "completion_tokens"),
            cache_read_tokens: cached,
            cache_write_tokens: 0,
        },
    })
}

fn count(object: Option<&Value>, field: &str) -> u32 {
    object.and_then(|o| o.get(field)).and_then(Value::as_u64).map(|n| n.min(u32::MAX as u64) as u32).unwrap_or(0)
}

/// The provider's own words for what was wrong, shortened — or the status when it gave none (a
/// wrong address often answers with an HTML 404 page, which is not worth repeating).
fn error_text(body: &str, status: u16) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed.as_ref().and_then(|v| {
        let error = v.get("error")?;
        error.get("message").and_then(Value::as_str).or_else(|| error.as_str()).map(str::to_string)
    });
    match message {
        Some(message) if message.chars().count() > MESSAGE_LIMIT => {
            format!("{}…", message.chars().take(MESSAGE_LIMIT).collect::<String>().trim_end())
        }
        Some(message) => message,
        None => format!("it answered with status {status}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt() -> Prompt {
        Prompt {
            system: vec![SystemPart::plain("You tighten prose."), SystemPart::cached("THE WHOLE MANUSCRIPT")],
            messages: vec![Message::user("Tighten: it is what it is.")],
            max_tokens: 500,
        }
    }

    fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
        request.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn anthropic() -> Provider {
        Provider::anthropic("claude-x")
    }

    fn local() -> Provider {
        Provider::openai_compatible("http://localhost:11434/v1/", "llama")
    }

    #[test]
    fn an_anthropic_request_carries_the_key_in_a_header_and_the_breakpoint_on_its_block() {
        let request = build_request(&anthropic(), Some("sk-secret"), &prompt()).unwrap();
        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(header(&request, "x-api-key"), Some("sk-secret"));
        assert_eq!(header(&request, "anthropic-version"), Some("2023-06-01"));

        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["model"], "claude-x");
        assert_eq!(body["max_tokens"], 500);
        assert_eq!(body["messages"][0], json!({ "role": "user", "content": "Tighten: it is what it is." }));
        assert_eq!(body["system"][0], json!({ "type": "text", "text": "You tighten prose." }));
        assert_eq!(
            body["system"][1],
            json!({ "type": "text", "text": "THE WHOLE MANUSCRIPT", "cache_control": { "type": "ephemeral" } })
        );
        assert!(!request.body.contains("sk-secret"), "the key travels in a header, never in the payload");
    }

    #[test]
    fn an_openai_style_request_joins_the_system_parts_and_needs_no_key_for_a_local_model() {
        let request = build_request(&local(), None, &prompt()).unwrap();
        assert_eq!(request.url, "http://localhost:11434/v1/chat/completions");
        assert_eq!(header(&request, "authorization"), None);

        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["messages"][0], json!({ "role": "system", "content": "You tighten prose.\n\nTHE WHOLE MANUSCRIPT" }));
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(!request.body.contains("cache_control"));

        let with_key = build_request(&local(), Some("  tok  "), &prompt()).unwrap();
        assert_eq!(header(&with_key, "authorization"), Some("Bearer tok"));
    }

    #[test]
    fn no_system_prompt_means_no_system_field() {
        let mut bare = prompt();
        bare.system.clear();
        let body: Value = serde_json::from_str(&build_request(&anthropic(), Some("k"), &bare).unwrap().body).unwrap();
        assert!(body.get("system").is_none());
    }

    #[test]
    fn anthropic_without_a_key_is_refused_before_anything_is_built() {
        for key in [None, Some(""), Some("   ")] {
            assert!(matches!(
                build_request(&anthropic(), key, &prompt()),
                Err(AssistantError::MissingKey { .. })
            ));
        }
    }

    #[test]
    fn a_key_is_never_sent_over_plain_http_to_another_machine() {
        let remote = Provider::openai_compatible("http://models.example.com/v1", "m");
        assert!(matches!(build_request(&remote, Some("k"), &prompt()), Err(AssistantError::InsecureAddress)));
        // Not a name that merely looks local either.
        let lookalike = Provider::openai_compatible("http://localhost.example.com/v1", "m");
        assert!(matches!(build_request(&lookalike, Some("k"), &prompt()), Err(AssistantError::InsecureAddress)));

        // This computer is fine, in each spelling; so is https anywhere.
        for address in ["http://localhost:8080/v1", "http://127.0.0.1:8080/v1", "http://[::1]:8080/v1", "https://models.example.com/v1"] {
            let provider = Provider::openai_compatible(address, "m");
            assert!(build_request(&provider, Some("k"), &prompt()).is_ok(), "{address}");
        }
        // With no key there is nothing secret to protect on the wire, so a LAN model is allowed.
        let lan = Provider::openai_compatible("http://192.168.1.20:11434/v1", "m");
        assert!(build_request(&lan, None, &prompt()).is_ok());
    }

    #[test]
    fn an_address_that_is_not_a_web_address_is_refused_by_name() {
        for address in ["", "not a url", "ftp://example.com/v1", "file:///etc/passwd"] {
            let provider = Provider::openai_compatible(address, "m");
            match build_request(&provider, None, &prompt()) {
                Err(AssistantError::BadAddress(shown)) => assert_eq!(shown, address),
                other => panic!("{address:?} gave {other:?}"),
            }
        }
    }

    #[test]
    fn a_prompt_with_no_message_is_not_sent() {
        let mut empty = prompt();
        empty.messages.clear();
        assert!(matches!(build_request(&anthropic(), Some("k"), &empty), Err(AssistantError::EmptyPrompt)));
    }

    #[test]
    fn debug_output_hides_credentials_but_not_the_payload() {
        let request = build_request(&anthropic(), Some("sk-secret"), &prompt()).unwrap();
        let shown = format!("{request:?}");
        assert!(!shown.contains("sk-secret"), "{shown}");
        assert!(shown.contains("<hidden>") && shown.contains("THE WHOLE MANUSCRIPT"));

        let bearer = format!("{:?}", build_request(&local(), Some("tok-secret"), &prompt()).unwrap());
        assert!(!bearer.contains("tok-secret"), "{bearer}");
    }

    #[test]
    fn the_key_slot_separates_services_and_hosts() {
        assert_eq!(anthropic().key_slot(), "anthropic");
        assert_eq!(local().key_slot(), "openai-compatible@localhost:11434");
        assert_eq!(Provider::openai_compatible("https://api.openai.com/v1", "m").key_slot(), "openai-compatible@api.openai.com");
    }

    #[test]
    fn a_provider_survives_the_settings_file() {
        let json = serde_json::to_string(&local()).unwrap();
        assert!(json.contains("\"kind\":\"openAiCompatible\""), "{json}");
        assert_eq!(serde_json::from_str::<Provider>(&json).unwrap(), local());
    }

    #[test]
    fn an_anthropic_answer_is_read_with_its_cache_figures() {
        let body = r#"{"content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"It is."},{"type":"text","text":" Done."}],
            "stop_reason":"end_turn",
            "usage":{"input_tokens":12,"output_tokens":5,"cache_read_input_tokens":4000,"cache_creation_input_tokens":30}}"#;
        let reply = parse_response(&anthropic(), 200, body).unwrap();
        assert_eq!(reply.text, "It is. Done.");
        assert!(!reply.truncated);
        assert_eq!(reply.usage, Usage { input_tokens: 12, output_tokens: 5, cache_read_tokens: 4000, cache_write_tokens: 30 });
    }

    #[test]
    fn an_answer_that_ran_out_of_room_says_so_in_either_format() {
        let anthropic_body = r#"{"content":[{"type":"text","text":"Half a sen"}],"stop_reason":"max_tokens","usage":{}}"#;
        assert!(parse_response(&anthropic(), 200, anthropic_body).unwrap().truncated);
        let openai_body = r#"{"choices":[{"message":{"content":"Half a sen"},"finish_reason":"length"}]}"#;
        assert!(parse_response(&local(), 200, openai_body).unwrap().truncated);
    }

    #[test]
    fn an_openai_answer_reports_cached_tokens_apart_from_fresh_ones() {
        let body = r#"{"choices":[{"message":{"content":"Fine."},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1000,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":900}}}"#;
        let reply = parse_response(&local(), 200, body).unwrap();
        assert_eq!(reply.text, "Fine.");
        assert_eq!(reply.usage, Usage { input_tokens: 100, output_tokens: 7, cache_read_tokens: 900, cache_write_tokens: 0 });
    }

    #[test]
    fn refusals_are_sentences_that_say_what_to_do() {
        let none = "";
        assert!(matches!(parse_response(&anthropic(), 401, none), Err(AssistantError::KeyRefused)));
        assert!(matches!(parse_response(&anthropic(), 403, none), Err(AssistantError::KeyRefused)));
        assert!(matches!(parse_response(&anthropic(), 429, none), Err(AssistantError::RateLimited)));
        assert!(matches!(parse_response(&anthropic(), 529, none), Err(AssistantError::ProviderTrouble { status: 529 })));
        assert!(matches!(parse_response(&anthropic(), 200, "<html>"), Err(AssistantError::UnreadableAnswer)));
        assert!(matches!(parse_response(&anthropic(), 200, r#"{"content":[]}"#), Err(AssistantError::EmptyAnswer)));
        assert!(matches!(parse_response(&local(), 200, r#"{"choices":[{"message":{"content":null}}]}"#), Err(AssistantError::EmptyAnswer)));
    }

    #[test]
    fn a_rejected_request_repeats_the_providers_words_but_not_a_whole_error_page() {
        let anthropic_error = r#"{"type":"error","error":{"type":"invalid_request_error","message":"model: unknown"}}"#;
        match parse_response(&anthropic(), 400, anthropic_error) {
            Err(AssistantError::Rejected { message }) => assert_eq!(message, "model: unknown"),
            other => panic!("{other:?}"),
        }
        let plain = r#"{"error":"model not found"}"#;
        assert!(matches!(parse_response(&local(), 404, plain), Err(AssistantError::Rejected { message }) if message == "model not found"));

        match parse_response(&local(), 404, "<html><body>Not Found</body></html>") {
            Err(AssistantError::Rejected { message }) => assert!(message.contains("404") && !message.contains("<html>"), "{message}"),
            other => panic!("{other:?}"),
        }
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(2000));
        match parse_response(&local(), 400, &long) {
            Err(AssistantError::Rejected { message }) => assert!(message.chars().count() <= MESSAGE_LIMIT + 1),
            other => panic!("{other:?}"),
        }
    }
}
