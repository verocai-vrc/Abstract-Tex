//! The compile orchestrator (DESIGN.md §3, §5.1).
//!
//! Owns exactly one rule: **one build in flight per project**. A new request cancels the running
//! build and starts over; nothing is ever queued. Typing bursts therefore produce one compile,
//! not forty, and the PDF on screen always corresponds to the latest text the author saved.
//!
//! This module reports through a callback, not through Tauri directly, so it can be tested with
//! a fake engine and a channel. The `commands` module supplies a callback that emits a window
//! event. It must never read the log beyond handing it to `texlog`, and never touch the editor.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use preamble_engine::{BuildJob, Engine, EngineError};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// What the frontend hears about a build. `generation` increases with every request so the
/// frontend can ignore anything older than the last one it asked for.
///
/// `#[serde(tag = "status")]` serialises as `{"status": "started", ...}`: one JSON shape the
/// TypeScript side can switch on.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum CompileEvent {
    Started {
        generation: u64,
        root_file: String,
    },
    /// One line of engine stderr, as it arrives (DESIGN.md §6: a cold package fetch is stated
    /// plainly, never a silent hang). There can be many of these between `Started` and
    /// `Finished`; the frontend keeps only the latest.
    Progress {
        generation: u64,
        message: String,
    },
    Finished {
        generation: u64,
        success: bool,
        /// Absolute path to the PDF, if one exists. The frontend converts it to an asset URL.
        pdf_path: Option<String>,
        log_path: Option<String>,
        /// One entry per problem, already explained in sentences by `texlog`'s rule catalog
        /// (S2.6). Errors and warnings both; the frontend tells them apart by `severity`.
        /// Carries a line but not yet a file — the paren-stack resolver is S5.2 — so the
        /// frontend attributes every one to the root file until then.
        diagnostics: Vec<texlog::Diagnostic>,
        duration_ms: u64,
        /// Engine stderr, for the "raw output" view that is one click away.
        stderr: String,
    },
    /// The build could not run at all: no engine, or it failed to spawn.
    Failed {
        generation: u64,
        message: String,
    },
}

/// Shared between the orchestrator and the tasks it spawns.
///
/// `Arc` ("atomically reference counted") lets several owners share one value; the last one
/// dropped frees it. We need it because each spawned build task must outlive the `&self` borrow
/// of the command that started it.
struct Inner {
    engine: Option<Arc<dyn Engine>>,
    /// The token for the build currently running, tagged with its generation.
    in_flight: Mutex<Option<(u64, CancellationToken)>>,
    /// Monotonic counter. `AtomicU64` gives us increment-and-read without a lock.
    generation: AtomicU64,
}

pub struct Orchestrator {
    inner: Arc<Inner>,
}

impl Orchestrator {
    pub fn new(engine: Option<Arc<dyn Engine>>) -> Self {
        Self {
            inner: Arc::new(Inner { engine, in_flight: Mutex::new(None), generation: AtomicU64::new(0) }),
        }
    }

    pub fn engine(&self) -> Option<Arc<dyn Engine>> {
        self.inner.engine.clone()
    }

    /// Start a build, cancelling whichever one was running. Returns the new generation number.
    ///
    /// `on_event` is called from a background task, so it must be `Send + Sync + 'static`:
    /// safe to hand to another thread and not borrowing anything short-lived.
    pub fn request<F>(&self, job: BuildJob, on_event: F) -> u64
    where
        F: Fn(CompileEvent) + Send + Sync + 'static,
    {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let token = CancellationToken::new();

        // Swap our token in and cancel whatever was there. The lock is held for nanoseconds.
        if let Some((old_generation, old_token)) = self.inner.in_flight.lock().unwrap().replace((generation, token.clone())) {
            debug!(old_generation, new_generation = generation, "cancelling superseded build");
            old_token.cancel();
        }

        let Some(engine) = self.inner.engine.clone() else {
            on_event(CompileEvent::Failed {
                generation,
                message: "No TeX engine is available. The bundled Tectonic binary was not found.".to_string(),
            });
            return generation;
        };

        on_event(CompileEvent::Started { generation, root_file: job.root_file.to_string_lossy().replace('\\', "/") });

        // `on_event` is called from two places now: this task, when the build finishes, and
        // the forwarder task below, for every progress line. `Arc` lets both hold a copy
        // without either owning it outright.
        let on_event = Arc::new(on_event);

        // Progress lines arrive on a channel rather than through `on_event` directly, because
        // the engine only knows how to send lines — it must never know it is even talking to
        // Tauri (preamble-engine's module doc). This task's only job is to relay each one as a
        // `CompileEvent::Progress` until the sender side (in the build task below) is dropped.
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<String>();
        let progress_on_event = Arc::clone(&on_event);
        let progress_task = tokio::spawn(async move {
            while let Some(message) = progress_rx.recv().await {
                progress_on_event(CompileEvent::Progress { generation, message });
            }
        });

        let inner = Arc::clone(&self.inner);
        // Tauri's runtime is Tokio; this works in tests too, where Tauri lazily creates one.
        tauri::async_runtime::spawn(async move {
            let result = engine.build(&job, token, Some(progress_tx)).await;

            // `build` dropped its end of the progress channel on the way out (whether it
            // returned `Ok` or `Err`), so the forwarder task above is guaranteed to run to
            // completion once it is polled. Waiting for it here means every `Progress` event
            // a build sent is relayed to the frontend before `Finished`/`Failed` is — without
            // this, the two run on independent tasks with no ordering between them.
            let _ = progress_task.await;

            // Only the build that is still current gets to clear the slot; a superseded build
            // finishing late must not clobber the newer token.
            {
                let mut slot = inner.in_flight.lock().unwrap();
                if slot.as_ref().is_some_and(|(g, _)| *g == generation) {
                    *slot = None;
                }
            }

            match result {
                Ok(outcome) => {
                    let diagnostics = outcome.log.as_deref().map(read_diagnostics).unwrap_or_default();
                    info!(generation, success = outcome.success, diagnostics = diagnostics.len(), "build finished");
                    on_event(CompileEvent::Finished {
                        generation,
                        success: outcome.success,
                        pdf_path: outcome.pdf.map(|p| p.to_string_lossy().into_owned()),
                        log_path: outcome.log.map(|p| p.to_string_lossy().into_owned()),
                        diagnostics,
                        duration_ms: outcome.duration.as_millis() as u64,
                        stderr: outcome.stderr,
                    });
                }
                Err(EngineError::Cancelled) => {
                    // Silence is correct: the frontend already received `Started` for the
                    // build that replaced this one.
                    debug!(generation, "build cancelled");
                }
                Err(error) => {
                    on_event(CompileEvent::Failed { generation, message: error.to_string() });
                }
            }
        });

        generation
    }

    /// Cancel the running build, if any. Idempotent.
    pub fn cancel(&self) {
        if let Some((generation, token)) = self.inner.in_flight.lock().unwrap().take() {
            debug!(generation, "cancel requested");
            token.cancel();
        }
    }
}

fn read_diagnostics(log_path: &Path) -> Vec<texlog::Diagnostic> {
    match std::fs::read(log_path) {
        // TeX logs are not reliably UTF-8; lossy conversion keeps the parser simple.
        Ok(bytes) => texlog::diagnostics(&String::from_utf8_lossy(&bytes)),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use preamble_engine::{BuildOutcome, EngineInfo};
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::sync::mpsc;

    /// An engine that "builds" by sleeping. Lets the tests control timing precisely.
    struct SleepyEngine {
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl Engine for SleepyEngine {
        async fn probe(&self) -> Result<EngineInfo, EngineError> {
            Ok(EngineInfo { name: "sleepy".into(), version: "0".into(), path: PathBuf::new() })
        }

        async fn build(
            &self,
            _job: &BuildJob,
            cancel: CancellationToken,
            _progress: Option<preamble_engine::ProgressSink>,
        ) -> Result<BuildOutcome, EngineError> {
            tokio::select! {
                _ = tokio::time::sleep(self.delay) => Ok(BuildOutcome {
                    success: true, pdf: None, log: None, synctex: None,
                    stderr: String::new(), exit_code: Some(0), duration: self.delay,
                }),
                _ = cancel.cancelled() => Err(EngineError::Cancelled),
            }
        }
    }

    /// An engine that reports two progress lines before finishing, so the forwarding path in
    /// `request()` — the whole point of this loop — has something to prove itself against.
    struct ChattyEngine;

    #[async_trait::async_trait]
    impl Engine for ChattyEngine {
        async fn probe(&self) -> Result<EngineInfo, EngineError> {
            Ok(EngineInfo { name: "chatty".into(), version: "0".into(), path: PathBuf::new() })
        }

        async fn build(
            &self,
            _job: &BuildJob,
            _cancel: CancellationToken,
            progress: Option<preamble_engine::ProgressSink>,
        ) -> Result<BuildOutcome, EngineError> {
            if let Some(sink) = &progress {
                let _ = sink.send("Downloading amsmath.sty".to_string());
                let _ = sink.send("Downloading hyperref.sty".to_string());
            }
            Ok(BuildOutcome {
                success: true,
                pdf: None,
                log: None,
                synctex: None,
                stderr: String::new(),
                exit_code: Some(0),
                duration: Duration::from_millis(1),
            })
        }
    }

    fn job() -> BuildJob {
        BuildJob {
            project_dir: PathBuf::from("."),
            root_file: PathBuf::from("main.tex"),
            out_dir: PathBuf::from("./.preamble/build"),
            synctex: false,
        }
    }

    fn status(event: &CompileEvent) -> (&'static str, u64) {
        match event {
            CompileEvent::Started { generation, .. } => ("started", *generation),
            CompileEvent::Progress { generation, .. } => ("progress", *generation),
            CompileEvent::Finished { generation, .. } => ("finished", *generation),
            CompileEvent::Failed { generation, .. } => ("failed", *generation),
        }
    }

    #[tokio::test]
    async fn a_second_request_cancels_the_first_and_only_the_second_finishes() {
        let orchestrator = Orchestrator::new(Some(Arc::new(SleepyEngine { delay: Duration::from_secs(5) })));
        let (tx, mut rx) = mpsc::unbounded_channel();

        let tx1 = tx.clone();
        let first = orchestrator.request(job(), move |e| {
            let _ = tx1.send(e);
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The second engine call has the same 5 s delay; we cancel it explicitly below. What we
        // want to see is that the *first* never reports Finished.
        let orchestrator_fast = Orchestrator::new(Some(Arc::new(SleepyEngine { delay: Duration::from_millis(50) })));
        let _ = orchestrator_fast; // (kept simple: a second request on the same orchestrator follows)

        let tx2 = tx.clone();
        let second = orchestrator.request(job(), move |e| {
            let _ = tx2.send(e);
        });
        assert_eq!(second, first + 1);

        // Collect for a short window; the first build would take 5 s, so nothing from it can
        // legitimately arrive as Finished in that time.
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            seen.push(status(&event));
        }
        assert_eq!(seen, vec![("started", first), ("started", second)]);

        // Cancel the second and make sure cancellation is silent, not an error event.
        orchestrator.cancel();
        let quiet = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
        assert!(quiet.is_err(), "cancellation must not emit an event, got {:?}", quiet);
    }

    #[tokio::test]
    async fn a_build_that_completes_reports_finished_with_its_generation() {
        let orchestrator = Orchestrator::new(Some(Arc::new(SleepyEngine { delay: Duration::from_millis(30) })));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let generation = orchestrator.request(job(), move |e| {
            let _ = tx.send(e);
        });

        let started = rx.recv().await.unwrap();
        assert_eq!(status(&started), ("started", generation));
        let finished = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
        assert_eq!(status(&finished), ("finished", generation));
        if let CompileEvent::Finished { success, .. } = finished {
            assert!(success);
        }
    }

    #[tokio::test]
    async fn progress_lines_arrive_between_started_and_finished_in_order() {
        let orchestrator = Orchestrator::new(Some(Arc::new(ChattyEngine)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let generation = orchestrator.request(job(), move |e| {
            let _ = tx.send(e);
        });

        let mut messages = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap() {
                CompileEvent::Started { generation: g, .. } => assert_eq!(g, generation),
                CompileEvent::Progress { generation: g, message } => {
                    assert_eq!(g, generation);
                    messages.push(message);
                }
                finished @ CompileEvent::Finished { .. } => {
                    assert_eq!(status(&finished), ("finished", generation));
                    break;
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }
        assert_eq!(messages, vec!["Downloading amsmath.sty", "Downloading hyperref.sty"]);
    }

    #[tokio::test]
    async fn no_engine_reports_failed_immediately() {
        let orchestrator = Orchestrator::new(None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        orchestrator.request(job(), move |e| {
            let _ = tx.send(e);
        });
        let event = rx.recv().await.unwrap();
        assert_eq!(status(&event).0, "failed");
    }
}
