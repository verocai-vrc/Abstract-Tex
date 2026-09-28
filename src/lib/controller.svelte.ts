// The flows from DESIGN.md §6, as functions: open a folder, open a file, compile, react to a
// build finishing, react to a file changing on disk. Components call these; nothing else
// mutates `app`.

import type { EditorView } from '@codemirror/view';
import { bibliography, lineAtByteOffset } from './bibliography.svelte';
import { registerCommand } from './commands';
import { ipc, type CompileEvent, type Diagnostic, type Finding, type FsEvent, type LspEvent } from './ipc';
import { decideExternalChange, type DocumentBackend } from './document';
import { DocumentManager } from './documents';
import { diagnosticTarget as targetOf, type DrawerFilter } from './drawer';
import { firstLocation } from './editor/definition';
import { flattenSymbols, type FlatSymbol } from './editor/symbols';
import { LspClient, uriToPath } from './lsp';
import { LspDiagnosticStore, type EditorDiagnostic } from './lsp-diagnostics';
import { isPublishDiagnosticsParams, type Hover } from './lsp-protocol';
import { mergeOutline, scanOutline, type OutlineItem } from './outline';
import { shouldCompileFor, toRelative } from './paths';
import { app } from './state.svelte';

const backend: DocumentBackend = {
  writeFile: (path, contents) => ipc.writeFile(path, contents),
  diffOps: (oldText, newText) => ipc.diffOps(oldText, newText),
  afterSave: (path) => {
    // The server hears about the new text here rather than on every keystroke: a save is
    // already debounced to 700 ms idle, and `didChange` is the message TexLab reparses on.
    syncDocumentToServer(path);
    void triggerCompile();
  },
};

/** The live tab set. `state.svelte.ts` holds a reactive snapshot of what this owns; `syncTabs`
 * below is the one place that copies from here into there. */
const manager = new DocumentManager(backend);

/** Keeps TexLab's idea of each open file in step with ours. Every call is best-effort: the
 * editor must work with no language server at all (DESIGN.md §2, commitment 6), so nothing here
 * is ever awaited by a path the author is waiting on. */
const lsp = new LspClient({
  request: (method, params) => ipc.lspRequest(method, params),
  notify: (method, params) => ipc.lspNotify(method, params),
});

/** Everything TexLab has said is wrong with each file, keyed by URI. Deliberately not a rune:
 * see `app.lspDiagnosticsVersion`, which is the reactive half of this pair. */
const lspDiagnostics = new LspDiagnosticStore();

/** The language server's absolute path for a project-relative one. LSP addresses files by URI,
 * and a URI needs the whole path; the rest of the app speaks in relative paths. */
function absolutePath(relativePath: string): string | null {
  const root = app.project?.rootDir;
  return root ? `${root}/${relativePath}` : null;
}

/** Run a language-server call, swallowing failures. A dead or missing server must never turn
 * into a notice about something the author did not ask for. */
function tellServer(work: (absolute: string) => Promise<unknown>, relativePath: string): void {
  if (!app.lspReady) return;
  const absolute = absolutePath(relativePath);
  if (!absolute) return;
  void work(absolute).catch((error) => console.warn('language server:', error));
}

/** Send one document's current text to the server, then tell it the file was saved. */
function syncDocumentToServer(relativePath: string): void {
  const text = manager.get(relativePath)?.text();
  if (text === undefined) return;
  tellServer(async (absolute) => {
    await lsp.didChange(absolute, text);
    await lsp.didSave(absolute);
  }, relativePath);
}

/**
 * Ask the language server for completions at a position in the given file, for the editor layer
 * (`completion.ts`) to call. Returns `null` for every case that must degrade silently rather
 * than error — no project, no server, or a `didChange` that failed to send — matching what
 * `tellServer` already does for the fire-and-forget calls above.
 *
 * `didChange` normally rides the 700 ms save debounce (S3.2 note 5), which is fine for
 * `publishDiagnostics` but wrong for completion: an author who has typed `\begin{it` and paused
 * to read the list must not be offered completions computed from the text before those four
 * letters. So this flushes the document's *current* text with `lsp.didChange` before asking —
 * one extra notification per completion request, cheap next to the round trip it precedes, and
 * it keeps `LspClient`'s version bookkeeping as the one source of truth rather than teaching
 * `completion.ts` about versions too.
 */
export async function lspCompletion(
  relativePath: string,
  line: number,
  character: number,
): ReturnType<LspClient['completion']> {
  if (!app.lspReady) return null;
  const absolute = absolutePath(relativePath);
  if (!absolute) return null;
  const text = manager.get(relativePath)?.text();
  if (text !== undefined) {
    try {
      await lsp.didChange(absolute, text);
    } catch (error) {
      console.warn('language server:', error);
      return null;
    }
  }
  try {
    return await lsp.completion(absolute, line, character);
  } catch (error) {
    console.warn('language server:', error);
    return null;
  }
}

/**
 * Ask the language server for hover contents at a position in the given file, for
 * `hover.ts`'s `hoverTooltip` to call. `null` for every degraded case — no project, no server,
 * a dead request — matching `lspCompletion`'s contract, and for the same reason: a hover popup
 * that shows an error where a tooltip should be is a notice nobody asked for (commitment 6).
 *
 * Unlike completion this does not flush a `didChange` first. A hover is read-only and answers
 * from whatever text the server already has; forcing a resend on every pause between keystrokes
 * would be a round trip for a feature nobody would notice was one keystroke stale.
 */
export async function lspHover(relativePath: string, line: number, character: number): Promise<Hover | null> {
  if (!app.lspReady) return null;
  const absolute = absolutePath(relativePath);
  if (!absolute) return null;
  try {
    return await lsp.hover(absolute, line, character);
  } catch (error) {
    console.warn('language server:', error);
    return null;
  }
}

/**
 * Ask the language server where the symbol at a position is defined, and act on the answer:
 * move the cursor if it is in the same file that is already open, or open the target file's tab
 * (creating it if needed) and move the cursor there. Returns whether anything was found, which
 * is all `definitionCommand` needs — the jump itself already happened by the time this resolves.
 *
 * This is the one LSP-backed function in this file that does more than answer a question: a
 * `Location`'s `uri` is the server's business, but turning that into "which tab is this" is
 * `openFile`'s job (S2.3), so go-to-definition has to live where both are reachable rather than
 * in `definition.ts`, which knows about neither.
 */
export async function lspGoToDefinition(relativePath: string, line: number, character: number): Promise<boolean> {
  if (!app.lspReady) return false;
  const absolute = absolutePath(relativePath);
  if (!absolute) return false;
  let answer;
  try {
    answer = await lsp.definition(absolute, line, character);
  } catch (error) {
    console.warn('language server:', error);
    return false;
  }
  const location = firstLocation(answer);
  if (!location) return false;

  const root = app.project?.rootDir;
  const targetPath = root ? toRelative(uriToPath(location.uri), root) : null;
  if (targetPath === null) return false;

  if (app.activePath !== targetPath) await openFile(targetPath);
  jumpToLine(location.range.start.line + 1);
  return true;
}

/**
 * Paste-to-cite (S7.6): `pasteCiteHandler` in `editor/paste.ts` has already recognised `pasted`
 * as a DOI/arXiv id/ISBN and prevented CodeMirror's own paste; this is what actually happens —
 * ask the backend to fetch, deduplicate, and maybe write a new `.bib` entry (`ipc.pasteCite`,
 * S7.6's whole point: "the entry appears, deduplicated," DESIGN.md §5.4), then insert `\cite{key}`
 * at the position the paste landed on. `from`/`to` were captured *before* this awaited, so the
 * insertion still targets the right spot even if the cursor moved during the round trip; if the
 * document was edited there in the meantime CodeMirror simply maps the position through those
 * changes itself; a plain `changes: { from, to, insert }` on a stale-but-still-valid range is
 * exactly what every other deferred edit in this codebase already trusts CodeMirror to do.
 *
 * A failure — no network, nothing found for the identifier, no `.bib` file to append to — is a
 * status message, not a dialog: the author's paste is simply left as it was (no raw text falls
 * back into the buffer, since nothing was ever inserted), same as everywhere else `app.notice`
 * reports a backend rejection without interrupting typing.
 */
export async function pasteCite(pasted: string, view: EditorView, from: number, to: number): Promise<void> {
  try {
    const { key } = await ipc.pasteCite(pasted);
    view.dispatch({ changes: { from, to, insert: `\\cite{${key}}` } });
  } catch (error) {
    app.notice = String(error);
  }
}

/**
 * Probe once for a running Zotero + Better BibTeX (S8.1) and record the answer in
 * `bibliography.zoteroStatus`. Not called automatically — DESIGN.md §5.4's Zotero integration
 * only matters to authors who use Zotero, and polling a local port for everyone else on every
 * session would be work with no visible benefit. A failure (the command itself erroring, not a
 * "not running" answer, which is a normal `ZoteroStatus` value) is a status message like every
 * other backend rejection.
 */
export async function detectZotero(): Promise<void> {
  try {
    bibliography.zoteroStatus = await ipc.detectZotero();
  } catch (error) {
    app.notice = String(error);
  }
}

/**
 * List Zotero's libraries and collections into `bibliography.zoteroLibraries`, for the "link a
 * collection" picker (S8.2). Called when the author opens that picker, not before — the same
 * ask-first shape as `detectZotero`.
 */
export async function listZoteroLibraries(): Promise<void> {
  try {
    bibliography.zoteroLibraries = await ipc.listZoteroLibraries();
  } catch (error) {
    app.notice = String(error);
  }
}

/**
 * Link a Zotero collection (S8.2): ask Better BibTeX to auto-export it to
 * `zotero/<collection name>.bib` under the project, add that path to the bibliography index, and
 * clear the picker's library list (`bibliography.zoteroLibraries`) so a re-open never shows a
 * stale snapshot. The chosen output path is deliberately a fixed, predictable location rather
 * than something the author names — DESIGN.md §2 rule 4 ("zero setup"), and one Zotero-managed
 * `.bib` per collection under `zotero/` is enough naming scheme for now; nothing stops a second
 * loop from letting the author choose if this turns out to matter.
 */
export async function linkZoteroCollection(collection: { path: string; name: string }): Promise<void> {
  const outputPath = `zotero/${collection.name.replace(/[\\/:*?"<>|]/g, '_')}.bib`;
  try {
    await ipc.linkZoteroCollection(collection.path, outputPath);
    bibliography.zoteroLibraries = null;
  } catch (error) {
    app.notice = String(error);
  }
}

/**
 * Unlink a linked `.bib` (S8.7). The backend re-indexes and emits `bibliography:changed`, which is
 * what updates every list showing it; a failure (say, `abstract-tex.toml` not writable) becomes the
 * notice, the same way `linkZoteroCollection` reports one.
 */
export async function unlinkBibFile(path: string): Promise<void> {
  try {
    await ipc.unlinkBibFile(path);
  } catch (error) {
    app.notice = String(error);
  }
}

/**
 * The document's symbol tree, flattened, for whatever first wants one (S4.2's Document map
 * panel, most likely). No UI reads this today; it exists so that loop starts from a request that
 * already works rather than from nothing, the same "plumbing before the visual layer" shape
 * `lsp.ts`'s other three request methods were built in.
 */
export async function lspDocumentSymbols(relativePath: string): Promise<FlatSymbol[]> {
  if (!app.lspReady) return [];
  const absolute = absolutePath(relativePath);
  if (!absolute) return [];
  try {
    const symbols = await lsp.documentSymbols(absolute);
    return flattenSymbols(symbols);
  } catch (error) {
    console.warn('language server:', error);
    return [];
  }
}

/**
 * The language server's diagnostics for one project-relative file, for `Editor.svelte`'s gutter.
 * Empty for a file the server has said nothing about, for a null path, and with no project open.
 *
 * Unlike the compile diagnostics this is *not* restricted to the root file: a
 * `publishDiagnostics` names its own URI, so a dot can be placed on the right tab. `Diagnostic`
 * from the log parser has no file until S5.2, which is why the other effect in `Editor.svelte`
 * still guards on the active tab being the root.
 *
 * `publishVersion` is not read. It exists so the caller can pass `app.lspDiagnosticsVersion` and
 * have a `$derived` genuinely depend on it: the rows live in a plain non-reactive store, and the
 * counter is the only reactive trace of a publish. Taking it as an argument beats a bare
 * `app.lspDiagnosticsVersion;` statement at the call site, which reads like dead code and
 * invites a later reader to delete the subscription along with it.
 */
export function lspDiagnosticsFor(relativePath: string | null, publishVersion?: number): EditorDiagnostic[] {
  void publishVersion;
  if (relativePath === null) return [];
  const absolute = absolutePath(relativePath);
  if (!absolute) return [];
  return lspDiagnostics.forPath(absolute);
}

/** Ask Rust to start TexLab for the open project. Never throws: a missing language server is a
 * degraded mode, not a failure to open the folder. */
async function startLanguageServer(): Promise<void> {
  app.lspReady = false;
  app.lspMessage = null;
  lsp.reset();
  clearLspDiagnostics();
  try {
    await ipc.lspStart();
    app.lspReady = true;
  } catch (error) {
    app.lspMessage = String(error);
  }
}

/** Empty the diagnostic store and tell the gutter effect to redraw. Both halves are needed: the
 * store holds the rows, the counter is what any `$effect` reading them is subscribed to. */
function clearLspDiagnostics(): void {
  lspDiagnostics.clear();
  app.lspDiagnosticsVersion += 1;
}

/** Everything TexLab says without being asked.
 *
 * The lifecycle cases and `publishDiagnostics` (S3.3b). Completion is a request, not a
 * notification, and hover is S3.3c; every other notification method keeps today's silence. */
function handleLspEvent(event: LspEvent): void {
  switch (event.kind) {
    case 'restarted':
      // A fresh process has never heard of our open files, so every one goes back at version 1.
      // Without this the editor and the server disagree about every document from here on, and
      // completion silently answers from stale text.
      app.lspReady = true;
      app.lspMessage = null;
      // Before the resync, not after: the old process's diagnostics describe text the new one
      // has never seen, and the new one will republish for every file as it re-parses them.
      clearLspDiagnostics();
      void lsp.resync().catch((error) => console.warn('language server resync:', error));
      break;
    case 'stopped':
      app.lspReady = false;
      app.lspMessage = event.message;
      lsp.reset();
      // A dead server's opinions must not outlive it: without this, dots for problems the author
      // has since fixed would sit in the gutter with nothing left alive to withdraw them.
      clearLspDiagnostics();
      break;
    case 'notification':
      if (event.method === 'textDocument/publishDiagnostics') {
        // `params` is `unknown` here — it crossed the Tauri event boundary as JSON. Narrowing
        // rather than casting means a malformed payload is dropped silently instead of keying
        // the store under `undefined` (`lsp-protocol.ts`).
        if (isPublishDiagnosticsParams(event.params)) {
          lspDiagnostics.publish(event.params);
          app.lspDiagnosticsVersion += 1;
        }
      }
      // Every other method stays ignored rather than logged: TexLab's progress notifications
      // arrive on every keystroke burst and would drown the console.
      break;
    case 'request':
      // S3.3c routes these.
      break;
  }
}

let jumpNonce = 0;
let treeRefreshTimer: ReturnType<typeof setTimeout> | null = null;
let outlineRefreshTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Rescan the active file for its Document map (S4.2) and, if the language server is answering,
 * merge in its `documentSymbol` structure (`mergeOutline`'s doc comment says why symbols win over
 * the scan on a shared line). No project or no active tab clears the panel rather than leaving
 * a stale outline pointing at a file that is no longer open.
 *
 * `path` is captured before the `await` and checked again after it, the same guard
 * `Editor.svelte`'s per-tab closures use for exactly this reason: a tab switch that lands while
 * `lspDocumentSymbols` is in flight must not paint the new tab with the old tab's structure.
 */
export async function refreshOutline(): Promise<void> {
  const path = app.activePath;
  const doc = app.activeDoc;
  if (!path || !doc) {
    app.outline = [];
    return;
  }
  const scanned = scanOutline(doc.text());
  if (!app.lspReady) {
    app.outline = scanned;
    return;
  }
  const symbols = await lspDocumentSymbols(path);
  if (app.activePath !== path) return; // a different tab is active now; that call's own refresh owns this
  app.outline = mergeOutline(scanned, symbols);
}

/** Coalesce the flood of `onTextChange` calls a busy keystroke burst produces into one rescan
 * ~250 ms after typing stops. Rescanning and asking the language server on every keystroke would
 * put this on the same path as the <16 ms keystroke budget in DESIGN.md §2 — the whole reason
 * `onTextChange` exists as a separate hook from the save debounce is that this one has to be
 * shorter than a save, not that it can skip debouncing altogether. */
function scheduleOutlineRefresh(): void {
  if (outlineRefreshTimer) clearTimeout(outlineRefreshTimer);
  outlineRefreshTimer = setTimeout(() => {
    outlineRefreshTimer = null;
    void refreshOutline();
  }, 250);
}

/** A Document map row was activated: move the cursor to it. The panel does not know what a line
 * request looks like — `jumpToLine` already does, from `jumpToDiagnostic` and go-to-definition. */
export function goToOutlineItem(item: OutlineItem): void {
  jumpToLine(item.line);
}

/** Called once from App.svelte. Subscribes to backend events and probes the engine. */
export async function start(): Promise<void> {
  await ipc.onCompile(handleCompileEvent);
  await ipc.onFsChanged((event) => void handleFsEvent(event));
  await ipc.onLsp(handleLspEvent);
  // Rust rebuilds the index whenever a .bib or .tex changes and sends it whole (S7.2); the
  // frontend only ever replaces its snapshot. Health (S8.3) is not part of that event's payload —
  // it is its own backend read, over the same files, kept in step by asking again right after.
  await ipc.onBibliographyChanged((index) => {
    bibliography.index = index;
    void refreshBibliographyHealth();
  });
  try {
    app.engine = await ipc.engineInfo();
  } catch (error) {
    app.engine = null;
    app.notice = `Could not probe the TeX engine: ${String(error)}`;
  }
  // `abstract-tex <folder>` on the command line, or ABSTRACT_TEX_OPEN in the environment.
  const initial = await ipc.initialProject();
  if (initial) await openFolder(initial);
}

export async function openFolder(path?: string): Promise<void> {
  const chosen = path ?? (await ipc.pickFolder());
  if (!chosen) return;
  try {
    const info = await ipc.openProject(chosen);
    closeAllDocuments();
    app.project = info;
    app.pdfUrl = null;
    app.compile = { ...app.compile, phase: 'idle', diagnostics: [], message: null };
    // The previous project's log must not be what the raw view shows for this one.
    app.showRawLog = false;
    app.rawLog = '';
    app.rawLogFocus = null;
    app.notice = null;
    // The previous project's bibliography is not this one's. The first index arrives from the
    // command below; later ones arrive as events.
    bibliography.index = null;
    void refreshBibliography();
    // Start the language server before opening the first file, so that file's `didOpen` is the
    // server's first news of it. Failure is a status line, not a notice: the editor, the
    // compile loop and the PDF all work without it.
    await startLanguageServer();
    if (info.rootFile) {
      await openFile(info.rootFile);
      await triggerCompile();
    } else {
      app.notice = 'No root .tex file found. Create main.tex, or pick a file and mark it as root.';
    }
  } catch (error) {
    app.notice = String(error);
  }
}

/** Ask Rust for the index as it stands on disk. Only needed once per project, at open; every
 * change after that arrives as a `bibliography:changed` event. A failure is not a notice: the
 * editor, the compile loop and the PDF all work without a bibliography index. */
async function refreshBibliography(): Promise<void> {
  try {
    bibliography.index = await ipc.bibliographyIndex();
  } catch {
    /* no project open any more, or it has no root file yet */
  }
  void refreshBibliographyHealth();
}

/** Ask Rust for the five health findings (S8.3), as they stand on disk. Same "silent on failure"
 * rule as `refreshBibliography`, and the same reason: a bibliography that cannot be checked right
 * now is not a reason to interrupt anything else the author is doing. */
async function refreshBibliographyHealth(): Promise<void> {
  try {
    bibliography.findings = await ipc.bibliographyHealth();
  } catch {
    bibliography.findings = [];
  }
}

/**
 * Open a file in a tab, or switch to it if it already has one (S2.3). Every open tab keeps its
 * own `Y.Doc` for as long as it stays open — switching away and back never re-reads the file or
 * loses an edit, which is what makes it safe to do with no confirmation.
 */
export async function openFile(relativePath: string): Promise<void> {
  if (app.activePath === relativePath) return;
  // A conflict bar means there are edits in some tab's buffer that exist nowhere else.
  // Switching away would leave it answerable by nobody, and the two buttons that resolve it
  // are already on screen for the tab it belongs to.
  if (app.conflict) {
    app.notice = `Answer the question about ${app.conflict.path} first: keep your version, or load the one on disk.`;
    return;
  }
  if (manager.isOpen(relativePath)) {
    app.activePath = relativePath;
    void refreshOutline();
    return;
  }
  try {
    const text = await ipc.readFile(relativePath);
    const doc = manager.open(relativePath, text);
    doc.onDirtyChange = (isDirty) => setDirty(relativePath, isDirty);
    doc.onTextChange = () => scheduleOutlineRefresh();
    syncTabs();
    app.activePath = relativePath;
    tellServer((absolute) => lsp.didOpen(absolute, text), relativePath);
    void refreshOutline();
  } catch (error) {
    app.notice = `Could not open ${relativePath}: ${String(error)}`;
  }
}

/** Close one tab. Flushes any unsaved edit first — closing must never silently drop it. */
export async function closeTab(path: string): Promise<void> {
  if (app.conflict?.path === path) {
    app.notice = `Answer the question about ${path} first: keep your version, or load the one on disk.`;
    return;
  }
  await manager.get(path)?.save();
  const next = manager.close(path);
  syncTabs();
  if (app.activePath === path) app.activePath = next;
  tellServer((absolute) => lsp.didClose(absolute), path);
}

function closeAllDocuments() {
  manager.closeAll();
  syncTabs();
  app.activePath = null;
  // A bar asking about a tab that no longer exists would be a question with no answer.
  app.conflict = null;
  app.quickOpenVisible = false;
  app.commandPaletteVisible = false;
  app.outline = [];
}

/** Copy the manager's tab list and dirty set into reactive state. Called after every open,
 * close, or closeAll — the three operations that change *which* tabs exist. Per-keystroke
 * dirtiness updates go through `setDirty` instead, which does not need to touch tab order. */
function syncTabs() {
  app.openTabs = [...manager.tabs()];
  app.docs = new Map(app.openTabs.map((path) => [path, manager.get(path)!]));
  app.dirtyPaths = new Set([...app.dirtyPaths].filter((path) => manager.isOpen(path)));
}

function setDirty(path: string, isDirty: boolean) {
  const next = new Set(app.dirtyPaths);
  if (isDirty) next.add(path);
  else next.delete(path);
  app.dirtyPaths = next;
}

export async function saveNow(): Promise<void> {
  await app.activeDoc?.save();
}

export async function triggerCompile(): Promise<void> {
  if (!app.project) return;
  // A compile reads whatever is on disk; every open tab's edits have to be there, not only the
  // active tab's (S2.3, save-all on compile).
  await manager.saveAll().catch(() => {
    /* the tab that failed to save is still dirty and will retry on its own debounce; a build
     * against a slightly stale version of one file beats not building at all. */
  });
  try {
    await ipc.compile();
  } catch (error) {
    app.compile = { ...app.compile, phase: 'failed', message: String(error) };
    app.drawerOpen = true;
  }
}

export async function setRootFile(relativePath: string): Promise<void> {
  try {
    app.project = await ipc.setRootFile(relativePath);
    await triggerCompile();
  } catch (error) {
    app.notice = String(error);
  }
}

export async function createFile(relativePath: string): Promise<void> {
  try {
    app.project = await ipc.createFile(relativePath);
    await openFile(relativePath);
  } catch (error) {
    app.notice = String(error);
  }
}

/** Show or hide the `Ctrl P` list. Pressing the chord again while it is up dismisses it. Opening
 * it closes the command palette, so the two never sit on screen together. */
export function toggleQuickOpen(): void {
  if (!app.project) return;
  const opening = !app.quickOpenVisible;
  app.quickOpenVisible = opening;
  if (opening) app.commandPaletteVisible = false;
}

/** The list's answer: open the chosen file and put the list away. */
export async function quickOpenPick(relativePath: string): Promise<void> {
  app.quickOpenVisible = false;
  await openFile(relativePath);
}

/** Show or hide the `Ctrl K` command palette (S4.3). Opening it closes quick-open, for the same
 * reason `toggleQuickOpen` closes this one. */
export function toggleCommandPalette(): void {
  if (!app.project) return;
  const opening = !app.commandPaletteVisible;
  app.commandPaletteVisible = opening;
  if (opening) app.quickOpenVisible = false;
}

// Every action a chord in `shortcuts.ts` reaches gets a matching entry here, run against the
// same function the chord calls — one action, one callback, so the palette can never drift from
// what the keyboard does. Registered once at module load: `registerCommand` only writes into a
// `Map`, so doing it here (rather than from `start()`) needs no project to be open yet, and the
// palette can list "Open folder…" before there is one.
registerCommand({ id: 'save', title: 'Save', category: 'action', shortcut: 'Ctrl S', run: () => void saveNow() });
registerCommand({
  id: 'compile',
  title: 'Build',
  category: 'action',
  shortcut: 'Ctrl B',
  run: () => void triggerCompile(),
});
registerCommand({
  id: 'open-folder',
  title: 'Open folder…',
  category: 'action',
  shortcut: 'Ctrl O',
  run: () => void openFolder(),
});
registerCommand({
  id: 'toggle-focus-mode',
  title: 'Toggle focus mode',
  category: 'action',
  run: () => toggleFocusMode(),
});
registerCommand({
  id: 'toggle-typewriter-mode',
  title: 'Toggle typewriter mode',
  category: 'action',
  run: () => toggleTypewriterMode(),
});
// The drawer and the raw log from the keyboard (S6.3). "Raw log one click away" has to hold with
// the drawer closed too — a clean build closes it — and the palette is that one action.
registerCommand({
  id: 'toggle-drawer',
  title: 'Toggle diagnostics',
  category: 'action',
  run: () => toggleDrawer(),
});
registerCommand({
  id: 'show-raw-log',
  title: 'Show raw log',
  category: 'action',
  run: () => {
    app.drawerOpen = true;
    if (!app.showRawLog) void toggleRawLog();
  },
});

/** Flip focus mode (S4.5): dim every paragraph but the one under the cursor. No chord in
 * `shortcuts.ts` reaches this yet — command palette only, following S4.3's own "Open folder…"
 * entry, which also has no raw keybinding beyond the ones already listed there. */
export function toggleFocusMode(): void {
  app.focusModeEnabled = !app.focusModeEnabled;
}

/** Flip typewriter mode (S4.5): keep the cursor's line vertically centred. Independent of focus
 * mode — either, both, or neither can be on. */
export function toggleTypewriterMode(): void {
  app.typewriterModeEnabled = !app.typewriterModeEnabled;
}

export function jumpToLine(line: number): void {
  app.jumpRequest = { line, nonce: ++jumpNonce };
}

let syncTexNonce = 0;

/**
 * SyncTeX forward search (S3.4): the cursor's line in the active file to a spot in the PDF.
 * Silent on every failure the author did not cause on purpose — no project, no build yet,
 * nothing typeset for a blank line — because this fires on ordinary cursor movement, not on an
 * explicit action, and a notice on every idle pause over a blank line would be noise DESIGN.md
 * §2 commitment 6 does not ask for. `PdfPane.svelte` is what turns the request into a scroll.
 */
export async function syncTexForward(relativePath: string, line: number): Promise<void> {
  if (!app.project) return;
  try {
    const hit = await ipc.synctexForward(relativePath, line);
    app.syncTexScrollRequest = { page: hit.page, x: hit.x, y: hit.y, nonce: ++syncTexNonce };
  } catch {
    /* no build yet, or nothing typeset for this line — both ordinary, neither worth a notice */
  }
}

/**
 * SyncTeX inverse search (S3.5): a double-click in the PDF, in PDF points, to a source line —
 * opening the file first if it has no tab yet (S2.3's `openFile` already does that, so this adds
 * nothing beyond the coordinate translation `synctex_inverse` did in Rust).
 */
export async function syncTexInverse(page: number, x: number, y: number): Promise<void> {
  if (!app.project) return;
  try {
    const hit = await ipc.synctexInverse(page, x, y);
    if (!hit.file) return; // resolved outside the project (a package's own file); nothing to open
    if (app.activePath !== hit.file) await openFile(hit.file);
    jumpToLine(hit.line);
  } catch (error) {
    app.notice = String(error);
  }
}

/** Which tab a diagnostic belongs to. The rule itself lives in `drawer.ts` (S6.3) so the gutter
 * and the drawer's grouping apply the same one; this only supplies the current project's root and
 * its include graph (which `drawer.ts` uses to complete an extensionless `\input`, S6.4). */
function diagnosticTarget(diagnostic: Diagnostic): string | null {
  return targetOf(diagnostic, app.project?.rootFile ?? null, app.project?.documentFiles ?? []);
}

/**
 * Go to where a diagnostic points, in the file it actually happened in (S5.6's `Diagnostic.file`,
 * finally read by something — S2.7's own comment on the gutter has named this gap since sprint 2).
 */
export async function jumpToDiagnostic(diagnostic: Diagnostic): Promise<void> {
  if (diagnostic.line === null) return;
  const target = diagnosticTarget(diagnostic);
  if (target && app.activePath !== target) await openFile(target);
  jumpToLine(diagnostic.line);
}

/**
 * Go to where a bibliography health finding (S8.3) points: a `.tex` line for an undefined
 * citation, or a `.bib` entry's span otherwise. A span is bytes, not the UTF-16 units `jumpToLine`
 * ultimately drives CodeMirror with, so a `BibEntry` jump opens the file (if it is not already the
 * active tab) and reads *that* file's just-opened text to convert — the same "open, then use the
 * text that came back" order `openFile` itself already applies, since the file may not have been
 * open at all before this call.
 */
export async function jumpToFinding(finding: Finding): Promise<void> {
  const { jump } = finding;
  if (jump.kind === 'missingFile') return; // S8.6: the file is not on disk; there is nothing to open
  if (jump.kind === 'texLine') {
    if (app.activePath !== jump.file) await openFile(jump.file);
    jumpToLine(jump.line);
    return;
  }
  if (app.activePath !== jump.file) await openFile(jump.file);
  const doc = manager.get(jump.file);
  if (!doc) return; // open failed; openFile already left a notice
  jumpToLine(lineAtByteOffset(doc.text(), jump.span.start));
}

/**
 * Apply a rule's one-click fix (S5.5's `Fix`, S6.2 makes it real): open the diagnosed file if
 * needed, then hand the line and the fix to the document's own `applyFix`, which does the actual
 * find/replace through its Y.Doc transaction — the same path every keystroke takes, so Ctrl-Z
 * undoes it and the 700 ms save debounce picks it up like any other edit. Requires a real target
 * file: a diagnostic with no `file` and no project root to fall back to could only guess which
 * tab to edit, and DESIGN.md §5.2's "a fix may only be automatic when it cannot be wrong" applies
 * exactly as hard to picking the file as to picking the edit. Returns whether the fix actually
 * applied, and leaves a notice rather than silently doing nothing when it did not — the same
 * shape `openFile`'s own failure path already uses.
 */
export async function applyDiagnosticFix(diagnostic: Diagnostic): Promise<boolean> {
  const fix = diagnostic.fix;
  if (!fix || diagnostic.line === null) return false;
  const target = diagnosticTarget(diagnostic);
  if (!target) return false;
  if (app.activePath !== target) await openFile(target);
  const doc = manager.get(target);
  if (!doc) return false;
  const applied = doc.applyFix(diagnostic.line, fix);
  if (applied) {
    jumpToLine(diagnostic.line);
  } else {
    app.notice = `Could not find "${fix.find}" on line ${diagnostic.line} of ${target} — nothing was changed.`;
  }
  return applied;
}

/** Read `main.log` into `app.rawLog`. A read failure becomes the view's own text rather than a
 * notice: the author asked to see the log, so the place they are looking is where the answer
 * goes, even when the answer is "there is no log yet". */
async function refreshRawLog(): Promise<void> {
  try {
    app.rawLog = await ipc.readLog();
  } catch (error) {
    app.rawLog = String(error);
  }
}

export async function toggleRawLog(): Promise<void> {
  app.showRawLog = !app.showRawLog;
  if (app.showRawLog) await refreshRawLog();
}

/**
 * The per-card "Raw log" (S6.3): open the drawer on the raw view, scrolled to TeX's own words for
 * this one diagnostic. Where those words are is `drawer.ts`'s `locateInLog`, run by the view over
 * the log text once it has it; this only says which words to look for. The one place a raw TeX
 * line is shown, and only ever on request (DESIGN.md §2, commitment 3).
 */
export async function showRawLogFor(diagnostic: Diagnostic): Promise<void> {
  app.drawerOpen = true;
  app.showRawLog = true;
  app.rawLogFocus = { rawMessage: diagnostic.rawMessage, nonce: (app.rawLogFocus?.nonce ?? 0) + 1 };
  await refreshRawLog();
}

/** Show or hide the drawer. The toolbar button and the palette command both come here rather than
 * flipping `app.drawerOpen` themselves, so a later rule about opening it (say, never over a
 * conflict bar) has one place to live. */
export function toggleDrawer(): void {
  app.drawerOpen = !app.drawerOpen;
}

/** Change part of the drawer's filter (S6.3), leaving the rest as it was. */
export function setDrawerFilter(change: Partial<DrawerFilter>): void {
  app.drawerFilter = { ...app.drawerFilter, ...change };
}

/** Keep the author's buffer and overwrite the disk, or take the disk version into the buffer. */
export async function resolveConflict(choice: 'keep-mine' | 'load-disk'): Promise<void> {
  const conflict = app.conflict;
  app.conflict = null;
  const doc = conflict ? manager.get(conflict.path) : undefined;
  if (!conflict || !doc) return;
  // The bar has been holding the 700 ms debounce back so that it could not answer for the
  // author. They have answered now, so writing is allowed again either way.
  doc.releaseSaves();
  if (choice === 'keep-mine') {
    // Force a write even if the debounce has not fired; the write is remembered, so the
    // watcher will not report it back as another external change.
    await doc.save();
  } else {
    await doc.applyExternal(conflict.diskText);
    await triggerCompile();
  }
}

function handleCompileEvent(event: CompileEvent): void {
  switch (event.status) {
    case 'started':
      app.compile = {
        phase: 'running',
        generation: event.generation,
        startedAt: Date.now(),
        durationMs: null,
        diagnostics: [],
        message: null,
        stderr: '',
        progress: null,
      };
      break;
    case 'progress':
      // Superseded builds keep streaming lines after a newer one has already started; a stale
      // line would flash "Downloading…" from a build nobody is waiting on any more.
      if (event.generation !== app.compile.generation) return;
      app.compile = { ...app.compile, progress: event.message };
      break;
    case 'finished': {
      if (event.generation < app.compile.generation) return; // superseded
      app.compile = {
        phase: event.success ? 'ok' : 'error',
        generation: event.generation,
        startedAt: app.compile.startedAt,
        durationMs: event.durationMs,
        diagnostics: event.diagnostics,
        message: null,
        stderr: event.stderr,
        progress: null,
      };
      if (event.success && event.pdfPath) {
        // The query string defeats the webview's cache; the path itself never changes.
        app.pdfUrl = `${ipc.assetUrl(event.pdfPath)}?v=${event.generation}`;
      }
      // A failed build opens the drawer; a clean build closes it (never shouting when nothing
      // is wrong, DESIGN.md §6). The raw log view is never the default.
      app.drawerOpen = !event.success;
      if (event.success) app.showRawLog = false;
      // The excerpt a card asked the raw view to highlight belonged to the log this build just
      // overwrote; and if the raw view is still showing, what it shows must be the new log, not
      // the one read when it was opened (`bugs-issues-fixes.md`, S6.3).
      app.rawLogFocus = null;
      if (app.showRawLog) void refreshRawLog();
      break;
    }
    case 'failed':
      if (event.generation < app.compile.generation) return;
      app.compile = {
        ...app.compile,
        phase: 'failed',
        generation: event.generation,
        message: event.message,
        progress: null,
      };
      app.drawerOpen = true;
      break;
  }
}

async function handleFsEvent(event: FsEvent): Promise<void> {
  const project = app.project;
  if (!project) return;
  const relative = toRelative(event.path, project.rootDir);
  if (relative === null) return;

  scheduleTreeRefresh();

  await reconcileOpenDocument(relative, event.exists);

  // A compile input changed by something else (git checkout, another editor): recompile. Not
  // while this very file is waiting on an answer, though — building one version of the file
  // while the author is being asked which version they want is noise on top of a question.
  // `shouldCompileFor` (S4.1) is what keeps a change to a `.tex` file nothing includes from
  // triggering a rebuild, while still recompiling for every `.tex` when the include graph
  // doesn't fully resolve — see its own doc comment for why that is the safe default.
  const waitingOnThisFile = app.conflict?.path === relative;
  if (event.exists && shouldCompileFor(relative, project) && !waitingOnThisFile) await triggerCompile();
}

/**
 * Fold one filesystem event into whichever open tab it is about, following the table in
 * `document.ts`. Any tab can receive this, not only the active one — S2.3 keeps every open
 * file's `Y.Doc` alive, and disk does not know or care which tab has focus.
 *
 * Saves are held across the whole of it. Reading the disk and diffing are both `await`s, and a
 * 700 ms debounce firing inside that window would write the buffer out to the file we are in
 * the middle of reconciling — turning the question this function may be about to ask into the
 * automatic merge DESIGN.md §5.6 forbids. They stay held while a conflict bar is on screen,
 * and `resolveConflict` releases them.
 */
async function reconcileOpenDocument(relative: string, existsOnDisk: boolean): Promise<void> {
  const doc = manager.get(relative);
  if (!doc) return; // not open in any tab; the tree refresh was the point

  doc.holdSaves();
  let keepHeldForConflict = false;
  try {
    let diskText: string | null = null;
    if (existsOnDisk) {
      try {
        diskText = await ipc.readFile(relative);
      } catch {
        return; // a transient read failure is not an answer; the next event will bring another
      }
    }

    const decision = decideExternalChange({
      isOpenFile: true,
      existsOnDisk,
      sameAsBuffer: diskText === doc.text(),
      bufferDirty: doc.dirty,
    });

    // `apply` and `conflict` are the two rows that need the disk text, and the table only
    // reaches them when the file exists — which is exactly when the read above ran. The
    // `!== null` guards are there so that stays true for the type checker as well as the
    // reader; neither of them can be false in practice.
    switch (decision) {
      case 'ignore':
        break;
      case 'apply':
        if (diskText !== null) await doc.applyExternal(diskText);
        break;
      case 'conflict':
        // Stop the loop here; the author decides (DESIGN.md §5.6). Bring the tab to the front
        // so the bar is not about to ask a question over a file nobody is looking at.
        if (diskText !== null) {
          app.conflict = { path: relative, diskText };
          app.activePath = relative;
          keepHeldForConflict = true;
        }
        break;
      case 'vanished':
        // The text is still the author's and we will not throw it away. Releasing before
        // forgetting the disk state means an unsaved buffer writes itself back where the file
        // was, which is the outcome that loses nothing, while a clean buffer only goes dirty
        // and waits for Ctrl S instead of silently resurrecting a file someone just deleted.
        doc.releaseSaves();
        doc.forgetDiskState();
        app.notice = `${relative} is no longer on disk. The text is still in the editor — save it to write the file back.`;
        break;
    }
  } finally {
    if (!keepHeldForConflict) doc.releaseSaves();
  }
}

function scheduleTreeRefresh() {
  if (treeRefreshTimer) clearTimeout(treeRefreshTimer);
  treeRefreshTimer = setTimeout(async () => {
    treeRefreshTimer = null;
    try {
      app.project = await ipc.refreshTree();
    } catch {
      /* the project may have been closed meanwhile */
    }
  }, 250);
}
