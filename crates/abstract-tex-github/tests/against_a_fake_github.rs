//! S10.4a: the device flow driven end to end against a GitHub that is not GitHub.
//!
//! Not `#[ignore]`d, because it needs no network beyond the loopback interface and nothing
//! installed: the server is forty lines of `TcpListener` below. What it buys over a mocked
//! `reqwest` is everything that actually breaks in an HTTP client — the form encoding, the
//! `Accept` header that decides which of two body formats GitHub answers in, the bearer token,
//! and the JSON shapes — all exercised for real.
//!
//! The one thing it cannot check is that GitHub behaves as documented. `tests/against_real_github.rs`
//! does that part, with the network, and is `#[ignore]`d.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use abstract_tex_github::{DeviceFlow, Endpoints, GitHubError, Poll};

/// One canned answer per request, in the order the flow will ask for them.
///
/// A queue rather than a router, because what these tests are about is the *sequence*: pending,
/// pending, slow_down, then a token. A router keyed by path could not express that.
fn fake_github(answers: Vec<&'static str>) -> (String, mpsc::Receiver<String>) {
    fake_github_answering(answers.into_iter().map(|body| (200, body)).collect())
}

/// The same, for the answers that are not `200` — a revoked token, a name GitHub will not take.
///
/// One server implementation and not two, because the first hand-rolled version of these tests
/// wrote its reply *without reading the request*, which closes the connection while the client is
/// still sending and comes back as "error sending request" rather than as the status under test.
/// That is a trap worth having exactly one copy of.
fn fake_github_answering(answers: Vec<(u16, &'static str)>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for (answer, stream) in answers.into_iter().zip(listener.incoming()) {
            let mut stream = stream.expect("a connection");
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            // Read the request line, the headers, and then exactly as many body bytes as
            // `Content-Length` promises — a `read_to_end` would block, because the client keeps
            // the connection open for a reply.
            let mut request = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(value) = line.strip_prefix("Content-Length: ").or_else(|| line.strip_prefix("content-length: ")) {
                    length = value.trim().parse().unwrap_or(0);
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            if length > 0 {
                let mut body = vec![0u8; length];
                std::io::Read::read_exact(&mut reader, &mut body).unwrap();
                request.push_str(&String::from_utf8_lossy(&body));
            }
            let _ = tx.send(request);

            let (status, body) = answer;
            let response = format!(
                "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reason(status),
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (origin, rx)
}

/// Enough of a reason phrase to be a valid status line; nothing here reads it.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        422 => "Unprocessable Entity",
        _ => "Status",
    }
}

fn flow(origin: &str) -> DeviceFlow {
    DeviceFlow::with_endpoints(Endpoints::under(origin)).unwrap()
}

#[test]
fn a_code_request_sends_what_github_documents_and_reads_back_the_code() {
    let (origin, requests) = fake_github(vec![
        r#"{"device_code":"dc-1","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":899,"interval":5}"#,
    ]);

    let code = flow(&origin).request_code("Iv1.test").unwrap();

    assert_eq!(code.user_code, "WDJB-MJHT");
    assert_eq!(code.verification_uri, "https://github.com/login/device");
    assert_eq!(code.device_code, "dc-1");
    assert_eq!(code.interval, Duration::from_secs(5));
    assert_eq!(code.expires_in, Duration::from_secs(899));

    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.starts_with("POST /login/device/code"), "{request}");
    // The `Accept` header is load-bearing: without it GitHub answers in form encoding, which
    // would need a second parser.
    assert!(request.contains("accept: application/json") || request.contains("Accept: application/json"), "{request}");
    assert!(request.contains("client_id=Iv1.test"), "{request}");
    // The one scope, as it crosses the wire.
    assert!(request.contains("scope=repo"), "{request}");
}

#[test]
fn the_five_answers_a_poll_can_give_are_five_different_things() {
    let (origin, requests) = fake_github(vec![
        r#"{"error":"authorization_pending","error_description":"The authorization request is still pending."}"#,
        r#"{"error":"slow_down","error_description":"Too many requests","interval":10}"#,
        r#"{"access_token":"gho_signed_in","token_type":"bearer","scope":"repo"}"#,
        r#"{"error":"access_denied","error_description":"The user denied the request."}"#,
        r#"{"error":"expired_token","error_description":"The device code has expired."}"#,
    ]);
    let flow = flow(&origin);

    assert_eq!(flow.poll("Iv1.test", "dc-1").unwrap(), Poll::Pending);
    // Mandatory, not advisory: the new interval comes back so the caller can obey it.
    assert_eq!(flow.poll("Iv1.test", "dc-1").unwrap(), Poll::SlowDown(Duration::from_secs(10)));
    assert_eq!(flow.poll("Iv1.test", "dc-1").unwrap(), Poll::Token("gho_signed_in".to_string()));
    assert!(matches!(flow.poll("Iv1.test", "dc-1"), Err(GitHubError::Denied)));
    assert!(matches!(flow.poll("Iv1.test", "dc-1"), Err(GitHubError::Expired)));

    // And the grant type, without which GitHub rejects the request outright.
    let first = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(first.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"), "{first}");
    assert!(first.contains("device_code=dc-1"), "{first}");
}

#[test]
fn a_slow_down_with_no_interval_still_slows_down() {
    // GitHub documents the field, and a server that leaves it out must not be read as "carry on
    // at the same rate" — that is how a flow gets itself rate-limited into a false expiry.
    let (origin, _requests) = fake_github(vec![r#"{"error":"slow_down"}"#]);
    assert_eq!(flow(&origin).poll("Iv1.test", "dc-1").unwrap(), Poll::SlowDown(Duration::from_secs(10)));
}

#[test]
fn an_unregistered_client_id_comes_back_as_githubs_own_sentence() {
    let (origin, _requests) = fake_github(vec![
        r#"{"error":"incorrect_client_credentials","error_description":"The client_id passed is incorrect."}"#,
    ]);
    let error = flow(&origin).request_code("Iv1.nope").unwrap_err();
    // The description, not the slug: "incorrect_client_credentials" is not a sentence for anyone.
    assert_eq!(error.to_string(), "GitHub said: The client_id passed is incorrect.");
}

#[test]
fn an_answer_this_crate_cannot_read_says_so_and_keeps_the_body() {
    let (origin, _requests) = fake_github(vec!["<html>maintenance</html>"]);
    let error = flow(&origin).request_code("Iv1.test").unwrap_err();
    assert!(matches!(error, GitHubError::Unreadable(_)), "{error}");
    // The body is kept, because this variant means *we* are wrong and someone has to see why.
    assert!(error.to_string().contains("maintenance"), "{error}");
}

#[test]
fn a_code_response_missing_the_fields_is_not_a_code() {
    // No `error`, no `user_code` either: a well-formed JSON object that is still not an answer.
    let (origin, _requests) = fake_github(vec![r#"{"expires_in":900}"#]);
    assert!(matches!(flow(&origin).request_code("Iv1.test"), Err(GitHubError::Unreadable(_))));
}

#[test]
fn the_account_call_carries_the_token_as_a_bearer_and_reads_the_login() {
    let (origin, requests) = fake_github(vec![r#"{"login":"ada","id":42,"name":"Ada Lovelace"}"#]);

    let account = flow(&origin).account("gho_signed_in").unwrap();
    assert_eq!(account.login, "ada");

    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.starts_with("GET /user"), "{request}");
    assert!(request.contains("authorization: Bearer gho_signed_in") || request.contains("Authorization: Bearer gho_signed_in"), "{request}");
    assert!(request.contains("x-github-api-version: 2022-11-28") || request.contains("X-GitHub-Api-Version: 2022-11-28"), "{request}");
}

#[test]
fn an_account_with_no_login_is_not_an_account() {
    let (origin, _requests) = fake_github(vec![r#"{"login":""}"#]);
    assert!(matches!(flow(&origin).account("gho_x"), Err(GitHubError::Unreadable(_))));
}

#[test]
fn a_github_that_is_not_there_is_a_network_error_and_not_a_panic() {
    // Nothing is listening on this port; the flow must come back with a sentence.
    let error = flow("http://127.0.0.1:1").request_code("Iv1.test").unwrap_err();
    assert!(matches!(error, GitHubError::Network(_)), "{error}");
    assert!(error.to_string().starts_with("Could not reach GitHub"), "{error}");
}

/// S10.4b needs this to be its own answer: a token revoked on github.com is not a token that
/// came back unreadable, and the app's reaction differs — forget it, and offer sign-in again.
#[test]
fn a_revoked_token_is_rejected_and_not_merely_unreadable() {
    let (origin, _requests) = fake_github_answering(vec![(401, r#"{"message":"Bad credentials"}"#)]);
    let error = flow(&origin).account("gho_revoked").unwrap_err();
    assert!(matches!(error, GitHubError::TokenRejected), "{error}");
}

// ---------------------------------------------------------------------------------------------
// S10.5b: creating a repository.
// ---------------------------------------------------------------------------------------------

fn repos(origin: &str) -> abstract_tex_github::Repos {
    abstract_tex_github::Repos::with_endpoints(Endpoints::under(origin)).unwrap()
}

fn wanted(name: &str, visibility: abstract_tex_github::Visibility) -> abstract_tex_github::NewRepository {
    abstract_tex_github::NewRepository { name: name.to_string(), visibility, description: None }
}

#[test]
fn a_new_repository_is_private_and_empty_on_the_wire() {
    let (origin, requests) = fake_github(vec![
        r#"{"full_name":"ada/thesis","clone_url":"https://github.com/ada/thesis.git","html_url":"https://github.com/ada/thesis","private":true}"#,
    ]);

    let created = repos(&origin)
        .create("gho_token", &wanted("thesis", abstract_tex_github::Visibility::Private))
        .unwrap();

    assert_eq!(created.full_name, "ada/thesis");
    assert_eq!(created.clone_url, "https://github.com/ada/thesis.git");
    assert!(created.private, "read back from GitHub's answer, not assumed from the request");

    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.starts_with("POST /user/repos"), "{request}");
    assert!(request.contains(r#""private":true"#), "{request}");
    // `auto_init: false` is load-bearing: a README GitHub made would be a commit the local
    // history does not have, and the first push would be rejected for reasons nobody can see.
    assert!(request.contains(r#""auto_init":false"#), "{request}");
    // No description field at all, rather than one mentioning this app in someone's research.
    assert!(!request.contains("description"), "{request}");
}

#[test]
fn a_public_repository_is_only_sent_once_it_has_been_confirmed() {
    // Refused before any request is made: nothing is listening on this port, and the test passes
    // precisely because nothing needed to be.
    let unconfirmed = repos("http://127.0.0.1:1")
        .create("gho_token", &wanted("thesis", abstract_tex_github::Visibility::Public { confirmed: false }))
        .unwrap_err();
    assert!(matches!(unconfirmed, GitHubError::PublicNotConfirmed), "{unconfirmed}");

    let (origin, requests) = fake_github(vec![
        r#"{"full_name":"ada/open","clone_url":"https://github.com/ada/open.git","html_url":"https://github.com/ada/open","private":false}"#,
    ]);
    let created = repos(&origin)
        .create("gho_token", &wanted("open", abstract_tex_github::Visibility::Public { confirmed: true }))
        .unwrap();
    assert!(!created.private);
    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.contains(r#""private":false"#), "{request}");
}

#[test]
fn a_name_github_will_not_take_comes_back_as_both_halves_of_its_sentence() {
    let (origin, _requests) = fake_github_answering(vec![(
        422,
        r#"{"message":"Repository creation failed.","errors":[{"field":"name","message":"name already exists on this account"}]}"#,
    )]);

    let error = repos(&origin)
        .create("gho_token", &wanted("thesis", abstract_tex_github::Visibility::Private))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "GitHub said: Repository creation failed. name already exists on this account"
    );
}

// ---- S11.5b: listing what an account can clone ----------------------------------------------

fn repository_json(name: &str) -> String {
    format!(
        r#"{{"full_name":"ada/{name}","clone_url":"https://github.com/ada/{name}.git","html_url":"https://github.com/ada/{name}","private":true,"id":1,"owner":{{"login":"ada"}}}}"#
    )
}

/// A page of `count` repositories, leaked so the fake server (which wants `&'static str`) can
/// hold it. Test-only, and a few kilobytes.
fn page_of(count: usize, start: usize) -> &'static str {
    let items: Vec<String> = (start..start + count).map(|n| repository_json(&format!("paper-{n}"))).collect();
    Box::leak(format!("[{}]", items.join(",")).into_boxed_str())
}

#[test]
fn listing_asks_for_the_accounts_repositories_with_the_token_and_reads_each_one() {
    let (origin, requests) = fake_github(vec![page_of(2, 0)]);

    let found = repos(&origin).list("gho_signed_in").unwrap();

    assert_eq!(found.iter().map(|r| r.full_name.as_str()).collect::<Vec<_>>(), ["ada/paper-0", "ada/paper-1"]);
    assert_eq!(found[0].clone_url, "https://github.com/ada/paper-0.git");
    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.starts_with("GET /user/repos?"), "{request}");
    assert!(request.contains("per_page=100") && request.contains("sort=pushed"), "{request}");
    assert!(request.contains("collaborator"), "a coauthor's paper is the commonest case: {request}");
    assert!(request.to_lowercase().contains("authorization: bearer gho_signed_in"), "{request}");
}

#[test]
fn a_full_page_asks_for_the_next_and_a_short_one_stops() {
    let (origin, requests) = fake_github(vec![page_of(100, 0), page_of(3, 100)]);

    let found = repos(&origin).list("t").unwrap();

    assert_eq!(found.len(), 103);
    let first = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    let second = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(first.contains("page=1"), "{first}");
    assert!(second.contains("page=2"), "{second}");
    assert!(requests.recv_timeout(Duration::from_millis(300)).is_err(), "a short page must end the walk");
}

#[test]
fn an_account_with_no_repositories_is_an_empty_list_and_not_an_error() {
    let (origin, _requests) = fake_github(vec!["[]"]);
    assert!(repos(&origin).list("t").unwrap().is_empty());
}

#[test]
fn listing_with_a_revoked_token_is_rejected_so_the_app_can_forget_it() {
    let (origin, _requests) = fake_github_answering(vec![(401, r#"{"message":"Bad credentials"}"#)]);
    assert!(matches!(repos(&origin).list("t"), Err(GitHubError::TokenRejected)));
}

#[test]
fn a_listing_that_is_not_a_list_says_so() {
    let (origin, _requests) = fake_github(vec![r#"{"unexpected":"object"}"#]);
    assert!(matches!(repos(&origin).list("t"), Err(GitHubError::Unreadable(_))));
}
