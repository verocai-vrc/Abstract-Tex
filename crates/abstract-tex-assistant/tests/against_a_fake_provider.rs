//! S12.1a: both wire formats driven end to end against a provider that is not one.
//!
//! Not `#[ignore]`d: the server is a `TcpListener` on the loopback interface, so nothing leaves
//! the machine and nothing needs installing. What it buys over comparing built values (the unit
//! tests in `provider.rs`) is the part that only breaks for real — that the headers really are
//! sent, that the status really is read, that a redirect really is refused. What it cannot check
//! is that Anthropic or OpenAI behave as this crate expects; that needs an account and a key.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use abstract_tex_assistant::{
    Assistant, AssistantError, KeyStore, MemoryKeyStore, Message, Prompt, Provider, SystemPart,
};

/// One canned `(status, extra header, body)` answer per request, in order; the receiver gets each
/// raw request (request line, headers and body) as the server saw it.
fn fake_provider(answers: Vec<(u16, &'static str, &'static str)>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for ((status, extra_header, body), stream) in answers.into_iter().zip(listener.incoming()) {
            let mut stream = stream.expect("a connection");
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            // Read the headers, then exactly `Content-Length` body bytes. Replying without reading
            // the request makes the client see a reset instead of the status under test.
            let mut request = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut payload = vec![0u8; length];
            reader.read_exact(&mut payload).unwrap();
            request.push_str(&String::from_utf8_lossy(&payload));
            let _ = tx.send(request);

            let response = format!(
                "HTTP/1.1 {status} X\r\n{extra_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (origin, rx)
}

fn prompt() -> Prompt {
    Prompt {
        system: vec![
            SystemPart::plain("You tighten prose."),
            SystemPart::cached("THE MANUSCRIPT"),
        ],
        messages: vec![Message::user("Tighten this.")],
        max_tokens: 300,
    }
}

const ANTHROPIC_OK: &str = r#"{"content":[{"type":"text","text":"Tighter."}],"stop_reason":"end_turn","usage":{"input_tokens":9,"output_tokens":2,"cache_read_input_tokens":500}}"#;
const OPENAI_OK: &str = r#"{"choices":[{"message":{"content":"Tighter."},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":2}}"#;

#[test]
fn an_anthropic_call_sends_the_key_and_the_breakpoint_and_reads_the_answer() {
    let (origin, requests) = fake_provider(vec![(200, "", ANTHROPIC_OK)]);
    let provider = Provider::Anthropic {
        model: "claude-x".into(),
        address: origin,
    };

    let reply = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, Some("sk-test"), &prompt())
        .unwrap();

    assert_eq!(reply.text, "Tighter.");
    assert_eq!(reply.usage.cache_read_tokens, 500);
    let seen = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(seen.starts_with("POST /v1/messages HTTP/1.1"), "{seen}");
    let lower = seen.to_ascii_lowercase();
    assert!(lower.contains("x-api-key: sk-test"), "{seen}");
    assert!(lower.contains("anthropic-version: 2023-06-01"), "{seen}");
    assert!(seen.contains(r#""cache_control":{"type":"ephemeral"}"#), "{seen}");
    assert!(seen.contains("THE MANUSCRIPT"));
}

#[test]
fn an_openai_style_call_to_a_local_model_sends_no_key_at_all() {
    let (origin, requests) = fake_provider(vec![(200, "", OPENAI_OK)]);
    let provider = Provider::openai_compatible(&origin, "llama");

    let reply = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, None, &prompt())
        .unwrap();

    assert_eq!(reply.text, "Tighter.");
    let seen = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(seen.starts_with("POST /chat/completions HTTP/1.1"), "{seen}");
    assert!(!seen.to_ascii_lowercase().contains("authorization"), "{seen}");
    assert!(seen.contains(r#""role":"system""#));
}

#[test]
fn the_saved_key_is_the_one_that_is_sent() {
    let (origin, requests) = fake_provider(vec![(200, "", OPENAI_OK)]);
    let provider = Provider::openai_compatible(&origin, "llama");
    let keys = MemoryKeyStore::default();
    keys.save(&provider.key_slot(), "from-the-keychain").unwrap();

    Assistant::new()
        .unwrap()
        .complete(&provider, &keys, &prompt())
        .unwrap();

    let seen = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        seen.to_ascii_lowercase()
            .contains("authorization: bearer from-the-keychain"),
        "{seen}"
    );
}

#[test]
fn a_refused_key_a_rate_limit_and_a_server_error_each_have_their_own_error() {
    let (origin, _) = fake_provider(vec![
        (401, "", "{}"),
        (429, "", "{}"),
        (503, "", "{}"),
        (400, "", r#"{"error":{"message":"bad model"}}"#),
    ]);
    let provider = Provider::openai_compatible(&origin, "m");
    let assistant = Assistant::new().unwrap();

    let ask = || {
        assistant
            .complete_with_key(&provider, Some("k"), &prompt())
            .unwrap_err()
    };
    assert!(matches!(ask(), AssistantError::KeyRefused));
    assert!(matches!(ask(), AssistantError::RateLimited));
    assert!(matches!(ask(), AssistantError::ProviderTrouble { status: 503 }));
    assert!(matches!(ask(), AssistantError::Rejected { message } if message == "bad model"));
}

#[test]
fn a_cut_off_answer_is_marked_as_one() {
    let (origin, _) = fake_provider(vec![(
        200,
        "",
        r#"{"choices":[{"message":{"content":"Half a sen"},"finish_reason":"length"}]}"#,
    )]);
    let provider = Provider::openai_compatible(&origin, "m");
    let reply = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, None, &prompt())
        .unwrap();
    assert!(reply.truncated);
}

/// A redirect would carry the key header to wherever it points. It is not followed, and the second
/// server — standing for "somewhere else" — never hears from us.
#[test]
fn a_redirect_is_not_followed_so_the_key_goes_nowhere_else() {
    let (elsewhere, elsewhere_requests) = fake_provider(vec![(200, "", OPENAI_OK)]);
    let location: &'static str =
        Box::leak(format!("Location: {elsewhere}/chat/completions\r\n").into_boxed_str());
    let (origin, _) = fake_provider(vec![(307, location, "")]);
    let provider = Provider::openai_compatible(&origin, "m");

    let error = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, Some("sk-secret"), &prompt())
        .unwrap_err();

    assert!(matches!(error, AssistantError::Rejected { .. }), "{error:?}");
    assert!(
        elsewhere_requests
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "the redirect target was contacted"
    );
}

/// "Nothing is sent without an explicit action": constructing the client and building a request
/// open no connection. Only `complete*` does.
#[test]
fn nothing_connects_until_complete_is_called() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let provider = Provider::openai_compatible(&format!("http://{}", listener.local_addr().unwrap()), "m");

    let _assistant = Assistant::new().unwrap();
    let _request = abstract_tex_assistant::build_request(&provider, Some("k"), &prompt()).unwrap();

    std::thread::sleep(Duration::from_millis(100));
    match listener.accept() {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
        Ok(_) => panic!("something connected without being asked to"),
    }
}

/// A refused address fails before a socket exists, so a key bound for plain http to a stranger's
/// machine cannot even reach name resolution. (`.invalid` never resolves; if the check ran late
/// this would be a `Network` error instead.)
#[test]
fn an_insecure_address_is_refused_before_any_connection_is_attempted() {
    let provider = Provider::openai_compatible("http://models.invalid/v1", "m");
    let error = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, Some("sk-secret"), &prompt())
        .unwrap_err();
    assert!(matches!(error, AssistantError::InsecureAddress), "{error:?}");
    assert!(!error.to_string().contains("sk-secret"));
}

#[test]
fn no_error_message_contains_the_key() {
    let (origin, _) = fake_provider(vec![(401, "", r#"{"error":{"message":"bad key"}}"#)]);
    let provider = Provider::openai_compatible(&origin, "m");
    let error = Assistant::new()
        .unwrap()
        .complete_with_key(&provider, Some("sk-secret"), &prompt())
        .unwrap_err();
    assert!(!format!("{error} {error:?}").contains("sk-secret"));

    // And a connection that fails outright.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = Provider::openai_compatible(&format!("http://{}", closed.local_addr().unwrap()), "m");
    drop(closed);
    let error = Assistant::new()
        .unwrap()
        .complete_with_key(&dead, Some("sk-secret"), &prompt())
        .unwrap_err();
    assert!(matches!(error, AssistantError::Network(_)), "{error:?}");
    assert!(!format!("{error} {error:?}").contains("sk-secret"));
}
