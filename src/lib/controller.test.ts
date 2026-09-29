// S2.1 end to end: a file changes on disk, the watcher event reaches the controller, and the
// change lands in the CRDT — or stops at a question. The IPC layer is faked, so this exercises
// every line of the reaction except the Rust on the far side of `invoke`.

import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type { BibliographyIndex, CompileEvent, Finding, FsEvent, LspEvent, ProjectInfo, TextOp } from './ipc';

/** The fake disk and the calls made against it. Declared before the mock factory uses it. */
const disk = new Map<string, string>();
const calls = {
  compiles: 0,
  /** The `file` each compile was asked to draft a chapter of (S9.9). */
  compileFiles: [] as Array<string | null>,
  /** The `draft` flag of each SyncTeX query, forward or inverse (S9.9). */
  synctexDraft: [] as boolean[],
  /** How often the shell-escape question was put to the person (S9.8). */
  shellEscapeQuestions: 0,
  writes: [] as Array<{ path: string; contents: string }>,
  reads: [] as string[],
  /** Every `textDocument/*` message the controller sent the language server. */
  lsp: [] as Array<{ method: string; params: any }>,
  lspStarts: 0,
  /** How often the raw view asked for `main.log` (S6.3). */
  logReads: 0,
};
/** This machine's shell-escape consent for `/proj` (S9.8), and what the person answers when asked. */
let shellEscapeOnDisk = false;
let shellEscapeAnswer = false;
/** Set to a message to make `lsp_start` fail, as a machine with no TexLab would. */
let lspStartError: string | null = null;
/** What `synctexForward`/`synctexInverse` answer with, or throw, for the S3.4/S3.5 tests below. */
let synctexForwardAnswer: { page: number; x: number; y: number } | Error = { page: 1, x: 10, y: 20 };
let synctexInverseAnswer: { file: string | null; line: number } | Error = { file: 'main.tex', line: 3 };
/** What `lspRequest` answers with next, for the hover/definition/documentSymbol tests below.
 * `unknown` because that is genuinely what crosses the Tauri event boundary; each test narrows
 * it to whatever shape it is pretending TexLab sent. */
let lspRequestAnswer: unknown = null;
/** What `readLog` returns — the `main.log` on the fake disk, for the raw-view tests (S6.3). */
let logOnDisk = '';
/** What `bibliographyIndex` answers with — the index Rust would have built from the fake disk. */
const emptyBibliography: BibliographyIndex = { files: [], entries: [], citations: [], hasNociteStar: false };
let bibliographyOnDisk: BibliographyIndex = emptyBibliography;
let bibliographyHandler: (index: BibliographyIndex) => void = () => {};
let fsHandler: (event: FsEvent) => void = () => {};
let compileHandler: (event: CompileEvent) => void = () => {};
let lspHandler: (event: LspEvent) => void = () => {};

const project: ProjectInfo = {
  rootDir: '/proj',
  rootFile: 'main.tex',
  buildDir: '/proj/.abstract-tex/build',
  tree: [],
  // Every test that triggers a filesystem event uses main.tex (see `fileChanged` below), so a
  // complete graph that lists it is enough to keep `shouldCompileFor` compiling as these tests
  // expect, without every test needing to know about S4.1's include graph.
  documentFiles: ['main.tex'],
  documentFilesComplete: true,
  engineNotice: null,
};

// `vi.mock` with a factory means the real ./ipc — and with it @tauri-apps/api — is never
// imported, so these tests run under plain Node with no webview.
vi.mock('./ipc', () => ({
  ipc: {
    initialProject: async () => null,
    engineInfo: async () => ({ name: 'Tectonic', version: '0.17.0', path: '/bin/tectonic' }),
    openProject: async () => project,
    shellEscapeAllowed: async () => shellEscapeOnDisk,
    allowShellEscape: async () => {
      shellEscapeOnDisk = true;
    },
    disallowShellEscape: async () => {
      shellEscapeOnDisk = false;
    },
    confirmShellEscape: async () => {
      calls.shellEscapeQuestions++;
      return shellEscapeAnswer;
    },
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
    compile: async (file: string | null) => {
      calls.compileFiles.push(file);
      return ++calls.compiles;
    },
    diffOps: async (oldText: string, newText: string): Promise<TextOp[]> => {
      // Prefix/suffix trimming, as the Rust reconciler does. Its own correctness is proved by
      // the proptests in abstract-tex-reconcile; here it only has to leave untouched text alone.
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
    readLog: async () => {
      calls.logReads++;
      return logOnDisk;
    },
    assetUrl: (p: string) => `asset://${p}`,
    synctexForward: async (_file: string, _line: number, draft: boolean) => {
      calls.synctexDraft.push(draft);
      if (synctexForwardAnswer instanceof Error) throw synctexForwardAnswer;
      return synctexForwardAnswer;
    },
    synctexInverse: async (_page: number, _x: number, _y: number, draft: boolean) => {
      calls.synctexDraft.push(draft);
      if (synctexInverseAnswer instanceof Error) throw synctexInverseAnswer;
      return synctexInverseAnswer;
    },
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
    lspRequest: async () => lspRequestAnswer,
    lspNotify: async (method: string, params: unknown) => {
      calls.lsp.push({ method, params });
    },
    lspRespond: async () => {},
    onLsp: async (handler: (event: LspEvent) => void) => {
      lspHandler = handler;
      return () => {};
    },
    bibliographyIndex: async (): Promise<BibliographyIndex> => bibliographyOnDisk,
    bibliographyHealth: async (): Promise<Finding[]> => [],
    onBibliographyChanged: async (handler: (index: BibliographyIndex) => void) => {
      bibliographyHandler = handler;
      return () => {};
    },
  },
}));

const {
  allowShellEscape,
  applyDiagnosticFix,
  applyFindingFix,
  closeTab,
  disallowShellEscape,
  goToOutlineItem,
  jumpToDiagnostic,
  lspDiagnosticsFor,
  lspDocumentSymbols,
  lspGoToDefinition,
  lspHover,
  openFile,
  openFolder,
  quickOpenPick,
  refreshOutline,
  resolveConflict,
  setDrawerFilter,
  showRawLogFor,
  start,
  syncTexForward,
  syncTexInverse,
  toggleDrawer,
  toggleQuickOpen,
  toggleRawLog,
  triggerCompile,
} = await import('./controller.svelte');
const { app } = await import('./state.svelte');
const { bibliography } = await import('./bibliography.svelte');
const { allCommands } = await import('./commands');

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
  calls.compileFiles = [];
  calls.synctexDraft = [];
  calls.shellEscapeQuestions = 0;
  shellEscapeOnDisk = false;
  shellEscapeAnswer = false;
  calls.writes = [];
  calls.reads = [];
  calls.lsp = [];
  calls.lspStarts = 0;
  calls.logReads = 0;
  lspStartError = null;
  lspRequestAnswer = null;
  logOnDisk = '';
  synctexForwardAnswer = { page: 1, x: 10, y: 20 };
  synctexInverseAnswer = { file: 'main.tex', line: 3 };
  bibliographyOnDisk = emptyBibliography;
  app.conflict = null;
  app.notice = null;
  await start();
  await openFolder('/proj');
  calls.compiles = 0; // the open triggered one; the tests care about the ones after
  calls.reads = []; // ditto for the read the open performed
});

afterEach(() => vi.useRealTimers());

describe('a project whose engine setting could not be honoured (S9.4)', () => {
  afterEach(() => {
    project.engineNotice = null;
  });

  it('says so as a notice, and still opens and builds', async () => {
    project.engineNotice = 'This project asks for pdflatex, but no latexmk with pdflatex was found on this machine.';
    await openFolder('/proj');
    expect(app.notice).toBe(project.engineNotice);
    expect(app.activePath).toBe('main.tex');
    expect(calls.compiles).toBe(1);
  });

  it('shows no notice when the setting was honoured', async () => {
    await openFolder('/proj');
    expect(app.notice).toBeNull();
  });
});

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

describe('shell escape by consent (S9.8)', () => {
  it('reads the folder\'s consent when the folder opens', async () => {
    expect(app.shellEscapeAllowed).toBe(false);
    shellEscapeOnDisk = true;
    await openFolder('/proj');
    expect(app.shellEscapeAllowed).toBe(true);
  });

  it('asks first, and a "not now" changes nothing and builds nothing', async () => {
    shellEscapeAnswer = false;
    await allowShellEscape();
    expect(calls.shellEscapeQuestions).toBe(1);
    expect(shellEscapeOnDisk).toBe(false);
    expect(app.shellEscapeAllowed).toBe(false);
    expect(calls.compiles).toBe(0);
  });

  it('records a yes for this folder and builds again with it', async () => {
    shellEscapeAnswer = true;
    await allowShellEscape();
    expect(shellEscapeOnDisk).toBe(true);
    expect(app.shellEscapeAllowed).toBe(true);
    expect(calls.compiles).toBe(1);

    await allowShellEscape();
    expect(calls.shellEscapeQuestions, 'already allowed: nothing to ask').toBe(1);
  });

  it('takes it back without a question', async () => {
    shellEscapeAnswer = true;
    await allowShellEscape();
    await disallowShellEscape();
    expect(calls.shellEscapeQuestions).toBe(1);
    expect(shellEscapeOnDisk).toBe(false);
    expect(app.shellEscapeAllowed).toBe(false);
  });
});

describe('the draft preview (S9.9)', () => {
  const finished = (generation: number, success: boolean): CompileEvent => ({
    status: 'finished',
    generation,
    success,
    pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
    logPath: null,
    diagnostics: [],
    durationMs: 3300,
    stderr: '',
  });
  const draft = (generation: number): CompileEvent => ({
    status: 'draft',
    generation,
    chapter: 'chapters/03-method',
    pdfPath: '/proj/.abstract-tex/draft/build/main.pdf',
    durationMs: 2060,
  });

  it('asks Rust to draft the file being edited', async () => {
    await triggerCompile();
    expect(app.activePath).toBe('main.tex');
    expect(calls.compileFiles.at(-1)).toBe('main.tex');
  });

  it('shows the draft while the full build runs, then the full PDF in its place', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(draft(1));
    expect(app.pdfUrl).toContain('/draft/build/main.pdf');
    expect(app.pdfDraftOf).toBe('chapters/03-method');

    compileHandler(finished(1, true));
    expect(app.pdfUrl).toContain('/build/main.pdf');
    expect(app.pdfUrl).not.toContain('/draft/');
    expect(app.pdfDraftOf).toBeNull();
  });

  it('never lets a draft replace a finished build, nor a superseded build put one on screen', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(finished(1, true));
    const full = app.pdfUrl;
    compileHandler(draft(1)); // late: Rust promises this cannot happen; the screen agrees anyway
    expect(app.pdfUrl).toBe(full);

    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });
    compileHandler({ status: 'started', generation: 3, rootFile: 'main.tex' });
    compileHandler(draft(2));
    expect(app.pdfUrl).toBe(full);
    expect(app.pdfDraftOf).toBeNull();
  });

  it('puts the last full PDF back when the build the draft stood in for fails', () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler(finished(1, true));
    const full = app.pdfUrl;

    compileHandler({ status: 'started', generation: 2, rootFile: 'main.tex' });
    compileHandler(draft(2));
    compileHandler(finished(2, false));
    expect(app.pdfUrl).toBe(full);
    expect(app.pdfDraftOf).toBeNull();
  });

  it('searches the SyncTeX of whichever PDF is on screen', async () => {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    await syncTexForward('main.tex', 3);
    compileHandler(draft(1));
    await syncTexForward('main.tex', 3);
    await syncTexInverse(1, 10, 20);
    compileHandler(finished(1, true));
    await syncTexInverse(1, 10, 20);
    expect(calls.synctexDraft).toEqual([false, true, true, false]);
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
    file: 'main.tex',
    severity: 'error' as const,
    rule: 'missing-dollar',
    rawMessage: 'Missing $ inserted.',
    fix: { description: 'Escape as \\_', find: '_', replace: '\\_' },
  };
  const citation = {
    title: '`knuth1984` is cited but not in the bibliography',
    explanation: 'No entry with the key `knuth1984` was found.',
    line: 7,
    file: 'main.tex',
    severity: 'warning' as const,
    rule: 'undefined-citation',
    rawMessage: "Citation `knuth1984' on page 1 undefined on input line 7.",
    fix: null,
  };

  function finished(success: boolean, diagnostics: Array<typeof underscore | typeof citation>) {
    compileHandler({ status: 'started', generation: 1, rootFile: 'main.tex' });
    compileHandler({
      status: 'finished',
      generation: 1,
      success,
      pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
      logPath: '/proj/.abstract-tex/build/main.log',
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

  it('jumping to a diagnostic brings its own file to the front first', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    expect(app.activePath).toBe('notes.tex');

    await jumpToDiagnostic(underscore);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(87);
  });

  it('jumping to a diagnostic in a different file opens that file, not the root (S5.6 wiring)', async () => {
    disk.set('chapters/intro.tex', 'text\n');
    await jumpToDiagnostic({ ...underscore, file: 'chapters/intro.tex', line: 1 });
    expect(app.activePath).toBe('chapters/intro.tex');
    expect(app.jumpRequest?.line).toBe(1);
  });

  it('a diagnostic with no file at all falls back to the root file', async () => {
    disk.set('notes.tex', 'notes');
    await openFile('notes.tex');
    await jumpToDiagnostic({ ...underscore, file: null });
    expect(app.activePath).toBe('main.tex');
  });

  it('a diagnostic with no line goes nowhere', async () => {
    const before = app.jumpRequest;
    await jumpToDiagnostic({ ...underscore, line: null });
    expect(app.jumpRequest).toBe(before);
  });
});

describe('applying a diagnostic fix (S6.2)', () => {
  const withAmpersand = {
    title: '`&` used outside a table',
    explanation: 'If you meant a literal ampersand, write `\\&` instead.',
    line: 3,
    file: 'main.tex',
    severity: 'error' as const,
    rule: 'misplaced-alignment-tab',
    rawMessage: 'Misplaced alignment tab character &.',
    fix: { description: 'Escape as \\&', find: '&', replace: '\\&' },
  };

  /** `main.tex` is already open by the outer `beforeEach` (`openFolder` opens the root file),
   * so replacing `disk`'s copy would not be seen — a fix always acts on the live buffer, the
   * same as any other edit. Setting the already-open document's own text is what a real external
   * change to that content would look like. */
  function setMainTexBuffer(text: string): void {
    const doc = app.docs.get('main.tex')!;
    doc.ytext.delete(0, doc.ytext.length);
    doc.ytext.insert(0, text);
  }

  beforeEach(() => {
    setMainTexBuffer('one\ntwo\nSalt & pepper.\n');
  });

  it('finds the file, edits it through the CRDT, and saves it on the usual debounce', async () => {
    const applied = await applyDiagnosticFix(withAmpersand);
    expect(applied).toBe(true);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(3);

    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('one\ntwo\nSalt \\& pepper.\n');
  });

  it('opens the file first when the fix belongs to a tab that is not open yet', async () => {
    disk.set('chapters/intro.tex', 'Salt & pepper.\n');
    const applied = await applyDiagnosticFix({ ...withAmpersand, file: 'chapters/intro.tex', line: 1 });
    expect(applied).toBe(true);
    expect(app.activePath).toBe('chapters/intro.tex');
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('chapters/intro.tex')).toBe('Salt \\& pepper.\n');
  });

  it('declines, and leaves a notice, when the named line does not hold the fix anymore', async () => {
    setMainTexBuffer('one\ntwo\nnothing to escape here\n');
    const applied = await applyDiagnosticFix(withAmpersand);
    expect(applied).toBe(false);
    expect(app.notice).toContain('main.tex');
  });

  it('does nothing for a diagnostic with no fix', async () => {
    const applied = await applyDiagnosticFix({ ...withAmpersand, fix: null });
    expect(applied).toBe(false);
  });

  it('does nothing for a diagnostic with no file and no project root to fall back to', async () => {
    app.project = null;
    const applied = await applyDiagnosticFix({ ...withAmpersand, file: null });
    expect(applied).toBe(false);
  });
});

describe('applying a bibliography finding fix (S8.8)', () => {
  const linkedNotNamed: Finding = {
    rule: 'linked-not-named',
    severity: 'error',
    message: "'zotero/Thesis.bib' is linked but the document does not name it.",
    jump: { kind: 'texLine', file: 'main.tex', line: 2 },
    fix: {
      description: 'Add zotero/Thesis to \\bibliography',
      find: '\\bibliography{references}',
      replace: '\\bibliography{references,zotero/Thesis}',
    },
  };

  function setMainTexBuffer(text: string): void {
    const doc = app.docs.get('main.tex')!;
    doc.ytext.delete(0, doc.ytext.length);
    doc.ytext.insert(0, text);
  }

  it('edits the named line through the CRDT, the same way a diagnostic fix does', async () => {
    setMainTexBuffer('\\cite{zot2020}\n\\bibliography{references}\n');
    const applied = await applyFindingFix(linkedNotNamed);
    expect(applied).toBe(true);
    expect(app.jumpRequest?.line).toBe(2);
    await vi.advanceTimersByTimeAsync(700);
    expect(disk.get('main.tex')).toBe('\\cite{zot2020}\n\\bibliography{references,zotero/Thesis}\n');
  });

  it('declines with a notice when the line changed since the index was built', async () => {
    setMainTexBuffer('\\cite{zot2020}\n\\bibliography{other}\n');
    expect(await applyFindingFix(linkedNotNamed)).toBe(false);
    expect(app.notice).toContain('main.tex');
  });

  it('does nothing for a finding with no fix, or one that does not point at a .tex line', async () => {
    expect(await applyFindingFix({ ...linkedNotNamed, fix: null })).toBe(false);
    expect(await applyFindingFix({ ...linkedNotNamed, jump: { kind: 'missingFile', file: 'zotero/Thesis.bib' } })).toBe(
      false,
    );
  });
});

describe('the drawer v1 (S6.3)', () => {
  const underscore = {
    title: '_ used outside maths',
    explanation: '`_` means "subscript" and only works inside maths mode.',
    line: 59,
    file: 'sections/background.tex',
    severity: 'error' as const,
    rule: 'missing-dollar',
    rawMessage: 'Missing $ inserted.',
    fix: { description: 'Escape as \\_', find: '_', replace: '\\_' },
  };

  function finished(generation: number, success: boolean, diagnostics: Array<typeof underscore>) {
    compileHandler({ status: 'started', generation, rootFile: 'main.tex' });
    compileHandler({
      status: 'finished',
      generation,
      success,
      pdfPath: success ? '/proj/.abstract-tex/build/main.pdf' : null,
      logPath: '/proj/.abstract-tex/build/main.log',
      diagnostics,
      durationMs: 10,
      stderr: '',
    });
  }

  beforeEach(() => {
    app.drawerOpen = false;
    app.showRawLog = false;
    app.rawLogFocus = null;
    setDrawerFilter({ severity: 'all', activeFileOnly: false });
  });

  it("a card's \"Raw log\" opens the drawer on the raw view, pointed at that diagnostic's own words", async () => {
    logOnDisk = 'This is XeTeX\n! Missing $ inserted.\nl.59 A stray underscore_\n';
    finished(1, false, [underscore]);
    app.drawerOpen = false; // the author closed it; the button must reopen it

    await showRawLogFor(underscore);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe(logOnDisk);
    expect(app.rawLogFocus?.rawMessage).toBe('Missing $ inserted.');
  });

  it('asking for the same card twice is two distinct requests, so the view scrolls back to it', async () => {
    await showRawLogFor(underscore);
    const first = app.rawLogFocus!.nonce;
    await showRawLogFor(underscore);
    expect(app.rawLogFocus!.nonce).toBeGreaterThan(first);
  });

  it('re-reads the log when a build finishes while the raw view is open (regression: it showed the old log)', async () => {
    logOnDisk = 'first build';
    finished(1, false, [underscore]);
    await toggleRawLog();
    expect(app.rawLog).toBe('first build');

    logOnDisk = 'second build';
    finished(2, false, [underscore]);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe('second build');
  });

  it('a finished build drops the highlight request, whose words belonged to the log it replaced', async () => {
    await showRawLogFor(underscore);
    expect(app.rawLogFocus).not.toBeNull();
    finished(2, false, [underscore]);
    expect(app.rawLogFocus).toBeNull();
  });

  it('a clean build leaves the raw view and does not re-read a log nobody is looking at', async () => {
    finished(1, false, [underscore]);
    await toggleRawLog();
    expect(calls.logReads).toBe(1);
    finished(2, true, []);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.showRawLog).toBe(false);
    expect(app.drawerOpen).toBe(false);
    expect(calls.logReads).toBe(1);
  });

  it('a failed build with the raw view closed does not read the log either — it is never the default', async () => {
    finished(1, false, [underscore]);
    await vi.advanceTimersByTimeAsync(0);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(false);
    expect(calls.logReads).toBe(0);
  });

  it('opening another project forgets the previous one\'s raw log', async () => {
    logOnDisk = 'old project';
    await toggleRawLog();
    expect(app.rawLog).toBe('old project');
    await openFolder('/other');
    expect(app.showRawLog).toBe(false);
    expect(app.rawLog).toBe('');
    expect(app.rawLogFocus).toBeNull();
  });

  it('setDrawerFilter changes one part and keeps the rest', () => {
    setDrawerFilter({ severity: 'errors' });
    expect(app.drawerFilter).toEqual({ severity: 'errors', activeFileOnly: false });
    setDrawerFilter({ activeFileOnly: true });
    expect(app.drawerFilter).toEqual({ severity: 'errors', activeFileOnly: true });
  });

  it('the filter survives a new build — an author chasing errors is still chasing them', () => {
    setDrawerFilter({ severity: 'errors' });
    finished(1, false, [underscore]);
    expect(app.drawerFilter.severity).toBe('errors');
  });

  it('toggleDrawer flips the drawer, and both it and the raw log are palette commands', () => {
    expect(app.drawerOpen).toBe(false);
    toggleDrawer();
    expect(app.drawerOpen).toBe(true);
    toggleDrawer();
    expect(app.drawerOpen).toBe(false);

    const ids = allCommands().map((c) => c.id);
    expect(ids).toContain('toggle-drawer');
    expect(ids).toContain('show-raw-log');
  });

  it('the "Show raw log" command reaches the log with the drawer closed, in one action', async () => {
    logOnDisk = 'the log';
    allCommands().find((c) => c.id === 'show-raw-log')!.run();
    await vi.advanceTimersByTimeAsync(0);
    expect(app.drawerOpen).toBe(true);
    expect(app.showRawLog).toBe(true);
    expect(app.rawLog).toBe('the log');
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

describe('hover, go-to-definition, document symbols (S3.3c)', () => {
  it('lspHover asks the server and returns its answer', async () => {
    lspRequestAnswer = { contents: 'undefined command' };
    const hover = await lspHover('main.tex', 0, 1);
    expect(hover).toEqual({ contents: 'undefined command' });
  });

  it('lspHover returns null with no project open', async () => {
    // `openFolder`/`start` in `beforeEach` already opened /proj; simulate "no project" the way
    // the rest of this file does not need to for its other tests, by clearing it directly.
    app.project = null;
    expect(await lspHover('main.tex', 0, 0)).toBeNull();
  });

  it('lspHover returns null when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    expect(await lspHover('main.tex', 0, 0)).toBeNull();
  });

  it('lspGoToDefinition moves the cursor within the same file', async () => {
    lspRequestAnswer = {
      uri: 'file:///proj/main.tex',
      range: { start: { line: 3, character: 0 }, end: { line: 3, character: 5 } },
    };
    const found = await lspGoToDefinition('main.tex', 0, 0);

    expect(found).toBe(true);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(4); // LSP's 0-based line 3 -> CodeMirror's 1-based line 4
  });

  it('lspGoToDefinition opens the target file when the definition is elsewhere', async () => {
    disk.set('sections/results.tex', 'Results.\nSee \\label{fig:one}\n');
    lspRequestAnswer = {
      uri: 'file:///proj/sections/results.tex',
      range: { start: { line: 1, character: 4 }, end: { line: 1, character: 18 } },
    };
    const found = await lspGoToDefinition('main.tex', 0, 0);

    expect(found).toBe(true);
    expect(app.activePath).toBe('sections/results.tex');
    expect(app.jumpRequest?.line).toBe(2);
  });

  it('lspGoToDefinition takes the first of several locations', async () => {
    lspRequestAnswer = [
      { uri: 'file:///proj/main.tex', range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } } },
      { uri: 'file:///proj/main.tex', range: { start: { line: 9, character: 0 }, end: { line: 9, character: 1 } } },
    ];
    await lspGoToDefinition('main.tex', 0, 0);
    expect(app.jumpRequest?.line).toBe(1);
  });

  it('lspGoToDefinition returns false and does nothing for a null answer', async () => {
    lspRequestAnswer = null;
    const before = app.activePath;
    expect(await lspGoToDefinition('main.tex', 0, 0)).toBe(false);
    expect(app.activePath).toBe(before);
  });

  it('lspGoToDefinition returns false when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    lspRequestAnswer = {
      uri: 'file:///proj/main.tex',
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
    };
    expect(await lspGoToDefinition('main.tex', 0, 0)).toBe(false);
  });

  it('lspDocumentSymbols flattens the server\'s tree', async () => {
    lspRequestAnswer = [
      {
        name: 'Introduction',
        kind: 1,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
        selectionRange: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      },
    ];
    const symbols = await lspDocumentSymbols('main.tex');
    expect(symbols).toEqual([{ name: 'Introduction', detail: null, kind: 1, line: 1, depth: 0 }]);
  });

  it('lspDocumentSymbols returns an empty list when the server is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    expect(await lspDocumentSymbols('main.tex')).toEqual([]);
  });
});

describe('publishDiagnostics reaching the gutter (S3.3b)', () => {
  /** One wire diagnostic, 0-based as the server sends it. */
  function wire(line: number, severity: 1 | 2 | 3 | 4 | undefined, message = 'Undefined reference.') {
    return {
      range: { start: { line, character: 0 }, end: { line, character: 6 } },
      severity,
      message,
      source: 'texlab',
    };
  }

  /** Push a notification through the same path a real event takes. */
  function publish(uri: string, diagnostics: unknown[]): void {
    lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: { uri, diagnostics } });
  }

  it('puts a diagnostic on the right 1-based line of the file it names', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);

    const rows = lspDiagnosticsFor('main.tex');
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ startLine: 4, severity: 'error', title: 'Undefined reference.' });
    expect(app.lspDiagnosticsVersion).toBeGreaterThan(0);
  });

  /** The case that rots silently: the server saying "this file is clean now". */
  it('a second publish with an empty array leaves the file with nothing', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
    const versionAfterFirst = app.lspDiagnosticsVersion;

    publish('file:///proj/main.tex', []);
    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
    // The gutter only redraws if the counter moved, so an empty publish must still bump it.
    expect(app.lspDiagnosticsVersion).toBeGreaterThan(versionAfterFirst);
  });

  it('a publish for a file with no open tab is kept and changes nothing about the active one', () => {
    publish('file:///proj/main.tex', [wire(1, 2)]);
    publish('file:///proj/sections/results.tex', [wire(9, 1)]);

    expect(app.openTabs).toEqual(['main.tex']);
    expect(lspDiagnosticsFor('sections/results.tex').map((r) => r.startLine)).toEqual([10]);
    expect(lspDiagnosticsFor('main.tex').map((r) => r.startLine)).toEqual([2]);
  });

  it('drops a malformed payload without throwing', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    const before = app.lspDiagnosticsVersion;

    expect(() => {
      lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: { diagnostics: [] } });
      lspHandler({
        kind: 'notification',
        method: 'textDocument/publishDiagnostics',
        params: { uri: 'file:///proj/main.tex', diagnostics: 'not an array' },
      });
      lspHandler({ kind: 'notification', method: 'textDocument/publishDiagnostics', params: null });
    }).not.toThrow();

    // Nothing was stored and nothing was withdrawn: the good publish still stands.
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
    expect(app.lspDiagnosticsVersion).toBe(before);
  });

  it('ignores a notification for any other method', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    const before = app.lspDiagnosticsVersion;

    lspHandler({ kind: 'notification', method: '$/progress', params: { token: 1, value: {} } });
    expect(app.lspDiagnosticsVersion).toBe(before);
    expect(lspDiagnosticsFor('main.tex')).toHaveLength(1);
  });

  it('a stopped server takes its diagnostics with it', () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    lspHandler({ kind: 'stopped', message: 'TexLab crashed 5 times' });

    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });

  it('a restart empties the store before the resync republishes', async () => {
    publish('file:///proj/main.tex', [wire(3, 1)]);
    lspHandler({ kind: 'restarted', restarts: 1 });
    await vi.advanceTimersByTimeAsync(0);

    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });

  /** The design decision this loop records, asserted rather than assumed: LSP diagnostics reach
   * the gutter and nothing else. Two cadences in one counter makes the status bar flicker per
   * keystroke, and the drawer's contract is an explained sentence, not a raw server string. */
  it('never touches the drawer, the compile diagnostics, or the two counts', () => {
    app.compile = { ...app.compile, diagnostics: [] };
    const drawerWasOpen = app.drawerOpen;

    publish('file:///proj/main.tex', [wire(3, 1), wire(8, 2)]);

    expect(app.compile.diagnostics).toEqual([]);
    expect(app.errorCount).toBe(0);
    expect(app.warningCount).toBe(0);
    expect(app.drawerOpen).toBe(drawerWasOpen);
  });

  it('answers with nothing for a path when no project is open', () => {
    app.project = null;
    expect(lspDiagnosticsFor('main.tex')).toEqual([]);
  });
});

describe('syncTexForward (S3.4)', () => {
  it('turns a hit into a scroll request the PDF pane can react to', async () => {
    synctexForwardAnswer = { page: 2, x: 100, y: 200 };
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toMatchObject({ page: 2, x: 100, y: 200 });
  });

  it('bumps the nonce on every call so a repeat request to the same spot still fires', async () => {
    await syncTexForward('main.tex', 7);
    const first = app.syncTexScrollRequest!.nonce;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest!.nonce).not.toBe(first);
  });

  it('is silent when there is nothing typeset for the line yet — not a notice', async () => {
    synctexForwardAnswer = new Error('Nothing typeset for main.tex:7 in the last build.');
    app.syncTexScrollRequest = null;
    app.notice = null;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toBeNull();
    expect(app.notice).toBeNull();
  });

  it('does nothing with no project open', async () => {
    app.project = null;
    await syncTexForward('main.tex', 7);
    expect(app.syncTexScrollRequest).toBeNull();
  });
});

describe('syncTexInverse (S3.5)', () => {
  it('moves the cursor in the already-open tab the click resolved to', async () => {
    synctexInverseAnswer = { file: 'main.tex', line: 5 };
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe('main.tex');
    expect(app.jumpRequest?.line).toBe(5);
  });

  it('opens the file first when the click resolves to a tab that is not open yet', async () => {
    disk.set('sections/intro.tex', 'Intro');
    synctexInverseAnswer = { file: 'sections/intro.tex', line: 1 };
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe('sections/intro.tex');
  });

  it('does nothing when the click resolves outside the project', async () => {
    synctexInverseAnswer = { file: null, line: 0 };
    const before = app.activePath;
    await syncTexInverse(1, 10, 20);
    expect(app.activePath).toBe(before);
  });

  it('reports a notice when the backend has nothing on that page', async () => {
    synctexInverseAnswer = new Error('Nothing on page 9 of the last build near that point.');
    await syncTexInverse(9, 0, 0);
    expect(app.notice).toContain('Nothing on page 9');
  });
});

describe('the Document map (S4.2)', () => {
  it('scans the active buffer', async () => {
    type('\\section{Intro}'); // appended after the beforeEach's 'hello', all on line 1
    await refreshOutline();
    expect(app.outline).toEqual([{ kind: 'section', title: 'Intro', line: 1, level: 2 }]);
  });

  it('refills for the newly active tab on switch, including a tab that was already open', async () => {
    // `openFile` fires its outline refresh without awaiting it — the same fire-and-forget shape
    // as `tellServer` — so a settled promise only means the *tab switch* is done; a further
    // microtask flush is what "and then it merged the server's answer in" needs in a test.
    disk.set('sections/results.tex', '\\section{Results}\n');
    await openFile('sections/results.tex');
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([{ kind: 'section', title: 'Results', line: 1, level: 2 }]);

    await openFile('main.tex');
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([]); // main.tex is still just 'hello' — no sectioning commands

    await openFile('sections/results.tex'); // switching back, not re-reading from disk
    await vi.advanceTimersByTimeAsync(0);
    expect(app.outline).toEqual([{ kind: 'section', title: 'Results', line: 1, level: 2 }]);
  });

  it("merges the server's document symbols in once it is ready, preferring them over the scan", async () => {
    type('\\section{Local}'); // the scan would find this on its own
    lspRequestAnswer = [
      {
        name: 'Server Intro',
        kind: 1,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
        selectionRange: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      },
    ];
    await refreshOutline();
    // The server's answer describes line 1 too (0-based 0 -> 1-based 1), so it wins there —
    // the scanned 'Local' section is dropped rather than shown twice.
    expect(app.outline).toEqual([{ kind: 'section', title: 'Server Intro', line: 1, level: 0 }]);
  });

  it('scans without the server when it is not ready', async () => {
    lspHandler({ kind: 'stopped', message: 'gone' });
    type('\\section{Offline}');
    await refreshOutline();
    expect(app.outline).toEqual([{ kind: 'section', title: 'Offline', line: 1, level: 2 }]);
  });

  it('clears when the project closes', async () => {
    type('\\section{Intro}');
    await refreshOutline();
    expect(app.outline.length).toBe(1);

    await openFolder('/proj'); // closeAllDocuments runs before the new project is opened
    expect(app.outline).toEqual([]);
  });

  it("goToOutlineItem moves the cursor to the item's line", () => {
    goToOutlineItem({ kind: 'section', title: 'X', line: 42, level: 0 });
    expect(app.jumpRequest?.line).toBe(42);
  });

  it('debounces a buffer change into one rescan ~250 ms after typing stops', async () => {
    await refreshOutline();
    expect(app.outline).toEqual([]); // 'hello' has none of the five kinds

    type('\\section{New}');
    // Nothing yet: onTextChange must not reach the scan on the keystroke path itself
    // (DESIGN.md §2's <16 ms budget is the reason this is debounced at all).
    expect(app.outline).toEqual([]);

    await vi.advanceTimersByTimeAsync(249);
    expect(app.outline).toEqual([]);

    await vi.advanceTimersByTimeAsync(1);
    expect(app.outline).toEqual([{ kind: 'section', title: 'New', line: 1, level: 2 }]);
  });
});

describe('the bibliography index (S7.2)', () => {
  const smith = {
    key: 'smith2019',
    entryType: 'article',
    author: 'Smith, Jane',
    year: '2019',
    title: 'A Title',
    file: 'refs.bib',
    span: { start: 0, end: 80 },
  };

  it('is fetched once when a folder opens, and cleared first', async () => {
    bibliographyOnDisk = {
      files: [{ path: 'refs.bib', exists: true, entryCount: 1, problems: [], origin: { kind: 'named', file: 'main.tex', line: 9 } }],
      entries: [smith],
      citations: [{ key: 'smith2019', file: 'main.tex', line: 3 }],
      hasNociteStar: false,
    };
    await openFolder('/proj');
    expect(bibliography.entries.get('smith2019')).toEqual(smith);
    expect(bibliography.citations.get('smith2019')).toEqual([{ key: 'smith2019', file: 'main.tex', line: 3 }]);
    expect(bibliography.missingFiles).toEqual([]);
  });

  it('replaces its snapshot whole when Rust says the index changed', async () => {
    expect(bibliography.entries.size).toBe(0);
    bibliographyHandler({
      files: [
        { path: 'refs.bib', exists: true, entryCount: 1, problems: [], origin: { kind: 'named', file: 'main.tex', line: 9 } },
        { path: 'missing.bib', exists: false, entryCount: 0, problems: [], origin: { kind: 'linked' } },
      ],
      entries: [smith],
      citations: [],
      hasNociteStar: false,
    });
    expect(bibliography.entries.get('smith2019')).toEqual(smith);
    expect(bibliography.missingFiles).toEqual(['missing.bib']);
  });
});
