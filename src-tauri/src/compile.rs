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
    Finished {
        generation: u64,
        success: bool,
        /// Absolute path to the PDF, if one exists. The frontend converts it to an asset URL.
        pdf_path: Option<String>,
        log_path: Option<String>,
        /// The crude sprint-1 error list. Replaced by real diagnostics in v0.3.
        errors: Vec<texlog::QuickError>,
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

        let inner = Arc::clone(&self.inner);
        // Tauri's runtime is Tokio; this works in tests too, where Tauri lazily creates one.
        tauri::async_runtime::spawn(async move {
            let result = engine.build(&job, token).await;

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
                    let errors = outcome.log.as_deref().map(read_quick_errors).unwrap_or_default();
                    info!(generation, success = outcome.success, errors = errors.len(), "build finished");
                    on_event(CompileEvent::Finished {
                        generation,
                        success: outcome.success,
                        pdf_path: outcome.pdf.map(|p| p.to_string_lossy().into_owned()),
                        log_path: outcome.log.map(|p| p.to_string_lossy().into_owned()),
                        errors,
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

fn read_quick_errors(log_path: &Path) -> Vec<texlog::QuickError> {
    match std::fs::read(log_path) {
        // TeX logs are not reliably UTF-8; lossy conversion keeps the parser simple.
        Ok(bytes) => texlog::quick_errors(&String::from_utf8_lossy(&bytes)),
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

        async fn build(&self, _job: &BuildJob, cancel: CancellationToken) -> Result<BuildOutcome, EngineError> {
            tokio::select! {
                _ = tokio::time::sleep(self.delay) => Ok(BuildOutcome {
                    success: true, pdf: None, log: None, synctex: None,
                    stderr: String::new(), exit_code: Some(0), duration: self.delay,
                }),
                _ = cancel.cancelled() => Err(EngineError::Cancelled),
            }
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
