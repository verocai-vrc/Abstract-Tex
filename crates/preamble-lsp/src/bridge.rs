//! JSON-RPC over the framed stdio pipe: who asked what, which reply belongs to whom, and what
//! happens when the server dies mid-question (S3.2).
//!
//! `server.rs` moves whole frames and has no idea what is inside them. This module is the layer
//! that does: it assigns request ids, hands each caller back exactly its own response, routes
//! everything the server says on its own initiative to one place, and restarts the process when
//! it crashes. Above it sits Tauri (events to TypeScript); below it, the pipe. It depends on
//! neither the editor nor Tauri, so `cargo test -p preamble-lsp` still runs with no window.
//!
//! It must never interpret a method's *meaning*. `textDocument/completion` is a string here and
//! nothing more — turning it into CodeMirror behaviour is the frontend's job (DESIGN.md §4.1,
//! "split the LSP client across the boundary").

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, info, warn};

use crate::server::{Running, TexLab};
use crate::LspError;

/// How many times a crashed server is restarted before we stop trying. A server that dies
/// repeatedly is broken in a way restarting will not fix, and an infinite respawn loop would
/// burn a core and fill the log rather than surface the problem.
const MAX_RESTARTS: u32 = 5;

/// Something the server said that nobody asked for: a notification (`textDocument/
/// publishDiagnostics`), or a request *to us* (`window/showMessage`). The bridge does not act on
/// these — it forwards them, and the app decides.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A notification: a method and its params, no reply expected.
    Notification { method: String, params: Value },
    /// The server asked *us* something. `id` must be echoed back in the answer.
    Request { id: Value, method: String, params: Value },
    /// The process died. The bridge restarts it; this says it happened, so the UI can tell the
    /// author why completion stopped for a moment instead of silently going dead.
    Crashed { restarts: u32 },
}

/// An error carried back from the server in a JSON-RPC `error` object, kept whole so a caller
/// can report the server's own words rather than a paraphrase.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("language server error {code}: {message}")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

/// What a `request` can come back as.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    /// The server died with this request still outstanding, or the bridge was shut down.
    #[error("the language server went away before answering")]
    Dropped,
    #[error(transparent)]
    Lsp(#[from] LspError),
}

/// The table of questions we are still waiting on answers to.
///
/// `Arc<Mutex<_>>` because two tasks touch it: the caller's task inserts before sending, and the
/// single reader task removes when the matching response arrives. This is the first
/// `Arc<Mutex<_>>` in this crate: `Arc` is the shared-ownership pointer (both tasks keep the map
/// alive), and `Mutex` makes the two-step "look up, then remove" indivisible. It is tokio's
/// mutex rather than `std`'s because it is held across no `.await` here but lives inside async
/// tasks, and mixing the two is the easier mistake to avoid by picking one.
type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, RpcError>>>>>;

/// A live language server, spoken to in JSON-RPC rather than in frames.
///
/// Cloneable: every clone talks to the same process, so the editor can hold one and the
/// document-sync code another without either owning the other.
#[derive(Clone)]
pub struct Bridge {
    /// Outbound messages. A channel rather than the pipe itself, because several tasks send and
    /// only one may write to stdin at a time; the writer task is the one place that does.
    outbound: mpsc::UnboundedSender<Vec<u8>>,
    pending: Pending,
    /// Request ids must be unique for the life of the connection. `AtomicI64` so `request` needs
    /// no lock for the common case of "take the next number".
    next_id: Arc<AtomicI64>,
}

impl Bridge {
    /// Start TexLab under a supervisor and return the bridge to it, plus the stream of
    /// everything the server says on its own initiative.
    ///
    /// The supervisor task owns the process: it reads frames, matches responses to callers,
    /// forwards the rest to `incoming`, and respawns on a crash. It ends when the `Bridge` and
    /// all its clones are dropped.
    pub async fn start(
        texlab: TexLab,
        root: &Path,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Incoming>), LspError> {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

        // Spawn once here rather than inside the supervisor so that "the binary does not exist"
        // is an error from `start`, not a crash notification a moment later.
        let running = texlab.spawn(root).await?;

        tokio::spawn(supervise(
            texlab,
            root.to_path_buf(),
            running,
            outbound_rx,
            incoming_tx,
            Arc::clone(&pending),
        ));

        Ok((
            Self { outbound: outbound_tx, pending, next_id: Arc::new(AtomicI64::new(1)) },
            incoming_rx,
        ))
    }

    /// Ask the server something and wait for its answer.
    ///
    /// The id is allocated and the caller's half of a `oneshot` is parked in `pending` *before*
    /// the message goes out, because a fast server can reply before `send` has even returned.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, CallError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if self.outbound.send(serde_json::to_vec(&body).expect("serialising a Value cannot fail")).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(CallError::Dropped);
        }
        debug!(id, method, "request sent");

        // `Err` on the oneshot means the sender was dropped without a value — the supervisor
        // cleared the table because the server died. That is `Dropped`, not a hang.
        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(rpc)) => Err(CallError::Rpc(rpc)),
            Err(_) => Err(CallError::Dropped),
        }
    }

    /// Tell the server something. No id, no reply, no waiting — `textDocument/didChange` is the
    /// one that matters for latency, and it must never block a keystroke.
    pub fn notify(&self, method: &str, params: Value) -> Result<(), CallError> {
        let body = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.outbound
            .send(serde_json::to_vec(&body).expect("serialising a Value cannot fail"))
            .map_err(|_| CallError::Dropped)
    }

    /// Answer a request the server made of us. `id` is the one from `Incoming::Request`.
    pub fn respond(&self, id: Value, result: Value) -> Result<(), CallError> {
        let body = json!({"jsonrpc": "2.0", "id": id, "result": result});
        self.outbound
            .send(serde_json::to_vec(&body).expect("serialising a Value cannot fail"))
            .map_err(|_| CallError::Dropped)
    }

    /// The LSP handshake, in the order the spec requires: `initialize`, then `initialized`.
    /// Returns the server's capabilities, which the frontend needs to know what it may ask for.
    pub async fn initialize(&self, root: &Path, capabilities: Value) -> Result<Value, CallError> {
        let result = self
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": path_to_uri(root),
                    "capabilities": capabilities,
                }),
            )
            .await?;
        self.notify("initialized", json!({}))?;
        Ok(result.get("capabilities").cloned().unwrap_or(Value::Null))
    }

    /// The polite stop: `shutdown` and then `exit`, per the spec. Dropping the bridge kills the
    /// process outright; this gives the server the chance to finish first.
    pub async fn shutdown(&self) -> Result<(), CallError> {
        self.request("shutdown", Value::Null).await?;
        self.notify("exit", Value::Null)
    }
}

/// Owns the process for as long as anyone holds the bridge. Reads frames, routes them, and
/// respawns a server that dies.
///
/// Written as one `select!` loop rather than two tasks because the outbound and inbound halves
/// have to agree about *when the process is replaced*: a restart swaps both pipes at once, and a
/// writer task holding the old stdin would write into a dead process.
async fn supervise(
    texlab: TexLab,
    root: PathBuf,
    mut running: Running,
    mut outbound: mpsc::UnboundedReceiver<Vec<u8>>,
    incoming: mpsc::UnboundedSender<Incoming>,
    pending: Pending,
) {
    let mut restarts = 0u32;
    loop {
        // `select!` waits on several futures and runs whichever finishes first, cancelling the
        // rest. Both arms here are cancel-safe: `recv` on an mpsc and `recv` on the frame reader
        // either complete or leave their buffers untouched.
        tokio::select! {
            outgoing = outbound.recv() => {
                let Some(body) = outgoing else {
                    // Every Bridge clone has been dropped. Nobody can ask anything again, so
                    // take the process down with us.
                    debug!("bridge dropped; stopping language server");
                    let _ = running.kill().await;
                    return;
                };
                if let Err(error) = running.send(&body).await {
                    // A failed write means a dead pipe; let the read arm notice and restart.
                    warn!(%error, "write to language server failed");
                }
            }

            frame = running.recv() => {
                match frame {
                    Ok(body) => route(&body, &pending, &incoming).await,
                    Err(error) => {
                        warn!(%error, "language server stopped");
                        // Everyone still waiting gets `Dropped` rather than waiting forever:
                        // clearing the map drops every oneshot sender, which wakes its receiver.
                        pending.lock().await.clear();

                        restarts += 1;
                        if restarts > MAX_RESTARTS {
                            warn!(restarts = MAX_RESTARTS, "language server keeps dying; giving up");
                            let _ = incoming.send(Incoming::Crashed { restarts });
                            return;
                        }
                        if incoming.send(Incoming::Crashed { restarts }).is_err() {
                            return; // nobody is listening any more
                        }
                        match texlab.spawn(&root).await {
                            Ok(fresh) => {
                                info!(restarts, "language server restarted");
                                running = fresh;
                            }
                            Err(error) => {
                                warn!(%error, "could not restart the language server");
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Decide what one inbound message is, and deliver it.
///
/// JSON-RPC tells the three kinds apart by which fields are present: a response has `id` and one
/// of `result`/`error`; a request has `id` and `method`; a notification has `method` and no `id`.
async fn route(body: &[u8], pending: &Pending, incoming: &mpsc::UnboundedSender<Incoming>) {
    let message: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(error) => {
            warn!(%error, "language server sent something that is not JSON");
            return;
        }
    };

    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id");

    match (method, id) {
        // A request from the server to us.
        (Some(method), Some(id)) => {
            let _ = incoming.send(Incoming::Request {
                id: id.clone(),
                method: method.to_string(),
                params: message.get("params").cloned().unwrap_or(Value::Null),
            });
        }
        // A notification.
        (Some(method), None) => {
            let _ = incoming.send(Incoming::Notification {
                method: method.to_string(),
                params: message.get("params").cloned().unwrap_or(Value::Null),
            });
        }
        // A response to something we asked.
        (None, Some(id)) => {
            // Ids we sent are always integers, so anything else is a response to a request we
            // never made — ignore it rather than guess.
            let Some(id) = id.as_i64() else {
                warn!(?id, "response with an id we never issued");
                return;
            };
            let Some(waiting) = pending.lock().await.remove(&id) else {
                // Not an error worth shouting about: a response that arrives after its caller
                // gave up is normal when a request races a restart.
                debug!(id, "response for a request nobody is waiting on");
                return;
            };
            let answer = match message.get("error") {
                Some(error) => Err(RpcError {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("(no message)")
                        .to_string(),
                    data: error.get("data").cloned(),
                }),
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            // `Err` here means the caller stopped waiting; nothing to do about it.
            let _ = waiting.send(answer);
        }
        (None, None) => warn!("message with neither a method nor an id"),
    }
}

/// LSP addresses files by URI, not path. Enough of a conversion for local absolute paths, which
/// is all a local-first editor ever has; a full RFC 8089 implementation is not needed here.
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    // Windows absolute paths (`C:/x`) need the extra slash that makes an empty authority.
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_unix_paths_both_become_file_uris() {
        assert_eq!(path_to_uri(Path::new("/home/a/p.tex")), "file:///home/a/p.tex");
        assert_eq!(path_to_uri(Path::new(r"C:\Users\a\p.tex")), "file:///C:/Users/a/p.tex");
    }

    /// Routing is pure enough to test without a process: build the pieces by hand and feed
    /// `route` the bytes a server would send.
    fn harness() -> (Pending, mpsc::UnboundedSender<Incoming>, mpsc::UnboundedReceiver<Incoming>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Arc::new(Mutex::new(HashMap::new())), tx, rx)
    }

    #[tokio::test]
    async fn a_response_reaches_the_caller_that_asked() {
        let (pending, tx, _rx) = harness();
        let (reply_tx, reply_rx) = oneshot::channel();
        pending.lock().await.insert(7, reply_tx);

        route(br#"{"jsonrpc":"2.0","id":7,"result":{"items":[]}}"#, &pending, &tx).await;

        assert_eq!(reply_rx.await.unwrap().unwrap(), json!({"items": []}));
        assert!(pending.lock().await.is_empty(), "the entry is removed once answered");
    }

    #[tokio::test]
    async fn an_error_response_becomes_an_rpc_error() {
        let (pending, tx, _rx) = harness();
        let (reply_tx, reply_rx) = oneshot::channel();
        pending.lock().await.insert(1, reply_tx);

        route(
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"unknown method"}}"#,
            &pending,
            &tx,
        )
        .await;

        let error = reply_rx.await.unwrap().unwrap_err();
        assert_eq!(error.code, -32601);
        assert_eq!(error.message, "unknown method");
    }

    #[tokio::test]
    async fn a_notification_goes_to_the_incoming_stream() {
        let (pending, tx, mut rx) = harness();
        route(
            br#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///a.tex"}}"#,
            &pending,
            &tx,
        )
        .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            Incoming::Notification {
                method: "textDocument/publishDiagnostics".into(),
                params: json!({"uri": "file:///a.tex"}),
            }
        );
    }

    #[tokio::test]
    async fn a_request_from_the_server_keeps_its_id_so_we_can_answer() {
        let (pending, tx, mut rx) = harness();
        route(
            br#"{"jsonrpc":"2.0","id":"srv-1","method":"window/showMessageRequest","params":{}}"#,
            &pending,
            &tx,
        )
        .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            Incoming::Request {
                id: json!("srv-1"),
                method: "window/showMessageRequest".into(),
                params: json!({}),
            }
        );
    }

    #[tokio::test]
    async fn a_response_nobody_waits_for_is_ignored_quietly() {
        let (pending, tx, mut rx) = harness();
        // No panic, no entry, and nothing pushed at the app.
        route(br#"{"jsonrpc":"2.0","id":99,"result":null}"#, &pending, &tx).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn rubbish_from_the_server_does_not_kill_the_router() {
        let (pending, tx, mut rx) = harness();
        route(b"this is not json", &pending, &tx).await;
        route(br#"{"jsonrpc":"2.0"}"#, &pending, &tx).await;
        assert!(rx.try_recv().is_err());
    }
}
