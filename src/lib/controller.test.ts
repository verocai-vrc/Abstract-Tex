// S2.1 end to end: a file changes on disk, the watcher event reaches the controller, and the
// change lands in the CRDT — or stops at a question. The IPC layer is faked, so this exercises
// every line of the reaction except the Rust on the far side of `invoke`.

import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type { CompileEvent, FsEvent, LspEvent, ProjectInfo, TextOp } from './ipc';

/** The fake disk and the calls made against it. Declared before the mock factory uses it. */
const disk = new Map<string, string>();
const calls = {
  compiles: 0,
  writes: [] as Array<{ path: string; contents: string }>,
  reads: [] as string[],
  /** Every `textDocument/*` message the controller sent the language server. */
  lsp: [] as Array<{ method: string; params: any }>,
  lspStarts: 0,
};
/** Set to a message to make `lsp_start` fail, as a machine with no TexLab would. */
let lspStartError: string | null = null;
let fsHandler: (event: FsEvent) => void = () => {};
let compileHandler: (event: CompileEvent) => void = () => {};
let lspHandler: (event: LspEvent) => void = () => {};

const project: ProjectInfo = {
  rootDir: '/proj',
  rootFile: 'main.tex',
  buildDir: '/proj/.preamble/build',
  tree: [],
};

// `vi.mock` with a factory means the real ./ipc — and with it @tauri-apps/api — is never
// imported, so these tests run under plain Node with no webview.
vi.mock('./ipc', () => ({
  ipc: {
    initialProject: async () => null,
    engineInfo: async () => ({ name: 'Tectonic', version: '0.17.0', path: '/bin/tectonic' }),
    openProject: async () => project,
    refreshTree: async () => project,
    readFile: async (path: string) => {
      calls.reads.push(path);
      const text = disk.get(path);
      if (text === undefined) throw new Error(`no such file: ${path}`);
      return text;
    },
    writeFile: async (path: string, contents: string) => {
      calls.writes.push({ path, contents });
      disk.set(path, contents);
    },
    compile: async () => ++calls.compiles,
    diffOps: async (oldText: string, newText: string): Promise<TextOp[]> => {
      // Prefix/suffix trimming, as the Rust reconciler does. Its own correctness is proved by
      // the proptests in preamble-reconcile; here it only has to leave untouched text alone.
      let prefix = 0;
      while (prefix < oldText.length && prefix < newText.length && oldText[prefix] === newText[prefix]) prefix++;
      let suffix = 0;
      while (
        suffix < oldText.length - prefix &&
        suffix < newText.length - prefix &&
        oldText[oldText.length - 1 - suffix] === newText[newText.length - 1 - suffix]
      )
        suffix++;
      return [
        { index: prefix, delete: oldText.length - prefix - suffix, insert: newText.slice(prefix, newText.length - suffix) },
      ];
    },
    readLog: async () => '',
    assetUrl: (p: string) => `asset://${p}`,
    onCompile: async (handler: (event: CompileEvent) => void) => {
      compileHandler = handler;
      return () => {};
    },
    onFsChanged: async (handler: (event: FsEvent) => void) => {
      fsHandler = handler;
      return () => {};
    },
    lspStart: async () => {
      calls.lspStarts++;
      if (lspStartError) throw new Error(lspStartError);
      return {};
    },
    lspRequest: async () => null,
    lspNotify: async (method: string, params: unknown) => {
      calls.lsp.push({ method, params });
    },
    lspRespond: async () => {},
    onLsp: async (handler: (event: LspEvent) => void) => {
      lspHandler = handler;
      return () => {};
    },
  },
}));

const { closeTab, jumpToDiagnostic, openFile, openFolder, quickOpenPick, resolveConflict, start, toggleQuickOpen, triggerCompile } =
  await import('./controller.svelte');
const { app } = await import('./state.svelte');

/** Pretend the watcher saw `path` change, and let the controller finish reacting. */
async function fileChanged(relative: string): Promise<void> {
  fsHandler({ path: `/proj/${relative}`, exists: disk.has(relative) });
  await vi.advanceTimersByTimeAsync(0);
}

/** Type into the open buffer as the author would, without waiting for the 700 ms save. */
function type(text: string): void {
  const doc = app.activeDoc!;
  doc.ytext.insert(doc.ytext.length, text);
}

/** Same, but into a specific tab rather than whichever one is active. */
function typeInto(path: string, text: string): void {
  const doc = app.docs.get(path)!;
  doc.ytext.insert(doc.ytext.length, text);
}

beforeEach(async () => {
  vi.useFakeTimers();
  disk.clear();
  disk.set('main.tex', 'hello');
  calls.compiles = 0;
  calls.writes = [];
  calls.reads = [];
  calls.lsp = [];
  calls.lspStarts = 0;
  lspStartError = null;
  app.conflict = null;
  app.notice = null;
  await start();
  await openFolder('/proj');
  calls.compiles = 0; // the open triggered one; the tests care about the ones after
  calls.reads = []; // ditto for the read the open performed
});

afterEach(() => vi.useRealTimers());

describe('an external change to the open file', () => {
  it('lands in the CRDT and rebuilds when the buffer has nothing to lose', async () => {
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');

    expect(app.activeDoc!.text()).toBe('hello world');
    expect(app.dirty).toBe(false);
    expect(app.conflict).toBeNull();
    expect(calls.compiles).toBe(1);
  });

  it('is ignored when the disk already matches the buffer', async () => {
    await fileChanged('main.tex');
    expect(app.conflict).toBeNull();
    expect(calls.writes).toHaveLength(0);
  });

  it('asks instead of merging when the buffer is dirty, and writes nothing while it asks', async () => {
    type(' there');
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');

    expect(app.conflict).toEqual({ path: 'main.tex', diskText: 'hello world' });
    // The buffer is untouched: no merge happened.
    expect(app.activeDoc!.text()).toBe('hello there');

    // The heart of the loop. The 700 ms debounce was armed by the keystrokes above; if it
    // fires while the bar is up it overwrites the disk and answers for the author.
    await vi.advanceTimersByTimeAsync(5000);
    expect(calls.writes).toHaveLength(0);
    expect(disk.get('main.tex')).toBe('hello world');
    expect(calls.compiles).toBe(0);
  });

  it('does not lose the buffer when the file is deleted underneath it', async () => {
    type(' there');
    disk.delete('main.tex');
    await fileChanged('main.tex');

    expect(app.activeDoc!.text()).toBe('hello there');
    expect(app.notice).toContain('no longer on disk');
    expect(app.conflict).toBeNull();
    // Unsaved edits to a file that no longer exists get written back rather than dropped.
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello there');
  });
});

describe('multi-document tabs (S2.3)', () => {
  beforeEach(() => {
    disk.set('sections/results.tex', 'Results.');
  });

  it('opening a second file adds a tab without touching the first', async () => {
    type(' there'); // dirty main.tex, unsaved

    await openFile('sections/results.tex');

    expect(app.openTabs).toEqual(['main.tex', 'sections/results.tex']);
    expect(app.activePath).toBe('sections/results.tex');
    // The first tab's buffer is untouched — opening a second file must not save, reload, or
    // otherwise disturb what the author was in the middle of typing.
    expect(app.docs.get('main.tex')!.text()).toBe('hello there');
    expect(app.dirtyPaths.has('main.tex')).toBe(true);
    expect(calls.writes).toHaveLength(0);
  });

  it('switching back to an already-open tab does not re-read the file from disk', async () => {
    await openFile('sections/results.tex');
    calls.reads = [];

    await openFile('main.tex');
    expect(app.activePath).toBe('main.tex');
    expect(calls.reads).toEqual([]);

    await openFile('sections/results.tex');
    expect(app.activePath).toBe('sections/results.tex');
    expect(calls.reads).toEqual([]);
  });

  it('closing a tab flushes its unsaved edit, then activates a neighbour', async () => {
    await openFile('sections/results.tex');
    type(' two'); // dirty the now-active results.tex

    await closeTab('sections/results.tex');

    expect(disk.get('sections/results.tex')).toBe('Results. two');
    expect(app.openTabs).toEqual(['main.tex']);
    expect(app.activePath).toBe('main.tex');
    expect(app.dirtyPaths.has('sections/results.tex')).toBe(false);
  });

  it('compiling saves every open tab, not only the active one', async () => {
    await openFile('sections/results.tex');
    // main.tex is a background tab now; dirty it without switching back to it.
    typeInto('main.tex', ' there');
    type(' too'); // and the active tab, results.tex

    await triggerCompile();

    expect(disk.get('main.tex')).toBe('hello there');
    expect(disk.get('sections/results.tex')).toBe('Results. too');
    expect(app.dirtyPaths.size).toBe(0);
    expect(calls.compiles).toBe(1);
  });

  it('an external conflict on a background tab brings that tab to the front to ask about it', async () => {
    await openFile('sections/results.tex');
    typeInto('main.tex', ' there'); // dirty the background tab
    disk.set('main.tex', 'hello world'); // and move the disk on underneath it

    fsHandler({ path: '/proj/main.tex', exists: true });
    await vi.advanceTimersByTimeAsync(0);

    expect(app.conflict).toEqual({ path: 'main.tex', diskText: 'hello world' });
    expect(app.activePath).toBe('main.tex'); // brought forward so the bar makes sense
    expect(app.docs.get('main.tex')!.text()).toBe('hello there'); // nothing was merged
  });
});

describe('compile progress (S2.2)', () => {
  it('shows the latest engine line while a build runs, and clears it once the build ends', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    expect(app.compile.progress).toBeNull();

    compileHandler({ status: 'progress', generation: 1, message: 'Downloading amsmath.sty' });
    expect(app.compile.progress).toBe('Downloading amsmath.sty');

    compileHandler({ status: 'progress', generation: 1, message: 'Downloading hyperref.sty' });
    expect(app.compile.progress).toBe('Downloading hyperref.sty');

    compileHandler({
      status: 'finished',
      generation: 1,
      success: true,
      pdfPath: null,
      logPath: null,
      diagnostics: [],
      durationMs: 10,
      stderr: 'Downloading amsmath.sty\nDownloading hyperref.sty\n',
    });
    expect(app.compile.progress).toBeNull();
  });

  it('ignores a progress line from a build that a newer request has already superseded', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });

    // A line from the cancelled build 1 arriving late must not overwrite build 2's state.
    compileHandler({ status: 'progress', generation: 1, message: 'stale line' });
    expect(app.compile.progress).toBeNull();

    compileHandler({ status: 'progress', generation: 2, message: 'current line' });
    expect(app.compile.progress).toBe('current line');
  });
});

describe('resolving a conflict', () => {
  beforeEach(async () => {
    type(' there');
    disk.set('main.tex', 'hello world');
    await fileChanged('main.tex');
    expect(app.conflict).not.toBeNull();
  });

  it('"keep mine" overwrites the disk with the buffer', async () => {
    await resolveConflict('keep-mine');
    expect(app.conflict).toBeNull();
    expect(disk.get('main.tex')).toBe('hello there');
    expect(app.activeDoc!.text()).toBe('hello there');
    expect(app.dirty).toBe(false);
  });

  it('"load from disk" takes the disk version into the buffer and rebuilds', async () => {
    await resolveConflict('load-disk');
    expect(app.conflict).toBeNull();
    expect(app.activeDoc!.text()).toBe('hello world');
    expect(app.dirty).toBe(false);
    expect(calls.writes).toHaveLength(0);
    expect(calls.compiles).toBe(1);
  });

  it('resumes normal saving once answered', async () => {
    await resolveConflict('load-disk');
    type('!');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello world!');
  });
});

describe('quick open (S2.4)', () => {
  it('toggles, and picking a file opens it and puts the list away', async () => {
    disk.set('sections/results.tex', 'Results.');
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(true);
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(false);

    toggleQuickOpen();
    await quickOpenPick('sections/results.tex');
    expect(app.quickOpenVisible).toBe(false);
    expect(app.activePath).toBe('sections/results.tex');
    expect(app.openTabs).toEqual(['main.tex', 'sections/results.tex']);
  });

  it('does nothing without a project open', () => {
    app.project = null; // a list of the files in no folder would be an empty box
    toggleQuickOpen();
    expect(app.quickOpenVisible).toBe(false);
  });
});

describe('diagnostics (S2.7)', () => {
  const underscore = {
    title: '_ used outside maths',
    explanation: '`_` means "subscript" and only works inside maths mode.',
    line: 87,
    severity: 'error' as const,
    rule: 'missing-dollar',
    rawMessage: 'Missing $ inserted.',
  };
  const citation = {
    title: '`knuth1984` is cited but not in the bibliography',
    explanation: 'No entry with the key `knuth1984` was found.',
    line: 7,
    severity: 'warning' as const,
    rule: 'undefined-citation',
    rawMessage: "Citation `knuth1984' on page 1 undefined on input line 7.",
  };

  function finished(success: boolean, diagnostics: Array<typeof underscore | typeof citation>) {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler({
      status: 'finished',
      generation: 1,
      success,
      pdfPath: success ? '/proj/.preamble/build/main.pdf' : null,
      logPath: '/proj/.preamble/build/main.log',
      diagnostics,
      durationMs: 10,
      stderr: '',
    });
  }

  it('a failed build opens the drawer with the sentence, never the raw log', () => {
    finished(false, [underscore]);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(false);
    expect(app.errorCount).toBe(1);
    expect(app.warningCount).toBe(0);
    expect(app.compile.diagnostics[0]!.title).toBe('_ used outside maths');
  });

  it('a clean build with warnings stays quiet but counts them', () => {
    finished(true, [citation]);
    expect(app.drawerOpen).toBe(false); // never shouting when the PDF was produced
    expect(app.warningCount).toBe(1);
    expect(app.errorCount).toBe(0);
  });

  it('jumping to a diagnostic brings the root file to the front first', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    expect(app.activePath).toBe('notes.tex');

    await jumpToDiagnostic(underscore);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(87);
  });

  it('a diagnostic with no line goes nowhere', async () => {
    const before = app.jumpRequest;
    await jumpToDiagnostic({ ...underscore, line: null });
    expect(app.jumpRequest).toBe(before);
  });
});

describe('the language server (S3.2)', () => {
  /** Only the document-sync messages, in order, for readable assertions. */
  const methods = () => calls.lsp.map((c) => c.method);

  it('starts when a project opens and tells the server about the first file', async () => {
    expect(calls.lspStarts).toBe(1);
    expect(app.lspReady).toBe(true);
    expect(app.lspMessage).toBeNull();

    const opens = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(opens).toHaveLength(1);
    expect(opens[0]?.params.textDocument).toMatchObject({
      uri: 'file:///proj/main.tex',
      languageId: 'latex',
      version: 1,
      text: 'hello',
    });
  });

  it('opening another tab opens it on the server too', async () => {
    disk.set('sections/results.tex', 'Results.');
    calls.lsp = [];
    await openFile('sections/results.tex');

    const opens = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(opens[0]?.params.textDocument.uri).toBe('file:///proj/sections/results.tex');
  });

  it('a save sends the new text and then didSave, with a higher version', async () => {
    calls.lsp = [];
    type(' there');
    await vi.advanceTimersByTimeAsync(700); // the debounced save

    expect(methods()).toEqual(['textDocument/didChange', 'textDocument/didSave']);
    const change = calls.lsp[0]!;
    expect(change.params.textDocument.version).toBe(2);
    expect(change.params.contentChanges[0].text).toBe('hello there');
  });

  it('closing a tab closes it on the server', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    calls.lsp = [];
    await closeTab('notes.tex');

    const closes = calls.lsp.filter((c) => c.method === 'textDocument/didClose');
    expect(closes[0]?.params.textDocument.uri).toBe('file:///proj/notes.tex');
  });

  /** The reason the restart path exists: a fresh TexLab has never heard of these files, and
   * carrying on from the old version numbers would leave the two permanently out of step. */
  it('re-opens every document at version 1 after the server restarts', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    type(' more'); // bump main.tex past version 1
    await vi.advanceTimersByTimeAsync(700);
    calls.lsp = [];

    lspHandler({ kind: 'restarted', restarts: 1 });
    await vi.advanceTimersByTimeAsync(0);

    const reopened = calls.lsp.filter((c) => c.method === 'textDocument/didOpen');
    expect(reopened.map((c) => c.params.textDocument.uri)).toEqual([
      'file:///proj/main.tex',
      'file:///proj/notes.tex',
    ]);
    for (const open of reopened) {
      expect(open.params.textDocument.version).toBe(1);
    }
    // And the latest text, not what the file was first opened with. `notes.tex` was the active
    // tab when ' more' was typed, so it is the one that changed.
    expect(reopened[1]?.params.textDocument.text).toBe('notes more');
    expect(reopened[0]?.params.textDocument.text).toBe('hello');
    expect(app.lspReady).toBe(true);
  });

  it('a stopped server is reported quietly and stops further traffic', async () => {
    lspHandler({ kind: 'stopped', message: 'TexLab crashed 5 times' });
    calls.lsp = [];

    expect(app.lspReady).toBe(false);
    expect(app.lspMessage).toBe('TexLab crashed 5 times');
    // A notice is for something the author must act on; losing completion is not that.
    expect(app.notice).toBeNull();

    type(' more');
    await vi.advanceTimersByTimeAsync(700);
    expect(calls.lsp).toHaveLength(0);
  });

  /** The commitment that matters most here: DESIGN.md §2 number 6. No TexLab, no language
   * features — and everything else works exactly as before. */
  it('opens the project and compiles normally when TexLab is missing', async () => {
    lspStartError = 'No TexLab binary found. Run `pnpm fetch-lsp`.';
    calls.lsp = [];
    await openFolder('/proj');

    expect(app.lspReady).toBe(false);
    expect(app.lspMessage).toContain('No TexLab binary found');
    expect(app.notice).toBeNull(); // not raised at the author
    expect(calls.lsp).toHaveLength(0); // nothing sent to a server that is not there

    // The editor is fully alive: the file opened, and typing still saves and rebuilds.
    expect(app.activePath).toBe('main.tex');
    calls.compiles = 0;
    type(' there');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('hello there');
    expect(calls.compiles).toBe(1);
  });
});
