//! Process lifecycle, against the `fake-lsp-echo` stand-in that `cargo test` builds alongside
//! these tests. The real TexLab is exercised by the `#[ignore]` test at the bottom.

use std::path::Path;
use std::time::Duration;

use abstract_tex_lsp::{LspError, TexLab};

fn echo_server() -> TexLab {
    TexLab::at(env!("CARGO_BIN_EXE_fake-lsp-echo"))
}

#[tokio::test]
async fn a_message_sent_comes_back_as_one_frame() {
    let mut server = echo_server().spawn(Path::new(".")).await.unwrap();
    server
        .send(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .await
        .unwrap();
    let reply = server.recv().await.unwrap();
    assert_eq!(reply, br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    server.kill().await.unwrap();
}

#[tokio::test]
async fn frames_keep_their_boundaries_under_load() {
    let mut server = echo_server().spawn(Path::new(".")).await.unwrap();
    // Bodies of wildly different sizes, sent without waiting for replies, must come back one
    // frame each and in order. A framing bug shows up here as a merged or torn message.
    let bodies: Vec<Vec<u8>> = (0..50)
        .map(|i| vec![b'a' + (i % 26) as u8; 1 + (i * 397) % 5000])
        .collect();
    // Ten at a time, not all fifty: `send` and `recv` both borrow the server mutably, so nothing
    // reads while we write. All fifty (~125 KB) overflow a Linux pipe's 64 KB twice over — the
    // echo blocks writing replies nobody reads, stops reading, and `send` waits forever (ledger).
    // Ten bodies stay under one pipe's worth and still put several frames in flight at once.
    for window in bodies.chunks(10) {
        for body in window {
            server.send(body).await.unwrap();
        }
        for body in window {
            assert_eq!(&server.recv().await.unwrap(), body);
        }
    }
    server.kill().await.unwrap();
}

#[tokio::test]
async fn a_server_that_exits_is_reported_as_exited_not_hung() {
    let mut server = echo_server().spawn(Path::new(".")).await.unwrap();
    server.send(b"die").await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("recv must not hang");
    assert!(matches!(result, Err(LspError::Exited)), "{result:?}");
    // And the exit code is visible, so the bridge can say "crashed" rather than "stopped".
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.exit_code(), Some(Some(3)));
}

#[tokio::test]
async fn a_binary_that_does_not_exist_is_a_spawn_error() {
    let result = TexLab::at("/definitely/not/a/language/server")
        .spawn(Path::new("."))
        .await;
    assert!(matches!(result, Err(LspError::Spawn(_))), "{:?}", result.err());
}

#[tokio::test]
async fn dropping_the_handle_kills_the_process() {
    let server = echo_server().spawn(Path::new(".")).await.unwrap();
    let pid = server.pid().expect("alive after spawn");
    drop(server);
    // `kill_on_drop` sends the signal on drop; give the OS a moment to act on it.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!process_exists(pid), "pid {pid} still alive after drop");
}

/// Is a process with this id still running? Portable enough for a test.
fn process_exists(pid: u32) -> bool {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    } else {
        // A zombie still "exists" until reaped, but `kill_on_drop` reaps; `/proc/<pid>/stat`
        // shows state `Z` for one that has not been, and we count that as gone.
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat.contains(" Z "),
            Err(_) => Path::new("/proc").exists() && false || !Path::new("/proc").exists() && kill_probe(pid),
        }
    }
}

/// macOS has no /proc; `kill -0` answers the same question.
fn kill_probe(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Talks to the real TexLab. Ignored by default because it needs the sidecar fetched
/// (`pnpm fetch-lsp`). `cargo test -p abstract-tex-lsp -- --ignored` runs it.
#[tokio::test]
#[ignore]
async fn the_real_texlab_answers_initialize() {
    let texlab = match TexLab::locate() {
        Ok(t) => t,
        Err(LspError::NotFound) => {
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            TexLab::at(
                abstract_tex_sidecar::in_repo_binaries("texlab", &repo).expect("run `pnpm fetch-lsp` first"),
            )
        }
        Err(e) => panic!("{e}"),
    };
    let root = tempfile::tempdir().unwrap();
    let mut server = texlab.spawn(root.path()).await.unwrap();

    // `format!("file://{path}")` is wrong on Windows: a path there is `C:\Users\...`, so the
    // result keeps its backslashes and has only two slashes — TexLab reads `C:` as the host and
    // exits. `path_to_uri` is the one place that conversion lives.
    let uri = abstract_tex_lsp::bridge::path_to_uri(root.path());
    let initialize = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"processId":null,"rootUri":"{uri}","capabilities":{{}}}}}}"#
    );
    server.send(initialize.as_bytes()).await.unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(10), server.recv())
        .await
        .unwrap()
        .unwrap();
    let reply = String::from_utf8(reply).unwrap();
    assert!(reply.contains(r#""id":1"#), "{reply}");
    assert!(reply.contains("capabilities"), "{reply}");
    assert!(
        reply.contains("completionProvider"),
        "TexLab should offer completion: {reply}"
    );

    // The spec requires `initialized` before any other request, and TexLab 5.26 enforces it:
    // without this it refuses the `shutdown` below and exits. `Bridge::initialize` sends it for
    // real callers; at this layer the test sends it by hand.
    server
        .send(br#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)
        .await
        .unwrap();

    server
        .send(br#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#)
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), server.recv())
        .await
        .unwrap()
        .unwrap();
    server
        .send(br#"{"jsonrpc":"2.0","method":"exit"}"#)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(server.exit_code(), Some(Some(0)), "a polite exit is code 0");
}
