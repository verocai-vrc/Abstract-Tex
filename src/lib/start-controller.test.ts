// S17.4: what the start screen does, with a fake Rust that records every call.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Preferences, ProjectInfo, RecentDocument, TemplateInfo } from './ipc';

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
const { newProject } = await import('./templates.svelte');
const { allCommands } = await import('./commands');
const {
  backToDocument, changePlace, chooseTemplate, clearRecents, createProject, findRecent, forgetRecent, goToPage, loadRecents,
  moveTemplateSelection, openRecent, runFromPalette, showRecentInFolder, showStart, startBack, startFromTemplate,
  templateFilterChanged,
} = await import('./start-controller.svelte');

function only(...names: string[]): unknown[][] {
  return ipcCalls.filter((call) => names.includes(call[0] as string));
}
function recent(folder: string, title = folder.slice(1)): RecentDocument {
  return { folder, rootFile: 'main.tex', title, place: [], usedAt: 1 };
}

let listed: RecentDocument[] = [];

const essay: TemplateInfo = {
  id: 'essay', name: 'Essay', category: 'essay', description: 'A short essay', previewUrl: 'data:,',
  fields: [{ id: 'title', label: 'Title', example: '' }, { id: 'author', label: 'Author', example: '' }],
};
const cv: TemplateInfo = { ...essay, id: 'cv', name: 'Curriculum vitae', category: 'cv', description: 'One page' };
const noPreferences: Preferences = {
  place: { path: '/home/ana/Documents/Abstract-Tex', parts: ['Documents', 'Abstract-Tex'] },
  author: '', textSize: 100, colours: 'match',
};
let preferences = noPreferences;

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
    templateList: () => [essay, cv],
    templateCreate: () => '/proj',
    preferences: () => preferences,
    setPreferences: (change: { place?: string; author?: string }) => {
      const { place, ...rest } = change;
      preferences = { ...preferences, ...rest, ...(place ? { place: { path: place, parts: place.split('/').filter(Boolean) } } : {}) };
      return preferences;
    },
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
  startScreen.preferences = null;
  preferences = noPreferences;
  newProject.catalog = null;
  newProject.values = {};
  newProject.error = null;
  newProject.loadError = null;
  newProject.creating = false;
  newProject.step = 'choose';
  newProject.selectedId = null;
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

describe('a new document (S11.11, S17.6)', () => {
  async function showNew() {
    goToPage('new', 'first');
    await vi.advanceTimersByTimeAsync(0);
  }
  async function fillIn(values: Record<string, string>, id = 'essay') {
    await showNew();
    chooseTemplate(id);
    newProject.values = values;
  }

  it('opens on the cards with the first one highlighted, and asks Rust for the catalog once', async () => {
    await showNew();
    expect([newProject.step, newProject.catalog, newProject.selectedId]).toEqual(['choose', [essay, cv], 'essay']);
    await showNew();
    expect(only('templateList')).toHaveLength(1);
  });

  it('moves the highlight along what the filter leaves, and re-aims it when the filter hides it', async () => {
    await showNew();
    moveTemplateSelection(1);
    expect(newProject.selectedId).toBe('cv');
    newProject.query = 'essay';
    templateFilterChanged();
    expect(newProject.selectedId).toBe('essay');
  });

  it('Esc steps back once: the questions, the cards, the document', async () => {
    app.project = project;
    await fillIn({ title: 'T' });
    const before = startScreen.focusNonce;
    startBack();
    expect([startScreen.visible, newProject.step, startScreen.focusNonce]).toEqual([true, 'choose', before + 1]);
    startBack();
    expect(startScreen.visible).toBe(false);
    expect(only('templateCreate')).toEqual([]);
  });

  it('creates in the remembered place without asking, and opens the result', async () => {
    await fillIn({ title: '  My thesis ', author: 'Ada' });
    await createProject();
    expect(only('templateCreate')).toEqual([
      ['templateCreate', 'essay', '/home/ana/Documents/Abstract-Tex', { title: 'My thesis', author: 'Ada' }],
    ]);
    expect(only('pickParentFolder')).toEqual([]);
    expect([startScreen.visible, app.project?.rootDir]).toEqual([false, '/proj']);
  });

  it('opens at the first thing to fill in', async () => {
    answers.readFile = () => 'x\n% FILL IN: your text\n';
    const tree = [{ name: 'main.tex', path: 'main.tex', isDir: false, children: [] }];
    answers.openProject = () => ({ ...project, tree });
    await fillIn({ title: 'T' });
    await createProject();
    expect(app.jumpRequest?.line).toBe(2);
  });

  it('reads the place first when Create is pressed before it is known', async () => {
    await fillIn({ title: 'T' });
    startScreen.preferences = null;
    await createProject();
    expect(only('templateCreate')[0]?.[2]).toBe('/home/ana/Documents/Abstract-Tex');
  });

  it('sends an empty author as an empty string, because the template declares the field', async () => {
    await fillIn({ title: 'T' });
    await createProject();
    expect(only('templateCreate')[0]?.[3]).toEqual({ title: 'T', author: '' });
    expect(only('setPreferences')).toEqual([]);
  });

  it('wants a title, and writes nothing without one', async () => {
    await fillIn({ title: '   ' });
    await createProject();
    expect(newProject.error).toContain('title');
    expect(only('templateCreate')).toEqual([]);
  });

  it('a refusal keeps the page and everything typed', async () => {
    answers.templateCreate = () => {
      throw 'There is already something called "my-thesis" in /papers. Change the title, or choose another place.';
    };
    await fillIn({ title: 'My thesis', author: 'Ada' });
    await createProject();
    expect(newProject.error).toContain('already something called');
    expect([startScreen.page, newProject.step, newProject.values.title]).toEqual(['new', 'fill', 'My thesis']);
    expect(newProject.creating).toBe(false);
    expect(only('setPreferences')).toEqual([]);
  });

  it('creates nothing while the open document cannot be left, and says why here (R6.9)', async () => {
    app.project = project;
    app.conflict = { path: 'main.tex' } as typeof app.conflict;
    await fillIn({ title: 'T' });
    await createProject();
    expect(only('templateCreate')).toEqual([]);
    expect(newProject.error).toContain('main.tex');
  });

  it('is not made twice, and cannot be stepped back from, while it is being made', async () => {
    let made!: (folder: string) => void;
    answers.templateCreate = () => new Promise((resolve) => (made = resolve));
    await fillIn({ title: 'T' });
    const creating = createProject();
    await vi.advanceTimersByTimeAsync(0);
    await createProject();
    startBack();
    expect([newProject.step, startScreen.page]).toEqual(['fill', 'new']);
    goToPage('new', 'heading');
    expect(newProject.step).toBe('fill');
    made('/proj');
    await creating;
    expect(only('templateCreate')).toHaveLength(1);
  });

  it('is not made while another document is being opened', async () => {
    await fillIn({ title: 'T' });
    app.opening = 'x';
    await createProject();
    expect(only('templateCreate')).toEqual([]);
  });

  it('remembers the author once a document is made, and offers it next time with an empty title', async () => {
    await fillIn({ title: 'T', author: 'Ada' });
    await createProject();
    expect(ipcCalls).toContainEqual(['setPreferences', { author: 'Ada' }]);
    await showNew();
    chooseTemplate('essay');
    expect(newProject.values).toEqual({ author: 'Ada' });
  });

  it('does not put back a name someone has just emptied', async () => {
    preferences = { ...noPreferences, author: 'Ada' };
    await fillIn({ title: 'T', author: '' });
    startBack();
    chooseTemplate('cv');
    expect(newProject.values).toEqual({ title: 'T', author: '' });
  });

  it('one that was made but could not be opened says so on the cards', async () => {
    answers.openProject = () => {
      throw 'no';
    };
    await fillIn({ title: 'T' });
    await createProject();
    expect([startScreen.message?.text, newProject.step, newProject.values.title]).toEqual(['no', 'choose', undefined]);
  });

  it('Change… remembers the place', async () => {
    await showNew();
    answers.pickParentFolder = () => '/usb';
    await changePlace();
    expect(ipcCalls).toContainEqual(['setPreferences', { place: '/usb' }]);
    expect(startScreen.preferences?.place.path).toBe('/usb');
  });

  it('Change… cancelled, or refused, keeps the place', async () => {
    await showNew();
    await changePlace();
    expect(only('setPreferences')).toEqual([]);
    answers.pickParentFolder = () => '/usb';
    answers.setPreferences = () => {
      throw 'the disk is full';
    };
    await changePlace();
    expect([startScreen.preferences?.place, newProject.error]).toEqual([noPreferences.place, 'the disk is full']);
  });

  it('a template on Home goes straight to its questions', async () => {
    await loadRecents();
    startFromTemplate('essay');
    expect([startScreen.page, newProject.step, newProject.selectedId]).toEqual(['new', 'fill', 'essay']);
  });

  it('is in the palette with its chord', () => {
    const command = allCommands().find((c) => c.id === 'new-project');
    expect([command?.title, command?.shortcut, command?.withoutDocument]).toEqual(['New document…', 'Ctrl Shift N', true]);
  });
});
