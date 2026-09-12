//! The language server session: one TexLab per open project, and the events it produces.
//!
//! `preamble-lsp` owns the process and the JSON-RPC correlation and knows nothing about Tauri
//! (S3.2). This module is the other side of that seam: it starts a bridge when a project opens,
//! keeps it for as long as that project is open, and turns everything the server says on its own
//! initiative into a window event the frontend can listen to.
//!
//! It must never interpret a method. A `textDocument/publishDiagnostics` notification is
//! forwarded with its params untouched — deciding what a diagnostic *means* on screen is the
//! frontend's job (DESIGN.md §4.1, "TypeScript owns protocol-to-CodeMirror adaptation").

use std::path::Path;
use std::sync::Mutex;

use preamble_lsp::{Bridge, Incoming, LspError, TexLab};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::{info, warn};

/// What the frontend hears from the language server.
///
/// Same shape as `CompileEvent`: a `#[serde(tag)]` enum, so the TypeScript side switches on one
/// field. The three cases are the three things that can happen without us asking.
#[derive(Debug, Clone, Serialize)]
// `rename_all_fields` for the reason given on `CompileEvent`: every field here is a single
// word today, so it changes nothing yet, but a two-word field added later would silently
// reach TypeScript as snake_case.
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LspEvent {
    /// The server sent a notification. `method` is the LSP method name, `params` its payload,
    /// both untouched.
    Notification { method: String, params: Value },
    /// The server asked *us* something. The frontend answers through the `lsp_respond` command,
    /// quoting `id`.
    Request { id: Value, method: String, params: Value },
    /// The process died and was restarted. The editor should re-open its documents, because a
    /// fresh server knows nothing about them.
    Restarted { restarts: u32 },
    /// The server is gone for good: it crashed more times than the bridge will restart. Language
    /// features stop; everything else in the app keeps working (DESIGN.md §2, commitment 6 —
    /// every feature has a path that does not depend on this one).
    Stopped { message: String },
}

/// The running language server, if one has been started.
///
/// `Mutex<Option<_>>` matches how `AppState` holds the project and the watcher: `None` until a
/// folder is open, replaced wholesale when a different one is. Dropping the old `Bridge` drops
/// the last sender, which tells the supervisor task to kill the process — so replacing the value
/// is all that "stop the previous server" requires.
#[derive(Default)]
pub struct LspSession {
    bridge: Mutex<Option<Bridge>>,
}

impl LspSession {
    /// The bridge for the open project, or an error naming what to do about it.
    pub fn bridge(&self) -> Result<Bridge, String> {
        self.bridge
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "The language server is not running for this project.".to_string())
    }

    /// Stop whatever is running. Called when a project closes, and before starting a new one.
    pub fn stop(&self) {
        if self.bridge.lock().unwrap().take().is_some() {
            info!("language server stopped");
        }
    }

    /// Start TexLab for `root` and forward its events through `on_event`.
    ///
    /// Returns the server's capabilities, so the frontend knows what it may ask for rather than
    /// guessing. A missing TexLab is reported as an error and nothing else: the editor must keep
    /// working without it (DESIGN.md §2, commitment 6).
    pub async fn start<F>(&self, root: &Path, on_event: F) -> Result<Value, String>
    where
        F: Fn(LspEvent) + Send + Sync + 'static,
    {
        self.stop();

        let texlab = TexLab::locate().map_err(|error| match error {
            LspError::NotFound => {
                "No TexLab binary found. Run `pnpm fetch-lsp`, or set PREAMBLE_TEXLAB.".to_string()
            }
            other => other.to_string(),
        })?;

        let (bridge, mut incoming) = Bridge::start(texlab, root).await.map_err(|e| e.to_string())?;

        // Forward on its own task for as long as the bridge lives. It ends when the supervisor
        // drops its sender, which happens when the last `Bridge` clone is dropped — so stopping
        // the session stops this task too, with no flag to remember to set.
        tauri::async_runtime::spawn(async move {
            while let Some(message) = incoming.recv().await {
                on_event(match message {
                    Incoming::Notification { method, params } => LspEvent::Notification { method, params },
                    Incoming::Request { id, method, params } => LspEvent::Request { id, method, params },
                    // The bridge restarts up to MAX_RESTARTS times and then stops sending. We
                    // cannot see which of the two this is from here, so both are reported as a
                    // restart and the frontend re-syncs; a server that is truly gone will fail
                    // the next request with a message that says so.
                    Incoming::Crashed { restarts } => {
                        warn!(restarts, "language server crashed");
                        LspEvent::Restarted { restarts }
                    }
                });
            }
        });

        // The handshake. `initialize` then `initialized`, in that order, because TexLab enforces
        // it — see the S3.1 outcome in SPRINTS.md for what skipping the notification looks like.
        //
        // Store the bridge *before* awaiting, not after. On the `?` path an early return would
        // otherwise drop the only `Bridge`, and dropping the last one tells the supervisor to
        // kill the process — so a handshake that merely failed would also take the server down,
        // and the log would read "restarted, then bridge dropped" instead of naming the real
        // cause. Found by running the app against a project path containing spaces.
        *self.bridge.lock().unwrap() = Some(bridge.clone());

        let capabilities = match bridge.initialize(root, client_capabilities()).await {
            Ok(capabilities) => capabilities,
            Err(error) => {
                self.stop();
                return Err(format!("The language server did not start: {error}"));
            }
        };
        info!(root = %root.display(), "language server ready");
        Ok(capabilities)
    }
}

/// What we tell TexLab we can do. Deliberately modest: every entry here is something S3.3 will
/// actually render. Claiming a capability we ignore makes the server do work for nothing.
fn client_capabilities() -> Value {
    json!({
        "textDocument": {
            "synchronization": { "didSave": true, "dynamicRegistration": false },
            "completion": {
                "completionItem": { "documentationFormat": ["plaintext", "markdown"] },
                "contextSupport": true,
            },
            "hover": { "contentFormat": ["plaintext", "markdown"] },
            "definition": {},
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
            "publishDiagnostics": {},
        },
        "workspace": { "workspaceFolders": false },
    })
}
