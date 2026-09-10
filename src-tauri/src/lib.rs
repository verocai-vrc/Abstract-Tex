//! The Preamble application crate: wires the Rust core to the Tauri window.
//!
//! Module map (each module's own doc comment says what it owns and must never do):
//! - [`project`]  — a folder on disk: file tree, root `.tex` detection, `preamble.toml`.
//! - [`compile`]  — the orchestrator: one build in flight, cancel-and-restart, events.
//! - [`watcher`]  — filesystem events, with our own writes filtered out.
//! - [`commands`] — the `#[tauri::command]` functions the frontend calls. Thin by design.
//!
//! State that lives for the whole app is in [`AppState`], handed to Tauri with `.manage()` and
//! borrowed by commands as `State<'_, AppState>`.

pub mod commands;
pub mod compile;
pub mod project;
pub mod watcher;

use std::sync::{Arc, Mutex};

use preamble_engine::tectonic::Tectonic;
use preamble_engine::Engine;
use tracing_subscriber::EnvFilter;

use crate::compile::Orchestrator;
use crate::project::Project;
use crate::watcher::{ProjectWatcher, WrittenHashes};

/// Everything shared between commands.
///
/// `Mutex` and not `RwLock` because contention is nil: a command runs, takes the lock for
/// microseconds, releases it. Simpler is better here. `std::sync::Mutex` rather than tokio's
/// because we never hold it across an `.await`.
pub struct AppState {
    /// The open project, if any. `None` until the user opens a folder.
    pub project: Mutex<Option<Project>>,
    /// Runs builds. Constructed once with whatever engine was found at startup.
    pub orchestrator: Orchestrator,
    /// Keeps the watcher alive; dropping it stops watching.
    pub watcher: Mutex<Option<ProjectWatcher>>,
    /// Content hashes of files *we* wrote, so the watcher can tell our writes from external ones.
    pub written: WrittenHashes,
}

impl AppState {
    fn new() -> Self {
        // Find an engine now so the UI can say "Tectonic 0.17.0, bundled" before the first build.
        let engine: Option<Arc<dyn Engine>> = match Tectonic::locate() {
            Ok(tectonic) => Some(Arc::new(tectonic)),
            Err(error) => {
                tracing::warn!(%error, "no TeX engine found at startup");
                None
            }
        };
        Self {
            project: Mutex::new(None),
            orchestrator: Orchestrator::new(engine),
            watcher: Mutex::new(None),
            written: WrittenHashes::default(),
        }
    }
}

/// Entry point called by `main.rs`.
pub fn run() {
    // `RUST_LOG=preamble=debug pnpm tauri dev` turns on verbose output; default is info.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,preamble=debug")))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::initial_project,
            commands::engine_info,
            commands::open_project,
            commands::refresh_tree,
            commands::read_file,
            commands::write_file,
            commands::create_file,
            commands::set_root_file,
            commands::compile,
            commands::cancel_compile,
            commands::read_log,
            commands::diff_ops,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Preamble window");
}
