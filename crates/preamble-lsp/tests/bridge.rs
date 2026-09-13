//! The bridge against a real process: correlation when answers come back out of order, errors,
//! notifications, and restart after a crash. `src/bridge.rs`'s own tests cover routing in
//! isolation; these cover it with a pipe and a scheduler in the way.

use std::path::Path;
use std::time::Duration;

use preamble_lsp::{Bridge, CallError, Incoming, TexLab};
use serde_json::{json, Value};

fn rpc_server() -> TexLab {
    TexLab::at(env!("CARGO_BIN_EXE_fake-lsp-rpc"))
}

async fn start() -> (Bridge, tokio::sync::mpsc::UnboundedReceiver<Incoming>) {
    Bridge::start(rpc_server(), Path::new(".")).await.expect("the fake server starts")
}

#[tokio::test]
async fn a_request_gets_its_own_answer() {
    let (bridge, _incoming) = start().await;
    let result = bridge.request("hello", json!({})).await.unwrap();
    assert_eq!(result["echo"], "hello");
}

/// The test that justifies the whole module: two requests in flight, the *second* answered
/// first. A bridge that matched answers to callers by arrival order would swap these.
#[tokio::test]
async fn answers_go_to_the_right_caller_when_they_arrive_out_of_order() {
    let (bridge, _incoming) = start().await;

    let slow = {
        let bridge = bridge.clone();
        tokio::spawn(async move { bridge.request("slow", json!({})).await })
    };
    // Give `slow` a moment to be sent, then send a request that answers immediately.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let quick = bridge.request("quick", json!({})).await.unwrap();

    assert_eq!(quick["echo"], "quick", "the fast answer must not be the slow one");
    let slow = slow.await.unwrap().unwrap();
    assert_eq!(slow["echo"], "slow", "and the slow caller still gets its own");
}

#[tokio::test]
async fn many_concurrent_requests_each_get_their_own_answer() {
    let (bridge, _incoming) = start().await;
    let calls = (0..40).map(|i| {
        let bridge = bridge.clone();
        let method = format!("m{i}");
        tokio::spawn(async move { (method.clone(), bridge.request(&method, json!({})).await) })
    });
    for call in calls {
        let (method, result) = call.await.unwrap();
        assert_eq!(result.unwrap()["echo"], Value::String(method));
    }
}

#[tokio::test]
async fn a_server_error_reaches_the_caller_as_an_error_not_a_result() {
    let (bridge, _incoming) = start().await;
    let error = bridge.request("fail", json!({})).await.unwrap_err();
    match error {
        CallError::Rpc(rpc) => {
            assert_eq!(rpc.code, -32000);
            assert_eq!(rpc.message, "as requested");
        }
        other => panic!("expected an RPC error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unsolicited_notification_reaches_the_incoming_stream() {
    let (bridge, mut incoming) = start().await;
    bridge.notify("notify", json!({})).unwrap();
    let message = tokio::time::timeout(Duration::from_secs(5), incoming.recv())
        .await
        .expect("a notification must arrive")
        .unwrap();
    match message {
        Incoming::Notification { method, .. } => assert_eq!(method, "telemetry/event"),
        other => panic!("expected a notification, got {other:?}"),
    }
}

/// A crash must do three things: not hang the caller, announce itself, and leave a working
/// server behind. The pid in the reply proves the third — it is a different process.
#[tokio::test]
async fn a_crashed_server_is_restarted_and_the_waiting_caller_is_released() {
    let (bridge, mut incoming) = start().await;

    let before = bridge.request("who", json!({})).await.unwrap()["pid"].as_u64().unwrap();

    // `boom` exits without replying. The caller must come back as `Dropped`, not wait forever.
    let result = tokio::time::timeout(Duration::from_secs(5), bridge.request("boom", json!({})))
        .await
        .expect("a crash must not hang the caller");
    assert!(matches!(result, Err(CallError::Dropped)), "{result:?}");

    let message = tokio::time::timeout(Duration::from_secs(5), incoming.recv())
        .await
        .expect("the crash must be announced")
        .unwrap();
    assert!(matches!(message, Incoming::Crashed { restarts: 1 }), "{message:?}");

    // The restarted server answers, and it is not the process that died.
    let after = tokio::time::timeout(Duration::from_secs(5), bridge.request("who", json!({})))
        .await
        .expect("the restarted server must answer")
        .unwrap()["pid"]
        .as_u64()
        .unwrap();
    assert_ne!(before, after, "a restart means a new process");
}

/// Talks to the real TexLab. Ignored by default: needs `pnpm fetch-lsp`.
#[tokio::test]
#[ignore]
async fn the_real_texlab_initializes_and_completes_through_the_bridge() {
    let texlab = match TexLab::locate() {
        Ok(t) => t,
        Err(_) => {
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            TexLab::at(preamble_sidecar::in_repo_binaries("texlab", &repo).expect("run `pnpm fetch-lsp`"))
        }
    };
    let root = tempfile::tempdir().unwrap();
    let (bridge, _incoming) = Bridge::start(texlab, root.path()).await.unwrap();

    let capabilities = tokio::time::timeout(
        Duration::from_secs(20),
        bridge.initialize(root.path(), json!({"textDocument": {"completion": {}}}), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(capabilities.get("completionProvider").is_some(), "{capabilities}");

    bridge.shutdown().await.unwrap();
}

/// The whole S3.2 path against the real server: initialize, open a document, ask for
/// completions inside a `\begin{`, get real ones back. This is the sequence the frontend's
/// `LspClient` sends, so a break here is a break in the feature.
#[tokio::test]
#[ignore]
async fn the_real_texlab_completes_an_environment_name() {
    let texlab = match TexLab::locate() {
        Ok(t) => t,
        Err(_) => {
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            TexLab::at(preamble_sidecar::in_repo_binaries("texlab", &repo).expect("run `pnpm fetch-lsp`"))
        }
    };
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("main.tex");
    // Each `\\` is one backslash in the TeX source; `\n` stays a real newline, which is why
    // this is not a raw string.
    let source = "\\documentclass{article}\n\\begin{document}\n\\begin{}\n\\end{document}\n";
    std::fs::write(&file, source).unwrap();

    let (bridge, _incoming) = Bridge::start(texlab, root.path()).await.unwrap();
    bridge.initialize(root.path(), json!({"textDocument": {"completion": {}}}), None).await.unwrap();

    let uri = preamble_lsp::bridge::path_to_uri(&file);
    bridge
        .notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "latex", "version": 1, "text": source}}),
        )
        .unwrap();

    // Line 2, just inside `\begin{` — zero-based, as LSP counts.
    let completions = tokio::time::timeout(
        Duration::from_secs(20),
        bridge.request(
            "textDocument/completion",
            json!({"textDocument": {"uri": uri}, "position": {"line": 2, "character": 7}}),
        ),
    )
    .await
    .expect("completion must not hang")
    .unwrap();

    // The reply is either a bare array or `{items: [...]}` depending on the server's mood.
    let items = completions
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| completions.as_array())
        .expect("a completion list");
    let labels: Vec<&str> = items.iter().filter_map(|i| i.get("label").and_then(Value::as_str)).collect();
    assert!(labels.contains(&"itemize"), "expected environment names, got {labels:?}");

    bridge.shutdown().await.unwrap();
}
