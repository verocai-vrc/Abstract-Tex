// The flows from DESIGN.md §6, as functions: open a folder, open a file, compile, react to a
// build finishing, react to a file changing on disk. Components call these; nothing else
// mutates `app`.

import { ipc, type CompileEvent, type Diagnostic, type FsEvent, type LspEvent } from './ipc';
import { decideExternalChange, type DocumentBackend } from './document';
import { DocumentManager } from './documents';
import { firstLocation } from './editor/definition';
import { flattenSymbols, type FlatSymbol } from './editor/symbols';
import { LspClient, uriToPath } from './lsp';
import { LspDiagnosticStore, type EditorDiagnostic } from './lsp-diagnostics';
import { isPublishDiagnosticsParams, type Hover } from './lsp-protocol';
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

/** Called once from App.svelte. Subscribes to backend events and probes the engine. */
export async function start(): Promise<void> {
  await ipc.onCompile(handleCompileEvent);
  await ipc.onFsChanged((event) => void handleFsEvent(event));
  await ipc.onLsp(handleLspEvent);
  try {
    app.engine = await ipc.engineInfo();
  } catch (error) {
    app.engine = null;
    app.notice = `Could not probe the TeX engine: ${String(error)}`;
  }
  // `preamble <folder>` on the command line, or PREAMBLE_OPEN in the environment.
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
    app.notice = null;
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
    return;
  }
  try {
    const text = await ipc.readFile(relativePath);
    const doc = manager.open(relativePath, text);
    doc.onDirtyChange = (isDirty) => setDirty(relativePath, isDirty);
    syncTabs();
    app.activePath = relativePath;
    tellServer((absolute) => lsp.didOpen(absolute, text), relativePath);
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

/** Show or hide the `Ctrl P` list. Pressing the chord again while it is up dismisses it. */
export function toggleQuickOpen(): void {
  if (!app.project) return;
  app.quickOpenVisible = !app.quickOpenVisible;
}

/** The list's answer: open the chosen file and put the list away. */
export async function quickOpenPick(relativePath: string): Promise<void> {
  app.quickOpenVisible = false;
  await openFile(relativePath);
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

/**
 * Go to where a diagnostic points. Until the paren-stack resolver lands (S5.2) a diagnostic
 * carries a line but no file, so every one is taken to be about the root file — which is
 * right for a single-file paper and the best available guess for anything else.
 */
export async function jumpToDiagnostic(diagnostic: Diagnostic): Promise<void> {
  if (diagnostic.line === null) return;
  const rootFile = app.project?.rootFile;
  if (rootFile && app.activePath !== rootFile) await openFile(rootFile);
  jumpToLine(diagnostic.line);
}

export async function toggleRawLog(): Promise<void> {
  app.showRawLog = !app.showRawLog;
  if (app.showRawLog) {
    try {
      app.rawLog = await ipc.readLog();
    } catch (error) {
      app.rawLog = String(error);
    }
  }
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
