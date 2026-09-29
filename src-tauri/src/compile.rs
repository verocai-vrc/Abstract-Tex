//! The compile orchestrator (DESIGN.md §3, §5.1).
//!
//! Owns exactly one rule: **one build in flight per project**. A new request cancels the running
//! build and starts over; nothing is ever queued. Typing bursts therefore produce one compile,
//! not forty, and the PDF on screen always corresponds to the latest text the author saved.
//!
//! Since S9.9 a build of a chapter also runs a one-chapter *draft* beside it (DESIGN.md §5.1
//! rung 4, `abstract_tex_engine::draft`): same generation, same cancel, and a `Draft` event that
//! can only ever arrive before that generation's `Finished`, never after.
//!
//! This module reports through a callback, not through Tauri directly, so it can be tested with
//! a fake engine and a channel. The `commands` module supplies a callback that emits a window
//! event. It must never read the log beyond handing it to `texlog`, and never touch the editor.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use abstract_tex_engine::draft::{self, DraftJob};
use abstract_tex_engine::latexmk::{EngineChoice, Latexmk};
use abstract_tex_engine::tectonic::Tectonic;
use abstract_tex_engine::{BuildJob, Engine, EngineError};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// What the frontend hears about a build. `generation` increases with every request so the
/// frontend can ignore anything older than the last one it asked for.
///
/// `#[serde(tag = "status")]` serialises as `{"status": "started", ...}`: one JSON shape the
/// TypeScript side can switch on.
// `rename_all` on the enum renames the *variant tags* (`Finished` -> `finished`), not the
// fields inside each variant: without `rename_all_fields`, `pdf_path` serialised as
// `pdf_path` while `src/lib/ipc.ts` declared `pdfPath`. TypeScript read `undefined`, the
// frontend never set `app.pdfUrl`, and the PDF pane stayed blank on a perfectly good build
// — silently, because nothing had failed. Both attributes are needed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
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
    /// A one-chapter draft of this generation's document is ready (S9.9): a stand-in to show
    /// until `Finished` replaces it. Only ever sent before this generation's `Finished` or
    /// `Failed`, and only for a draft that succeeded — a failed draft says nothing, because the
    /// full build's diagnostics are the only ones the author should see.
    Draft {
        generation: u64,
        /// The chapter it typeset, as its `\include` wrote it (`chapters/03-method`).
        chapter: String,
        /// Absolute path to the draft's PDF, under `.abstract-tex/draft/`.
        pdf_path: String,
        duration_ms: u64,
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
    /// Behind a `Mutex` since S9.4: opening a project chooses its engine, so the one set at
    /// startup can be replaced. A running build keeps the `Arc` it cloned and finishes on the
    /// engine it started with.
    engine: Mutex<Option<Arc<dyn Engine>>>,
    /// The token for the build currently running, tagged with its generation.
    in_flight: Mutex<Option<(u64, CancellationToken)>>,
    /// Monotonic counter. `AtomicU64` gives us increment-and-read without a lock.
    generation: AtomicU64,
    /// The task running the last request's build and its draft. Each request's task waits for
    /// the one before it (S9.9): cancelling only *asks* a build to stop, and a draft still
    /// writing into `.abstract-tex/draft/` while the next request's `draft::prepare` clears it
    /// would mix two drafts in one folder. The wait is short — a superseded build was cancelled
    /// first, and a cancelled engine is one `kill` away from gone.
    last_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// The engine a project's `engine` setting asks for (S9.4), and a sentence when it cannot have
/// it. Falls back to the bundled Tectonic rather than to no engine: a project that asked for
/// `pdflatex` on a machine without TeX Live should still build, and say why it built
/// differently. `None` only when Tectonic itself is missing too.
pub fn engine_for(setting: Option<&str>) -> (Option<Arc<dyn Engine>>, Option<String>) {
    let tectonic = || Tectonic::locate().ok().map(|engine| Arc::new(engine) as Arc<dyn Engine>);
    match EngineChoice::parse(setting) {
        Ok(EngineChoice::Tectonic) => (tectonic(), None),
        Ok(EngineChoice::System(program)) => match Latexmk::locate(program) {
            Ok(engine) => (Some(Arc::new(engine)), None),
            Err(_) => (
                tectonic(),
                Some(format!(
                    "This project asks for {}, but no latexmk with {} was found on this machine; \
                     building with the bundled Tectonic instead.",
                    program.binary_name(),
                    program.binary_name()
                )),
            ),
        },
        Err(sentence) => (tectonic(), Some(format!("{sentence} Building with the bundled Tectonic."))),
    }
}

pub struct Orchestrator {
    inner: Arc<Inner>,
}

impl Orchestrator {
    pub fn new(engine: Option<Arc<dyn Engine>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                engine: Mutex::new(engine),
                in_flight: Mutex::new(None),
                generation: AtomicU64::new(0),
                last_task: Mutex::new(None),
            }),
        }
    }

    pub fn engine(&self) -> Option<Arc<dyn Engine>> {
        self.inner.engine.lock().unwrap().clone()
    }

    /// Replace the engine for every build from now on (S9.4: a project's `engine` setting).
    pub fn set_engine(&self, engine: Option<Arc<dyn Engine>>) {
        *self.inner.engine.lock().unwrap() = engine;
    }

    /// Start a build, cancelling whichever one was running. Returns the new generation number.
    /// With `draft`, also typeset that one chapter beside it, if the engine and the last full
    /// build allow (`draft::prepare` decides; most of the time they do not, and nothing is said).
    ///
    /// `on_event` is called from a background task, so it must be `Send + Sync + 'static`:
    /// safe to hand to another thread and not borrowing anything short-lived.
    pub fn request<F>(&self, job: BuildJob, draft: Option<DraftJob>, on_event: F) -> u64
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

        let Some(engine) = self.engine() else {
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
        // Tauri (abstract-tex-engine's module doc). This task's only job is to relay each one as a
        // `CompileEvent::Progress` until the sender side (in the build task below) is dropped.
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<String>();
        let progress_on_event = Arc::clone(&on_event);
        // `tauri::async_runtime::spawn`, not `tokio::spawn`: `request` is called from the
        // synchronous `compile` command, where there is no Tokio context to pick up, and bare
        // `tokio::spawn` panics with "there is no reactor running" the moment a build starts.
        // Tauri's own handle always has a runtime behind it. (Found by running the app: the
        // copy-into-a-throwaway-crate check that S2.2 and S2.7 relied on swaps exactly this
        // call, so it is the one line that verification could not see.)
        let progress_task = tauri::async_runtime::spawn(async move {
            while let Some(message) = progress_rx.recv().await {
                progress_on_event(CompileEvent::Progress { generation, message });
            }
        });

        // Set once this generation has said `Finished` or `Failed`, and read by the draft before
        // it says `Draft`. Both emits happen *while holding this lock*, so they cannot interleave:
        // either the draft got in first, or it sees the flag and stays quiet. A draft arriving
        // after the full PDF would replace a finished build with a partial one.
        let full_reported = Arc::new(Mutex::new(false));

        let inner = Arc::clone(&self.inner);
        let mut last_task = self.inner.last_task.lock().unwrap();
        let previous_task = last_task.take();
        // Tauri's runtime is Tokio; this works in tests too, where Tauri lazily creates one.
        *last_task = Some(tauri::async_runtime::spawn(async move {
            if let Some(previous_task) = previous_task {
                let _ = previous_task.await;
            }
            // Superseded while waiting: a newer request already said `Started`, so say nothing,
            // lay nothing out and spawn nothing. Returning drops `progress_tx`, which ends the
            // forwarder task too.
            if token.is_cancelled() {
                return;
            }

            // The draft is laid out before the full build starts, never beside it: the full build
            // rewrites the `.aux` files `prepare` copies (the `draft` module doc says why).
            let draft_run = draft.and_then(|draft_job| lay_out_draft(&job, draft_job)).map(|(chapter, layout)| {
                // A child token is cancelled with its parent (a newer request), and can also be
                // cancelled alone — below, once the full build has made the draft pointless.
                let draft_token = token.child_token();
                let engine = Arc::clone(&engine);
                let job = job.clone();
                let on_event = Arc::clone(&on_event);
                let full_reported = Arc::clone(&full_reported);
                let cancel = draft_token.clone();
                let handle = tauri::async_runtime::spawn(async move {
                    let Ok(Some(outcome)) = engine.build_draft(&job, &layout, cancel).await else {
                        return; // no draft mode, cancelled, or could not run: silent, all three
                    };
                    // A failed draft is silent: the full build's diagnostics are the ones to show.
                    if !outcome.success {
                        return;
                    }
                    let Some(pdf) = outcome.pdf else { return };
                    let full_reported = full_reported.lock().unwrap();
                    if !*full_reported {
                        on_event(CompileEvent::Draft {
                            generation,
                            chapter,
                            pdf_path: pdf.to_string_lossy().into_owned(),
                            duration_ms: outcome.duration.as_millis() as u64,
                        });
                    }
                });
                (draft_token, handle)
            });

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

            {
                let mut full_reported = full_reported.lock().unwrap();
                *full_reported = true;
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
            }

            // A draft still running once the full PDF exists has nothing left to show. Stop it,
            // and wait, so this task ending means its draft has ended too (`Inner::last_task`).
            if let Some((draft_token, draft_handle)) = draft_run {
                draft_token.cancel();
                let _ = draft_handle.await;
            }
        }));

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

/// Write the draft's wrapper and seed its folder, or say why not in the debug log. `None` is the
/// common case — no full build to borrow numbering from yet, an engine with no draft mode — and
/// never an error the author hears about: the full build runs either way.
fn lay_out_draft(job: &BuildJob, draft_job: DraftJob) -> Option<(String, draft::DraftLayout)> {
    match draft::prepare(job, &draft_job) {
        Ok(Some(layout)) => Some((draft_job.chapter, layout)),
        Ok(None) => None,
        Err(error) => {
            debug!(%error, "no draft: its folder could not be laid out");
            None
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

    /// The frontend reads `event.pdfPath`; serde must spell it that way.
    ///
    /// This test exists because the app shipped with `rename_all = "camelCase"` alone, which
    /// renames variant *tags* and not the fields inside them. Every field of `Finished`
    /// reached TypeScript as snake_case, `event.pdfPath` was `undefined`, and the PDF pane
    /// stayed blank after a build that had succeeded — with no error anywhere, because
    /// nothing had failed. 94 Rust tests and 93 Vitest tests passed throughout: nothing
    /// asserted on the wire format, which is the one thing the two languages must agree on.
    #[test]
    fn finished_serialises_its_fields_in_camel_case() {
        let event = CompileEvent::Finished {
            generation: 1,
            success: true,
            pdf_path: Some("/tmp/main.pdf".to_string()),
            log_path: Some("/tmp/main.log".to_string()),
            diagnostics: Vec::new(),
            duration_ms: 1234,
            stderr: String::new(),
        };
        let json = serde_json::to_value(&event).expect("event serialises");

        // The names `src/lib/ipc.ts` declares, and the tag the frontend switches on.
        assert_eq!(json["status"], "finished");
        assert_eq!(json["pdfPath"], "/tmp/main.pdf");
        assert_eq!(json["logPath"], "/tmp/main.log");
        assert_eq!(json["durationMs"], 1234);

        // The snake_case spellings must not be there at all: a field present under both
        // names would let the frontend keep working while the contract silently rotted.
        assert!(json.get("pdf_path").is_none(), "pdf_path leaked: {json}");
        assert!(json.get("log_path").is_none(), "log_path leaked: {json}");
        assert!(json.get("duration_ms").is_none(), "duration_ms leaked: {json}");
    }

    /// `Started` carries the other two-word field the frontend reads.
    #[test]
    fn started_serialises_root_file_as_camel_case() {
        let event = CompileEvent::Started {
            generation: 7,
            root_file: "main.tex".to_string(),
        };
        let json = serde_json::to_value(&event).expect("event serialises");
        assert_eq!(json["status"], "started");
        assert_eq!(json["rootFile"], "main.tex");
        assert!(json.get("root_file").is_none(), "root_file leaked: {json}");
    }

    /// `Draft` carries two-word fields too; `src/lib/ipc.ts` reads them as `pdfPath`/`durationMs`.
    #[test]
    fn draft_serialises_its_fields_in_camel_case() {
        let event = CompileEvent::Draft { generation: 3, chapter: "chapters/one".into(), pdf_path: "/d/main.pdf".into(), duration_ms: 2060 };
        let json = serde_json::to_value(&event).expect("event serialises");
        assert_eq!(json["status"], "draft");
        assert_eq!(json["chapter"], "chapters/one");
        assert_eq!(json["pdfPath"], "/d/main.pdf");
        assert_eq!(json["durationMs"], 2060);
    }

    use abstract_tex_engine::{BuildOutcome, EngineInfo};
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
            _progress: Option<abstract_tex_engine::ProgressSink>,
        ) -> Result<BuildOutcome, EngineError> {
            tokio::select! {
                _ = tokio::time::sleep(self.delay) => Ok(BuildOutcome {
                    success: true, pdf: None, log: None, synctex: None,
                    stderr: String::new(), exit_code: Some(0), duration: self.delay,
                    steps: Default::default(),
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
            progress: Option<abstract_tex_engine::ProgressSink>,
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
                steps: Default::default(),
            })
        }
    }

    fn job() -> BuildJob {
        BuildJob {
            project_dir: PathBuf::from("."),
            root_file: PathBuf::from("main.tex"),
            out_dir: PathBuf::from("./.abstract-tex/build"),
            synctex: false,
        }
    }

    fn status(event: &CompileEvent) -> (&'static str, u64) {
        match event {
            CompileEvent::Started { generation, .. } => ("started", *generation),
            CompileEvent::Progress { generation, .. } => ("progress", *generation),
            CompileEvent::Finished { generation, .. } => ("finished", *generation),
            CompileEvent::Draft { generation, .. } => ("draft", *generation),
            CompileEvent::Failed { generation, .. } => ("failed", *generation),
        }
    }

    #[tokio::test]
    async fn a_second_request_cancels_the_first_and_only_the_second_finishes() {
        let orchestrator = Orchestrator::new(Some(Arc::new(SleepyEngine { delay: Duration::from_secs(5) })));
        let (tx, mut rx) = mpsc::unbounded_channel();

        let tx1 = tx.clone();
        let first = orchestrator.request(job(), None, move |e| {
            let _ = tx1.send(e);
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The second engine call has the same 5 s delay; we cancel it explicitly below. What we
        // want to see is that the *first* never reports Finished.
        let orchestrator_fast = Orchestrator::new(Some(Arc::new(SleepyEngine { delay: Duration::from_millis(50) })));
        let _ = orchestrator_fast; // (kept simple: a second request on the same orchestrator follows)

        let tx2 = tx.clone();
        let second = orchestrator.request(job(), None, move |e| {
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
        let generation = orchestrator.request(job(), None, move |e| {
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
        let generation = orchestrator.request(job(), None, move |e| {
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
        orchestrator.request(job(), None, move |e| {
            let _ = tx.send(e);
        });
        let event = rx.recv().await.unwrap();
        assert_eq!(status(&event).0, "failed");
    }

    // ---- the draft beside the build (S9.9) ----

    /// A full build and a draft that each take as long as the test says, and a draft that
    /// succeeds or fails as told. It never looks at the draft's layout: `prepare` is real and
    /// tested in its own crate; what is under test here is only the order of the events.
    struct DraftingEngine {
        full: Duration,
        draft: Duration,
        draft_succeeds: bool,
    }

    #[async_trait::async_trait]
    impl Engine for DraftingEngine {
        async fn probe(&self) -> Result<EngineInfo, EngineError> {
            Ok(EngineInfo { name: "drafting".into(), version: "0".into(), path: PathBuf::new() })
        }

        async fn build(
            &self,
            job: &BuildJob,
            cancel: CancellationToken,
            _progress: Option<abstract_tex_engine::ProgressSink>,
        ) -> Result<BuildOutcome, EngineError> {
            SleepyEngine { delay: self.full }.build(job, cancel, None).await
        }

        async fn build_draft(
            &self,
            _job: &BuildJob,
            _layout: &draft::DraftLayout,
            cancel: CancellationToken,
        ) -> Result<Option<BuildOutcome>, EngineError> {
            tokio::select! {
                _ = tokio::time::sleep(self.draft) => Ok(Some(BuildOutcome {
                    success: self.draft_succeeds, pdf: Some(PathBuf::from("/proj/.abstract-tex/draft/build/main.pdf")),
                    log: None, synctex: None, stderr: String::new(), exit_code: Some(0), duration: self.draft,
                    steps: Default::default(),
                })),
                _ = cancel.cancelled() => Err(EngineError::Cancelled),
            }
        }
    }

    /// A project whose last full build succeeded, so `draft::prepare` agrees to lay a draft out.
    fn warm_project() -> (tempfile::TempDir, BuildJob, DraftJob) {
        let project = tempfile::tempdir().unwrap();
        let build = project.path().join(".abstract-tex/build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("main.aux"), "\\relax\n").unwrap();
        std::fs::write(build.join(abstract_tex_engine::incremental::WARM_MARKER), "").unwrap();
        let job = BuildJob { project_dir: project.path().to_path_buf(), root_file: PathBuf::from("main.tex"), out_dir: build, synctex: true };
        let draft = DraftJob { chapter: "chapters/one".into(), dir: project.path().join(".abstract-tex/draft") };
        (project, job, draft)
    }

    /// Every event one request produces, until its `Finished` and a short quiet spell after it,
    /// so a draft that arrived late would be caught rather than missed.
    async fn events_of(engine: DraftingEngine, job: BuildJob, draft: Option<DraftJob>) -> Vec<(&'static str, u64)> {
        let orchestrator = Orchestrator::new(Some(Arc::new(engine)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        orchestrator.request(job, draft, move |e| {
            let _ = tx.send(e);
        });
        let mut seen = Vec::new();
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(400), rx.recv()).await {
            if let CompileEvent::Draft { chapter, pdf_path, .. } = &event {
                assert_eq!(chapter, "chapters/one");
                assert!(pdf_path.ends_with("main.pdf"));
            }
            seen.push(status(&event));
        }
        seen
    }

    #[tokio::test]
    async fn a_draft_faster_than_the_full_build_arrives_between_started_and_finished() {
        let (_project, job, draft) = warm_project();
        let engine = DraftingEngine { full: Duration::from_millis(150), draft: Duration::from_millis(20), draft_succeeds: true };
        assert_eq!(events_of(engine, job, Some(draft)).await, vec![("started", 1), ("draft", 1), ("finished", 1)]);
    }

    #[tokio::test]
    async fn a_draft_slower_than_the_full_build_is_never_shown() {
        let (_project, job, draft) = warm_project();
        let engine = DraftingEngine { full: Duration::from_millis(20), draft: Duration::from_millis(150), draft_succeeds: true };
        assert_eq!(events_of(engine, job, Some(draft)).await, vec![("started", 1), ("finished", 1)]);
    }

    #[tokio::test]
    async fn a_failed_draft_says_nothing() {
        let (_project, job, draft) = warm_project();
        let engine = DraftingEngine { full: Duration::from_millis(150), draft: Duration::from_millis(20), draft_succeeds: false };
        assert_eq!(events_of(engine, job, Some(draft)).await, vec![("started", 1), ("finished", 1)]);
    }

    #[tokio::test]
    async fn no_draft_without_a_warm_full_build_to_borrow_numbering_from() {
        let (project, job, draft) = warm_project();
        std::fs::remove_file(job.out_dir.join(abstract_tex_engine::incremental::WARM_MARKER)).unwrap();
        let engine = DraftingEngine { full: Duration::from_millis(150), draft: Duration::from_millis(20), draft_succeeds: true };
        assert_eq!(events_of(engine, job, Some(draft)).await, vec![("started", 1), ("finished", 1)]);
        assert!(!project.path().join(".abstract-tex/draft").exists(), "nothing laid out");
    }

    // ---- choosing the engine (S9.4) ----

    #[test]
    fn the_default_setting_asks_for_nothing_and_says_nothing() {
        let (_, notice) = engine_for(None);
        assert_eq!(notice, None);
        let (_, notice) = engine_for(Some("tectonic"));
        assert_eq!(notice, None);
    }

    #[test]
    fn a_setting_it_does_not_know_is_explained_not_ignored() {
        let (_, notice) = engine_for(Some("pdftex"));
        let notice = notice.expect("an unknown engine must be reported");
        assert!(notice.contains("\"pdftex\"") && notice.contains("Tectonic"), "{notice}");
    }

    #[test]
    fn a_system_engine_this_machine_lacks_falls_back_with_a_sentence() {
        use abstract_tex_engine::latexmk::TexProgram;
        // Only meaningful where there is no TeX Live: on a machine that has one, the engine is
        // simply used and there is nothing to fall back from.
        if Latexmk::locate(TexProgram::LuaLatex).is_ok() {
            return;
        }
        let (_, notice) = engine_for(Some("lualatex"));
        let notice = notice.expect("a missing system engine must be reported");
        assert!(notice.contains("lualatex") && notice.contains("bundled Tectonic"), "{notice}");
    }

    #[test]
    fn set_engine_replaces_the_engine_for_later_builds() {
        let orchestrator = Orchestrator::new(None);
        assert!(orchestrator.engine().is_none());
        orchestrator.set_engine(Some(Arc::new(SleepyEngine { delay: Duration::from_millis(1) })));
        assert!(orchestrator.engine().is_some());
    }
}
