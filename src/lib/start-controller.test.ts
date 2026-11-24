// S17.4: what the start screen does, with a fake Rust that records every call.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProjectInfo, RecentDocument } from './ipc';

/** Every call made to the fake, as `[name, ...arguments]`. */
const ipcCalls: unknown[][] = [];
/** Every folder `openProject` was asked for. */
const opened: string[] = [];
/** What a call answers with. A test replaces the ones it is about. */
let answers: Record<string, (...args: any[]) => unknown> = {};

const project: ProjectInfo = {
  rootDir: '/proj', title: 'proj', rootFile: 'main.tex', buildDir: '/proj/.abstract-tex/build', tree: [],
  documentFiles: ['main.tex'], documentFilesComplete: true, engineNotice: null, watchNotice: null,
};

vi.mock('./ipc', () => ({
  // A `Proxy` answers for any name, so this fake does not have to list every command there is.
  ipc: new Proxy({}, {
    get: (_target, name: string) => (...args: unknown[]) => {
      ipcCalls.push([name, ...args]);
      if (name === 'openProject') opened.push(args[0] as string);
      if (name.startsWith('on')) return Promise.resolve(() => {});
      const answer = answers[name];
      if (name === 'assetUrl') return String(args[0]);
      return new Promise((resolve) => resolve(answer ? answer(...args) : null));
    },
  }),
}));

const { app } = await import('./state.svelte');
const { startScreen } = await import('./start.svelte');
const {
  backToDocument, clearRecents, findRecent, forgetRecent, goToPage, loadRecents, openRecent, runFromPalette,
  showRecentInFolder, showStart, startBack,
} = await import('./start-controller.svelte');

function only(...names: string[]): unknown[][] {
  return ipcCalls.filter((call) => names.includes(call[0] as string));
}
function recent(folder: string, title = folder.slice(1)): RecentDocument {
  return { folder, rootFile: 'main.tex', title, place: [], usedAt: 1 };
}

let listed: RecentDocument[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  listed = [recent('/a'), recent('/b')];
  answers = {
    recentDocuments: () => listed,
    recentIsHere: () => true,
    openProject: (path: string) => ({ ...project, rootDir: path }),
    readFile: () => 'hello',
    documentTitle: () => 'proj',
    bibliographyHealth: () => [],
    compile: () => 1,
    templateList: () => [],
    forgetRecent: (folder: string) => {
      listed = listed.filter((r) => r.folder !== folder);
    },
    clearRecents: () => {
      listed = [];
    },
  };
  app.project = null;
  app.launching = false;
  app.focusModeEnabled = false;
  app.opening = null;
  app.conflict = null;
  app.notice = null;
  startScreen.visible = true;
  startScreen.page = 'home';
  startScreen.recents = null;
  startScreen.presence = new Map();
  startScreen.carryOn = null;
  startScreen.message = null;
  ipcCalls.length = 0;
  opened.length = 0;
});
afterEach(() => vi.useRealTimers());

describe('loading the recent documents', () => {
  it('decides Carry on after 150 ms at most, and never changes it', async () => {
    let answerA!: (here: boolean) => void;
    answers.recentIsHere = (folder: string) => (folder === '/a' ? new Promise((r) => (answerA = r)) : true);
    const loading = loadRecents();
    await vi.advanceTimersByTimeAsync(149);
    expect([startScreen.carryOn, startScreen.decided]).toEqual([null, false]);
    await vi.advanceTimersByTimeAsync(1);
    await loading;
    expect([startScreen.carryOn, startScreen.decided]).toEqual(['/a', true]);
    answerA(false);
    await vi.advanceTimersByTimeAsync(0);
    expect([startScreen.carryOn, startScreen.presence.get('/a')]).toEqual(['/a', 'missing']);
  });

  it('decides at once when every check has answered, skipping a missing one', async () => {
    answers.recentIsHere = (folder: string) => folder !== '/a';
    const loading = loadRecents();
    await vi.advanceTimersByTimeAsync(1);
    await loading;
    expect(startScreen.carryOn).toBe('/b');
  });

  it('shows the rows before any check has answered', async () => {
    answers.recentIsHere = () => new Promise(() => {});
    void loadRecents();
    await vi.advanceTimersByTimeAsync(0);
    expect([startScreen.recents?.length, startScreen.presence.size]).toEqual([2, 0]);
  });

  it('is an empty list when it cannot be read', async () => {
    answers.recentDocuments = () => {
      throw new Error('no');
    };
    await loadRecents();
    expect([startScreen.recents, startScreen.decided]).toEqual([[], true]);
  });

  it('a check of an older load that answers late changes nothing', async () => {
    let late!: (here: boolean) => void;
    answers.recentIsHere = (folder: string) => (folder === '/a' ? new Promise((r) => (late = r)) : true);
    void loadRecents();
    await vi.advanceTimersByTimeAsync(0);
    const answerOlder = late;
    answers.recentIsHere = () => true;
    void loadRecents();
    await vi.advanceTimersByTimeAsync(0);
    answerOlder(false);
    await vi.advanceTimersByTimeAsync(0);
    expect(startScreen.presence.get('/a')).toBe('here');
  });

  it('a check that fails says missing, and an older load changes nothing', async () => {
    let slowAnswer!: (list: RecentDocument[]) => void;
    answers.recentDocuments = () => new Promise((r) => (slowAnswer = r));
    const older = loadRecents();
    answers.recentDocuments = () => listed;
    answers.recentIsHere = () => {
      throw new Error('gone');
    };
    const newer = loadRecents();
    await vi.advanceTimersByTimeAsync(200);
    await newer;
    slowAnswer([recent('/old')]);
    await older;
    expect(startScreen.recents?.map((r) => r.folder)).toEqual(['/a', '/b']);
    expect([startScreen.presence.get('/a'), startScreen.carryOn]).toEqual(['missing', null]);
  });
});

describe('acting on a recent document', () => {
  beforeEach(async () => {
    const loading = loadRecents();
    await vi.advanceTimersByTimeAsync(200);
    await loading;
    ipcCalls.length = 0;
  });

  it('opens it under its title and the start screen goes', async () => {
    startScreen.recents = [recent('/a', 'On Bees')];
    let answer!: (info: ProjectInfo) => void;
    answers.openProject = () => new Promise((r) => (answer = r));
    const opening = openRecent('/a');
    expect(app.opening).toBe('On Bees');
    await vi.advanceTimersByTimeAsync(0);
    answer({ ...project, rootDir: '/a' });
    await opening;
    expect([opened, startScreen.visible, app.project?.rootDir]).toEqual([['/a'], false, '/a']);
  });

  it('a refused one keeps the start screen, with the sentence', async () => {
    answers.openProject = () => {
      throw '/a is not a folder';
    };
    await openRecent('/a');
    expect([startScreen.visible, startScreen.message]).toEqual([true, { kind: 'refusal', text: '/a is not a folder' }]);
  });

  it('a missing document found again is corrected and opened', async () => {
    answers.pickFolder = () => '/usb/thesis';
    await findRecent('/a');
    expect(only('pickFolder', 'openProject', 'recentMoved')).toEqual([
      ['pickFolder'], ['openProject', '/usb/thesis'], ['recentMoved', '/a', '/usb/thesis'],
    ]);
  });

  it('a wrong folder picked for it is not recorded as where it went', async () => {
    answers.pickFolder = () => '/usb/thesis';
    answers.openProject = () => {
      throw 'no';
    };
    await findRecent('/a');
    expect(only('recentMoved')).toEqual([]);
  });

  it('a cancelled picker changes nothing', async () => {
    await findRecent('/a');
    expect(only('openProject', 'recentMoved')).toEqual([]);
  });

  it('taking one off the list changes the list and nothing else', async () => {
    await forgetRecent('/a');
    expect(only('forgetRecent', 'recentDocuments', 'openProject')).toEqual([['forgetRecent', '/a'], ['recentDocuments']]);
    expect(startScreen.recents?.map((r) => r.folder)).toEqual(['/b']);
  });

  it('clearing empties the list', async () => {
    await clearRecents();
    expect(startScreen.recents).toEqual([]);
  });

  it('a folder that cannot be shown says so here', async () => {
    answers.showRecentInFolder = () => {
      throw 'no file manager';
    };
    await showRecentInFolder('/a');
    expect(startScreen.message).toEqual({ kind: 'refusal', text: 'no file manager' });
  });
});

it('going to a page clears what the last one said', () => {
  startScreen.message = { kind: 'refusal', text: 'x' };
  const before = startScreen.focusNonce;
  goToPage('open', 'first');
  expect([startScreen.page, startScreen.focusTarget, startScreen.message, startScreen.focusNonce]).toEqual([
    'open', 'first', null, before + 1,
  ]);
});

describe('Home over an open document (S17.5)', () => {
  beforeEach(() => {
    app.project = project;
  });

  it('opens on its page, with the document behind it', () => {
    startScreen.visible = false;
    showStart('home', false);
    expect([startScreen.visible, startScreen.page, app.project]).toEqual([true, 'home', project]);
  });

  it('does not open while a dialog is up', () => {
    startScreen.visible = false;
    showStart('home', true);
    expect(startScreen.visible).toBe(false);
  });

  it('does not open while a document is being opened, or before the app knows what to show', () => {
    startScreen.visible = false;
    app.opening = 'x';
    showStart('home', false);
    app.opening = null;
    app.launching = true;
    showStart('home', false);
    expect(startScreen.visible).toBe(false);
  });

  it('Esc on any page goes back to the document', () => {
    startScreen.page = 'open';
    startBack();
    expect(startScreen.visible).toBe(false);
  });

  it('with no document, Esc goes to Home and stays there', () => {
    app.project = null;
    startScreen.page = 'open';
    startBack();
    startBack();
    expect([startScreen.visible, startScreen.page]).toEqual([true, 'home']);
  });

  it('Esc while a document is being opened leaves the screen where it was', () => {
    // `OpeningNotice` hears it as Stop.
    startScreen.page = 'open';
    app.opening = 'x';
    startBack();
    expect([startScreen.visible, startScreen.page]).toEqual([true, 'open']);
    app.project = null;
    startBack();
    expect(startScreen.page).toBe('open');
  });

  it('going back gives the editor the focus, and forgets what was said here', () => {
    const before = app.editorFocusRequest;
    startScreen.message = { kind: 'refusal', text: 'x' };
    backToDocument();
    expect([app.editorFocusRequest, startScreen.message]).toEqual([before + 1, null]);
  });

  it('there is no document to go back to when none is open', () => {
    app.project = null;
    backToDocument();
    expect(startScreen.visible).toBe(true);
  });

  it('the document that is open now is gone back to, not opened again', async () => {
    await openRecent('/proj');
    expect([startScreen.visible, opened]).toEqual([false, []]);
  });

  it('a refused opening does not send the focus to the editor behind', async () => {
    const before = app.editorFocusRequest;
    answers.openProject = () => {
      throw 'no';
    };
    await openRecent('/a');
    expect([app.editorFocusRequest, startScreen.visible]).toEqual([before, true]);
  });

  it('a palette action about the document goes back to it first', () => {
    runFromPalette('toggle-focus-mode');
    expect([startScreen.visible, app.focusModeEnabled]).toEqual([false, true]);
  });

  it('a palette action that needs no document runs here', () => {
    const before = app.editorFocusRequest;
    runFromPalette('open-folder');
    expect([startScreen.visible, startScreen.page, app.editorFocusRequest]).toEqual([true, 'open', before]);
  });

  it('Home is in the palette', () => {
    startScreen.visible = false;
    runFromPalette('home');
    expect([startScreen.visible, startScreen.page]).toEqual([true, 'home']);
  });
});
