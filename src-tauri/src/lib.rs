//! The Abstract-Tex application crate: wires the Rust core to the Tauri window.
//!
//! Module map (each module's own doc comment says what it owns and must never do):
//! - [`project`]  — a folder on disk: file tree, root `.tex` detection, `abstract-tex.toml`.
//! - [`compile`]  — the orchestrator: one build in flight, cancel-and-restart, events.
//! - [`latexdiff`] — change review between two commits: the diff lane and its folder.
//! - [`lsp`]      — the TexLab session: one per open project, its events forwarded to the window.
//! - [`watcher`]  — filesystem events, with our own writes filtered out.
//! - [`git`]      — which repository the open project is in, for the Source Control view.
//! - [`github`]   — the GitHub sign-in session: the waiting, and the events it reports through.
//! - [`synctex`]  — cursor-to-PDF and PDF-to-cursor lookups over `abstract-tex-synctex`.
//! - [`bibliography`] — the `.bib` index: files, entries, citations, rebuilt from disk on change.
//! - [`commands`] — the `#[tauri::command]` functions the frontend calls. Thin by design.
//!
//! State that lives for the whole app is in [`AppState`], handed to Tauri with `.manage()` and
//! borrowed by commands as `State<'_, AppState>`.

pub mod bibliography;
pub mod commands;
pub mod compile;
pub mod consent;
pub mod git;
pub mod github;
pub mod latexdiff;
pub mod lfs;
pub mod lsp;
pub mod paste;
pub mod project;
pub mod synctex;
pub mod watcher;

use std::sync::{Arc, Mutex};

use abstract_tex_engine::tectonic::Tectonic;
use abstract_tex_engine::Engine;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::compile::Orchestrator;
use crate::github::GitHubSession;
use crate::latexdiff::DiffLane;
use crate::lsp::LspSession;
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
    /// Runs change-review builds (S11.4c), beside the live ones and never in their way.
    pub diff_lane: DiffLane,
    /// Keeps the watcher alive; dropping it stops watching.
    pub watcher: Mutex<Option<ProjectWatcher>>,
    /// Content hashes of files *we* wrote, so the watcher can tell our writes from external ones.
    pub written: WrittenHashes,
    /// The language server for the open project. Empty until the frontend asks for one: the
    /// editor must open and compile without TexLab (DESIGN.md §2, commitment 6).
    pub lsp: LspSession,
    /// The GitHub sign-in (S10.4b). Not per project: an account belongs to the person and the
    /// machine, and its token is in the keychain rather than in here.
    pub github: GitHubSession,
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
            orchestrator: Orchestrator::new(engine.clone()),
            diff_lane: DiffLane::new(engine),
            watcher: Mutex::new(None),
            written: WrittenHashes::default(),
            lsp: LspSession::default(),
            github: GitHubSession::default(),
        }
    }
}

/// Entry point called by `main.rs`.
pub fn run() {
    // `RUST_LOG=abstract_tex=debug pnpm tauri dev` turns on verbose output; default is info.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,abstract_tex=debug")))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // S10.4b: opening the person's browser at github.com/login/device.
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        // S9.8: consent lives in the app's own config folder, which only exists once the app
        // does — hence `setup`, not `AppState::new`. Never in a project (`consent.rs`).
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            app.manage(consent::ShellEscapeConsent::at(config.join("shell-escape.toml")));
            Ok(())
        })
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
            commands::shell_escape_allowed,
            commands::allow_shell_escape,
            commands::disallow_shell_escape,
            commands::read_log,
            commands::diff_ops,
            commands::synctex_forward,
            commands::synctex_inverse,
            commands::lsp_start,
            commands::lsp_request,
            commands::lsp_notify,
            commands::lsp_respond,
            commands::bibliography_index,
            commands::bibliography_health,
            commands::identify_paste,
            commands::paste_cite,
            commands::detect_zotero,
            commands::list_zotero_libraries,
            commands::link_zotero_collection,
            commands::unlink_bib_file,
            commands::git_status,
            commands::git_stage,
            commands::git_unstage,
            commands::git_discard,
            commands::git_branch,
            commands::git_log,
            commands::git_commit,
            commands::git_amend,
            commands::git_prose_summary,
            commands::git_initialise,
            commands::git_our_folder_is_ignored,
            commands::git_ignore_our_folder,
            commands::github_account,
            commands::github_sign_in,
            commands::github_cancel_sign_in,
            commands::github_sign_out,
            commands::github_create_repository,
            commands::git_origin_url,
            commands::git_push,
            commands::git_sync,
            commands::git_large_files,
            commands::git_track_with_lfs,
            commands::compare_revisions,
            commands::save_comparison_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Abstract-Tex window");
}
