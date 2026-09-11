// The flows from DESIGN.md §6, as functions: open a folder, open a file, compile, react to a
// build finishing, react to a file changing on disk. Components call these; nothing else
// mutates `app`.

import { ipc, type CompileEvent, type FsEvent } from './ipc';
import { decideExternalChange, type DocumentBackend } from './document';
import { DocumentManager } from './documents';
import { isTexSource, toRelative } from './paths';
import { app } from './state.svelte';

const backend: DocumentBackend = {
  writeFile: (path, contents) => ipc.writeFile(path, contents),
  diffOps: (oldText, newText) => ipc.diffOps(oldText, newText),
  afterSave: () => void triggerCompile(),
};

/** The live tab set. `state.svelte.ts` holds a reactive snapshot of what this owns; `syncTabs`
 * below is the one place that copies from here into there. */
const manager = new DocumentManager(backend);

let jumpNonce = 0;
let treeRefreshTimer: ReturnType<typeof setTimeout> | null = null;

/** Called once from App.svelte. Subscribes to backend events and probes the engine. */
export async function start(): Promise<void> {
  await ipc.onCompile(handleCompileEvent);
  await ipc.onFsChanged((event) => void handleFsEvent(event));
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
    app.compile = { ...app.compile, phase: 'idle', errors: [], message: null };
    app.notice = null;
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
        errors: [],
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
        errors: event.errors,
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

  // A .tex or .bib changed by something else (git checkout, another editor): recompile. Not
  // while this very file is waiting on an answer, though — building one version of the file
  // while the author is being asked which version they want is noise on top of a question.
  const waitingOnThisFile = app.conflict?.path === relative;
  if (event.exists && isTexSource(relative) && !waitingOnThisFile) await triggerCompile();
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
