//! The commands the frontend can invoke. Thin by design: validate, delegate, emit.
//!
//! Anything worth a unit test lives in `project`, `compile`, `watcher` or a library crate; this
//! file only glues them to Tauri. It must never hold a `Mutex` guard across an `.await`, and it
//! must never hand the frontend a path outside the open project.

use std::fmt::Display;
use std::path::Path;

use preamble_engine::{BuildJob, EngineInfo};
use preamble_reconcile::TextOp;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::compile::CompileEvent;
use crate::project::{write_atomically, Project, ProjectInfo};
use crate::watcher::{self, remember_write};
use crate::AppState;

/// Tauri needs command errors to be serialisable. A `String` is the simplest thing that is, and
/// every error here is destined for a person anyway, so we flatten `anyhow`/`thiserror` errors
/// to their message at this edge (see the note in `preamble_engine`).
type CommandResult<T> = Result<T, String>;

fn to_message(error: impl Display) -> String {
    error.to_string()
}

/// Run a closure with the open project, or fail with a clear message if none is open.
fn with_project<T>(state: &AppState, f: impl FnOnce(&mut Project) -> anyhow::Result<T>) -> CommandResult<T> {
    let mut guard = state.project.lock().unwrap();
    let project = guard.as_mut().ok_or_else(|| "No project is open.".to_string())?;
    f(project).map_err(to_message)
}

/// A folder to open at launch: the first command-line argument, or `PREAMBLE_OPEN`.
/// `preamble C:\thesis` is how a desktop app is expected to behave; the environment variable
/// is for smoke tests and CI, where passing arguments through `tauri dev` is awkward.
#[tauri::command]
pub fn initial_project() -> Option<String> {
    let from_args = std::env::args().nth(1).filter(|a| !a.starts_with('-'));
    let candidate = from_args.or_else(|| std::env::var("PREAMBLE_OPEN").ok())?;
    let path = Path::new(&candidate);
    path.is_dir().then(|| candidate.clone())
}

/// Which engine will run builds, or `None` if nothing was found.
#[tauri::command]
pub async fn engine_info(state: State<'_, AppState>) -> CommandResult<Option<EngineInfo>> {
    match state.orchestrator.engine() {
        Some(engine) => engine.probe().await.map(Some).map_err(to_message),
        None => Ok(None),
    }
}

/// Open a folder: load config, start watching, allow the PDF pane to read the build folder.
#[tauri::command]
pub fn open_project(app: AppHandle, state: State<'_, AppState>, path: String) -> CommandResult<ProjectInfo> {
    let project = Project::open(Path::new(&path)).map_err(to_message)?;

    // pdf.js loads the PDF over asset://, which only serves paths inside an allowed scope.
    app.asset_protocol_scope()
        .allow_directory(project.build_dir(), true)
        .map_err(to_message)?;

    let emitter = app.clone();
    let watcher = watcher::watch(&project.root_dir, state.written.clone(), move |event| {
        let _ = emitter.emit("fs:changed", event);
    })
    .map_err(to_message)?;

    let info = project.info();
    // Replace the old watcher *after* the new one exists, so there is never a gap.
    *state.watcher.lock().unwrap() = Some(watcher);
    *state.project.lock().unwrap() = Some(project);
    Ok(info)
}

/// Re-list the file tree (after an external change, for instance).
#[tauri::command]
pub fn refresh_tree(state: State<'_, AppState>) -> CommandResult<ProjectInfo> {
    with_project(&state, |project| Ok(project.info()))
}

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> CommandResult<String> {
    with_project(&state, |project| {
        let absolute = project.resolve(&path)?;
        let bytes = std::fs::read(&absolute)?;
        // `.tex` files from other tools are occasionally Latin-1; lossy decoding keeps them
        // openable. A later loop can offer re-encoding; silently failing to open is worse.
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
}

/// Write a file the author edited. Atomic, and remembered so the watcher ignores the echo.
#[tauri::command]
pub fn write_file(state: State<'_, AppState>, path: String, contents: String) -> CommandResult<()> {
    with_project(&state, |project| {
        let absolute = project.resolve(&path)?;
        remember_write(&state.written, &absolute, contents.as_bytes());
        write_atomically(&absolute, &contents)?;
        Ok(())
    })
}

#[tauri::command]
pub fn create_file(state: State<'_, AppState>, path: String) -> CommandResult<ProjectInfo> {
    with_project(&state, |project| {
        let absolute = project.resolve(&path)?;
        if absolute.exists() {
            anyhow::bail!("{path} already exists");
        }
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent)?;
        }
        remember_write(&state.written, &absolute, b"");
        write_atomically(&absolute, "")?;
        Ok(project.info())
    })
}

#[tauri::command]
pub fn set_root_file(state: State<'_, AppState>, path: String) -> CommandResult<ProjectInfo> {
    with_project(&state, |project| {
        project.set_root_file(&path)?;
        Ok(project.info())
    })
}

/// Start a build of the project's root file. Progress arrives as `compile` events.
#[tauri::command]
pub fn compile(app: AppHandle, state: State<'_, AppState>) -> CommandResult<u64> {
    // Gather everything under the lock, then release it before the build starts.
    let job = with_project(&state, |project| {
        let root_file = project.root_file().ok_or_else(|| {
            anyhow::anyhow!("No root .tex file found. Create main.tex or choose a root file in the tree.")
        })?;
        Ok(BuildJob {
            project_dir: project.root_dir.clone(),
            root_file,
            out_dir: project.build_dir(),
            synctex: true,
        })
    })?;

    let emitter = app.clone();
    let generation = state.orchestrator.request(job, move |event: CompileEvent| {
        let _ = emitter.emit("compile", event);
    });
    Ok(generation)
}

#[tauri::command]
pub fn cancel_compile(state: State<'_, AppState>) -> CommandResult<()> {
    state.orchestrator.cancel();
    Ok(())
}

/// The raw log, for the view that is one click away and never the default (DESIGN.md §2).
#[tauri::command]
pub fn read_log(state: State<'_, AppState>) -> CommandResult<String> {
    with_project(&state, |project| {
        let root = project.root_file().ok_or_else(|| anyhow::anyhow!("no root file"))?;
        let stem = root.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
        let log = project.build_dir().join(format!("{stem}.log"));
        match std::fs::read(&log) {
            Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
            Err(_) => Ok(String::new()),
        }
    })
}

/// Diff two texts into CRDT operations. Pure; lives in Rust because `similar` is there and the
/// UTF-16 conversion is the kind of detail we want in one tested place.
#[tauri::command]
pub fn diff_ops(old: String, new: String) -> Vec<TextOp> {
    preamble_reconcile::diff_ops(&old, &new)
}
