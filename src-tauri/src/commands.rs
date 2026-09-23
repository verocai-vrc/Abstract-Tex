//! The commands the frontend can invoke. Thin by design: validate, delegate, emit.
//!
//! Anything worth a unit test lives in `project`, `compile`, `watcher` or a library crate; this
//! file only glues them to Tauri. It must never hold a `Mutex` guard across an `.await`, and it
//! must never hand the frontend a path outside the open project.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use preamble_engine::{BuildJob, EngineInfo};
use preamble_reconcile::TextOp;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::bibliography::{self, BibliographyIndex};
use crate::compile::CompileEvent;
use crate::lsp::LspEvent;
use crate::project::{write_atomically, Project, ProjectInfo};
use crate::synctex::{self, ForwardQuery, ForwardResult, InverseQuery, InverseResult};
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
    folder_from_args(std::env::args().skip(1))
        .or_else(|| std::env::var("PREAMBLE_OPEN").ok().filter(|p| Path::new(p).is_dir()))
}

/// The folder named by the command line, if it names one.
///
/// Takes the arguments rather than reading them, so it can be tested.
///
/// A path with spaces is the case worth the extra code. `preamble C:\My Thesis` reaches us as
/// one argument when it was quoted, but as `["C:\My", "Thesis"]` when it was not — and this
/// repository's own path contains spaces, so the unquoted form is not a corner case. We try the
/// whole tail joined first, then the first argument alone; whichever is a directory wins. The
/// ambiguity is real but harmless: a folder that exists is what the author meant.
fn folder_from_args(args: impl Iterator<Item = String>) -> Option<String> {
    let candidates: Vec<String> = args.take_while(|a| !a.starts_with('-')).collect();
    if candidates.is_empty() {
        return None;
    }
    let joined = candidates.join(" ");
    if Path::new(&joined).is_dir() {
        return Some(joined);
    }
    candidates.into_iter().next().filter(|first| Path::new(first).is_dir())
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
        // The watcher already dropped our own writes, so this is an external change. A `.bib`
        // or `.tex` may have changed what the bibliography index says; rebuild it here, on the
        // watcher's thread, so the frontend learns within one debounce window (S7.2).
        let touches_bibliography = bibliography::affects_index(Path::new(&event.path));
        let _ = emitter.emit("fs:changed", event);
        if touches_bibliography {
            emit_bibliography(&emitter);
        }
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
pub fn write_file(app: AppHandle, state: State<'_, AppState>, path: String, contents: String) -> CommandResult<()> {
    with_project(&state, |project| {
        let absolute = project.resolve(&path)?;
        remember_write(&state.written, &absolute, contents.as_bytes());
        write_atomically(&absolute, &contents)?;
        Ok(())
    })?;
    // The watcher will ignore this write (that is what `remember_write` is for), so an edit to
    // a `.bib` or a new `\cite` in a `.tex` made in our own editor would never reach the
    // index unless it is rebuilt here. Outside `with_project`: the rebuild reads files and
    // must not run under the project lock.
    if bibliography::affects_index(Path::new(&path)) {
        emit_bibliography(&app);
    }
    Ok(())
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

// ---------------------------------------------------------------------------
// Bibliography (S7.2). `bibliography` builds the index from disk; this is the glue that decides
// when, and hands it to the window as the `bibliography:changed` event.
// ---------------------------------------------------------------------------

/// The project's bibliography index, built fresh from disk. Empty — not an error — when there
/// is no root file, because then there is no document to have a bibliography.
#[tauri::command]
pub fn bibliography_index(state: State<'_, AppState>) -> CommandResult<BibliographyIndex> {
    let located = project_root(&state)?;
    Ok(match located {
        Some((root_dir, root_file)) => bibliography::build_index(&root_dir, &root_file),
        None => BibliographyIndex::default(),
    })
}

/// What `identify` recognised a paste as, so the frontend can offer "Cite" without repeating
/// `texbib::acquire::identify`'s own matching rules.
#[tauri::command]
pub fn identify_paste(pasted: String) -> Option<String> {
    match texbib::acquire::identify(&pasted)? {
        texbib::acquire::Identified::Doi(_) => Some("doi".to_string()),
        texbib::acquire::Identified::Arxiv(_) => Some("arxiv".to_string()),
        texbib::acquire::Identified::Isbn(_) => Some("isbn".to_string()),
    }
}

/// What a successful paste-to-cite resolves to: a key to insert as `\cite{key}`, and whether it
/// was already in the bibliography or a new entry was appended.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteCiteResult {
    pub key: String,
    pub created: bool,
}

/// Paste-to-cite (S7.6): identify `pasted`, fetch its entry from the matching source, deduplicate
/// against the project's bibliography, and — on a miss — append the new entry to the first `.bib`
/// file the document names. Async because fetching is a real network request; the request itself
/// runs on a blocking-pool thread (`texbib::acquire`'s fetchers are synchronous `reqwest::blocking`
/// calls, the same shape S2.2's outcome warned an async command must never run directly, since
/// that would stall every other command sharing this runtime's worker thread for as long as the
/// request takes).
#[tauri::command]
pub async fn paste_cite(app: AppHandle, state: State<'_, AppState>, pasted: String) -> CommandResult<PasteCiteResult> {
    let identified = texbib::acquire::identify(&pasted).ok_or_else(|| "Not a DOI, arXiv id, or ISBN.".to_string())?;

    let fetched = tauri::async_runtime::spawn_blocking(move || fetch_identified(identified))
        .await
        .map_err(to_message)?
        .map_err(to_message)?;

    let (root_dir, root_file) = project_root(&state)?.ok_or_else(|| "No root .tex file found.".to_string())?;
    let index = bibliography::build_index(&root_dir, &root_file);

    match crate::paste::resolve_paste(&fetched, &index).map_err(to_message)? {
        crate::paste::PasteOutcome::Existing { key } => Ok(PasteCiteResult { key, created: false }),
        crate::paste::PasteOutcome::New { bib_file, keys_in_use } => {
            let absolute = with_project(&state, |project| project.resolve(&bib_file))?;
            let current_text = std::fs::read_to_string(&absolute).map_err(to_message)?;
            let (key, new_text) = crate::paste::render_new_entry(&fetched, &keys_in_use, &current_text);

            with_project(&state, |project| {
                let absolute = project.resolve(&bib_file)?;
                remember_write(&state.written, &absolute, new_text.as_bytes());
                write_atomically(&absolute, &new_text)?;
                Ok(())
            })?;
            emit_bibliography(&app);
            Ok(PasteCiteResult { key, created: true })
        }
    }
}

/// The blocking half of [`paste_cite`]: one network request, on whichever source `identified`
/// names. Kept as a plain function (not a closure inline in `spawn_blocking`) so the "this runs
/// off the async runtime" boundary is a named, readable line rather than an anonymous block.
fn fetch_identified(identified: texbib::acquire::Identified) -> Result<texbib::Entry, String> {
    match identified {
        texbib::acquire::Identified::Doi(doi) => texbib::acquire::doi::fetch_doi(&doi).map_err(to_message),
        texbib::acquire::Identified::Arxiv(id) => texbib::acquire::arxiv::fetch_arxiv(&id).map_err(to_message),
        texbib::acquire::Identified::Isbn(isbn) => texbib::acquire::isbn::fetch_isbn(&isbn).map_err(to_message),
    }
}

/// Whether Zotero, with the Better BibTeX plugin, is reachable on this machine (S8.1;
/// DESIGN.md §5.4). Async for the same reason `paste_cite` is: `texbib::acquire::zotero::detect`
/// is a synchronous `reqwest::blocking` call, and running it directly on an async command would
/// stall every other command sharing this runtime's worker thread until it times out. A manual
/// command, not a background poll — nothing calls this until the author opens whatever UI offers
/// Zotero linking, matching the card's "detection is a manual command" choice.
#[tauri::command]
pub async fn detect_zotero() -> CommandResult<texbib::acquire::zotero::ZoteroStatus> {
    tauri::async_runtime::spawn_blocking(texbib::acquire::zotero::detect).await.map_err(to_message)
}

/// Rebuild the index and emit it as `bibliography:changed`. Called from the watcher thread and
/// from `write_file`; both take the project lock only long enough to copy two paths out.
fn emit_bibliography(app: &AppHandle) {
    // `app.state()` is how a thread that was not handed `State<'_, AppState>` by Tauri — the
    // watcher's — reaches the same shared state the commands borrow.
    let state = app.state::<AppState>();
    let index = match project_root(&state) {
        Ok(Some((root_dir, root_file))) => bibliography::build_index(&root_dir, &root_file),
        Ok(None) => BibliographyIndex::default(),
        Err(_) => return, // the project was closed meanwhile; nobody is listening
    };
    let _ = app.emit("bibliography:changed", index);
}

/// The open project's folder and root file, or `None` when it has no root file yet.
fn project_root(state: &AppState) -> CommandResult<Option<(PathBuf, PathBuf)>> {
    with_project(state, |project| Ok(project.root_file().map(|root_file| (project.root_dir.clone(), root_file))))
}

// ---------------------------------------------------------------------------
// SyncTeX (S3.4 forward, S3.5 inverse). `synctex` resolves the .synctex.gz path and turns typed
// parser errors into a sentence; this is only the Tauri glue on top of it.
// ---------------------------------------------------------------------------

/// Cursor in the editor → page and point in the PDF (S3.4).
#[tauri::command]
pub fn synctex_forward(state: State<'_, AppState>, query: ForwardQuery) -> CommandResult<ForwardResult> {
    with_project(&state, |project| {
        let root_file = project.root_file().ok_or_else(|| anyhow::anyhow!("No root .tex file found."))?;
        let source = project.resolve(&query.file)?;
        let table = synctex::open(&project.build_dir(), &root_file).map_err(|e| anyhow::anyhow!(e))?;
        table
            .forward_search(&source, query.line)
            .map(ForwardResult::from)
            .ok_or_else(|| anyhow::anyhow!("Nothing typeset for {}:{} in the last build.", query.file, query.line))
    })
}

/// Click in the PDF → file and line in the source (S3.5).
#[tauri::command]
pub fn synctex_inverse(state: State<'_, AppState>, query: InverseQuery) -> CommandResult<InverseResult> {
    with_project(&state, |project| {
        let root_file = project.root_file().ok_or_else(|| anyhow::anyhow!("No root .tex file found."))?;
        let table = synctex::open(&project.build_dir(), &root_file).map_err(|e| anyhow::anyhow!(e))?;
        let position = preamble_synctex::PdfPosition { page: query.page, x: query.x, y: query.y };
        let hit = table
            .inverse_search(position)
            .ok_or_else(|| anyhow::anyhow!("Nothing on page {} of the last build near that point.", query.page))?;
        Ok(synctex::to_relative(&project.root_dir, hit))
    })
}

// ---------------------------------------------------------------------------
// Language server (S3.2). Thin, like everything else here: the session owns the
// process, `preamble-lsp` owns the protocol, and these four functions only pass
// messages between the frontend and the bridge.
// ---------------------------------------------------------------------------

/// Start TexLab for the open project and return its capabilities. Safe to call twice: the
/// session stops whatever was running first.
#[tauri::command]
pub async fn lsp_start(app: AppHandle, state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    // Take what's needed out from under the lock before any `.await` — the module rule in
    // `commands`'s doc comment, and the reason this is not one `with_project` call.
    let (root_dir, build_dir) = {
        let guard = state.project.lock().unwrap();
        let project = guard.as_ref().ok_or_else(|| "No project is open.".to_string())?;
        (project.root_dir.clone(), project.build_dir())
    };

    let emitter = app.clone();
    state
        .lsp
        .start(&root_dir, &build_dir, move |event: LspEvent| {
            let _ = emitter.emit("lsp", event);
        })
        .await
}

/// Send a request and wait for the server's answer. Errors — including "the server went away" —
/// come back as a message, never as a hang.
#[tauri::command]
pub async fn lsp_request(
    state: State<'_, AppState>,
    method: String,
    params: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    let bridge = state.lsp.bridge()?;
    bridge.request(&method, params).await.map_err(to_message)
}

/// Tell the server something without waiting. `textDocument/didChange` goes through here on
/// every keystroke burst, so it must not block (DESIGN.md §2, commitment 2).
#[tauri::command]
pub fn lsp_notify(state: State<'_, AppState>, method: String, params: serde_json::Value) -> CommandResult<()> {
    state.lsp.bridge()?.notify(&method, params).map_err(to_message)
}

/// Answer a request the server made of us, quoting the `id` from the `lsp` event.
#[tauri::command]
pub fn lsp_respond(state: State<'_, AppState>, id: serde_json::Value, result: serde_json::Value) -> CommandResult<()> {
    state.lsp.bridge()?.respond(id, result).map_err(to_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> + use<> {
        list.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn a_quoted_path_with_spaces_opens() {
        let dir = tempfile::tempdir().unwrap();
        let with_spaces = dir.path().join("My Thesis");
        std::fs::create_dir(&with_spaces).unwrap();
        let whole = with_spaces.to_string_lossy().into_owned();
        assert_eq!(folder_from_args(args(&[&whole])), Some(whole));
    }

    /// The case from the smoke run: the shell split the path on its spaces before we saw it.
    #[test]
    fn an_unquoted_path_with_spaces_is_rejoined() {
        let dir = tempfile::tempdir().unwrap();
        let with_spaces = dir.path().join("My Thesis");
        std::fs::create_dir(&with_spaces).unwrap();
        let whole = with_spaces.to_string_lossy().into_owned();
        let split: Vec<&str> = whole.split(' ').collect();
        assert_eq!(folder_from_args(args(&split)), Some(whole));
    }

    #[test]
    fn a_path_without_spaces_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().to_string_lossy().into_owned();
        assert_eq!(folder_from_args(args(&[&plain])), Some(plain));
    }

    #[test]
    fn nothing_is_opened_when_the_argument_is_not_a_directory() {
        assert_eq!(folder_from_args(args(&["/definitely/not/here"])), None);
        assert_eq!(folder_from_args(args(&[])), None);
    }

    #[test]
    fn flags_are_not_mistaken_for_paths() {
        assert_eq!(folder_from_args(args(&["--devtools"])), None);
    }
}
