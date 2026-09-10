// The flows from DESIGN.md §6, as functions: open a folder, open a file, compile, react to a
// build finishing, react to a file changing on disk. Components call these; nothing else
// mutates `app`.

import { ipc, type CompileEvent, type FsEvent } from './ipc';
import { OpenDocument, decideExternalChange, type DocumentBackend } from './document';
import { isTexSource, toRelative } from './paths';
import { app } from './state.svelte';

const backend: DocumentBackend = {
  writeFile: (path, contents) => ipc.writeFile(path, contents),
  diffOps: (oldText, newText) => ipc.diffOps(oldText, newText),
  afterSave: () => void triggerCompile(),
};

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
    closeActiveDocument();
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

export async function openFile(relativePath: string): Promise<void> {
  if (app.activePath === relativePath) return;
  // A conflict bar means there are edits in the buffer that exist nowhere else. Switching files
  // would drop them, and the two buttons that resolve it are on screen already.
  if (app.conflict) {
    app.notice = `Answer the question about ${app.conflict.path} first: keep your version, or load the one on disk.`;
    return;
  }
  try {
    // Flush unsaved edits before switching; losing them on a click would be unforgivable.
    await app.activeDoc?.save();
    const text = await ipc.readFile(relativePath);
    closeActiveDocument();
    const doc = new OpenDocument(relativePath, text, backend);
    doc.onDirtyChange = (dirty) => (app.dirty = dirty);
    app.activeDoc = doc;
    app.activePath = relativePath;
    app.dirty = false;
    app.conflict = null;
  } catch (error) {
    app.notice = `Could not open ${relativePath}: ${String(error)}`;
  }
}

function closeActiveDocument() {
  app.activeDoc?.dispose();
  app.activeDoc = null;
  app.activePath = null;
  app.dirty = false;
  // A bar asking about a file that is no longer open would be a question with no answer.
  app.conflict = null;
}

export async function saveNow(): Promise<void> {
  await app.activeDoc?.save();
}

export async function triggerCompile(): Promise<void> {
  if (!app.project) return;
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
  const doc = app.activeDoc;
  app.conflict = null;
  if (!conflict || !doc || doc.path !== conflict.path) return;
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
      };
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
      app.compile = { ...app.compile, phase: 'failed', generation: event.generation, message: event.message };
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
 * Fold one filesystem event into the open buffer, following the table in `document.ts`.
 *
 * Saves are held across the whole of it. Reading the disk and diffing are both `await`s, and a
 * 700 ms debounce firing inside that window would write the buffer out to the file we are in
 * the middle of reconciling — turning the question this function may be about to ask into the
 * automatic merge DESIGN.md §5.6 forbids. They stay held while a conflict bar is on screen,
 * and `resolveConflict` releases them.
 */
async function reconcileOpenDocument(relative: string, existsOnDisk: boolean): Promise<void> {
  const doc = app.activeDoc;
  if (!doc || doc.path !== relative) return; // some other file; the tree refresh was the point

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
        // Stop the loop here; the author decides (DESIGN.md §5.6).
        if (diskText !== null) {
          app.conflict = { path: relative, diskText };
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
