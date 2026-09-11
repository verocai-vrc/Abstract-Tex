// S2.1 end to end: a file changes on disk, the watcher event reaches the controller, and the
// change lands in the CRDT — or stops at a question. The IPC layer is faked, so this exercises
// every line of the reaction except the Rust on the far side of `invoke`.

import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type { CompileEvent, FsEvent, ProjectInfo, TextOp } from './ipc';

/** The fake disk and the calls made against it. Declared before the mock factory uses it. */
const disk = new Map<string, string>();
const calls = { compiles: 0, writes: [] as Array<{ path: string; contents: string }>, reads: [] as string[] };
let fsHandler: (event: FsEvent) => void = () => {};
let compileHandler: (event: CompileEvent) => void = () => {};

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
  },
}));

const { closeTab, openFile, openFolder, quickOpenPick, resolveConflict, start, toggleQuickOpen, triggerCompile } =
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
      errors: [],
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
