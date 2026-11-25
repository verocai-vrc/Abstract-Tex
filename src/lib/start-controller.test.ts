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
  documentFiles: ['main.tex'], documentFilesComplete: true, engineNotice: null, watchNotice: null, hasPdf: false,
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
const { startScreen, strayQuestion, UNPACK_REFUSALS } = await import('./start.svelte');
const { words } = await import('./words');
const { newProject } = await import('./templates.svelte');
const { allCommands } = await import('./commands');
const { stopOpening } = await import('./controller.svelte');
const {
  backToDocument, cancelChoice, changePlace, chooseFile, chooseFolder, chooseTemplate, clearRecents, copyAndOpen, createProject, findRecent, forgetRecent, goToPage, loadRecents,
  moveTemplateSelection, openAnyway, openRecent, runFromPalette, showRecentInFolder, showStart, startBack, startFromTemplate,
  templateFilterChanged,
} = await import('./start-controller.svelte');

function only(...names: string[]): unknown[][] {
  return ipcCalls.filter((call) => names.includes(call[0] as string));
}
function recent(folder: string, title = folder.slice(1)): RecentDocument {
  return { folder, rootFile: 'main.tex', title, place: [], usedAt: 1, picture: null };
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
    inspectChoice: (path: string) => ({ kind: 'document', folder: path, file: null }),
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
  startScreen.choice = null;
  startScreen.looking = null;
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
      ['pickFolder', words.findItTitle], ['openProject', '/usb/thesis'], ['recentMoved', '/a', '/usb/thesis'],
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

  it('Help is in the palette, document or not', () => {
    expect(allCommands().find((c) => c.id === 'help')).toMatchObject({ title: 'Help', withoutDocument: true });
    startScreen.visible = false;
    runFromPalette('help');
    expect([startScreen.visible, startScreen.page]).toEqual([true, 'help']);
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
    expect([newProject.creating, startScreen.message]).toEqual([false, null]); // said once
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

  it('can be stopped like any opening, and keeps everything typed', async () => {
    let refuse!: (error: string) => void;
    answers.templateCreate = () => new Promise((_resolve, reject) => (refuse = reject));
    answers.stopOpening = () => {
      refuse('Opening was stopped.'); // as Rust answers the command that was stopped
      return true;
    };
    await fillIn({ title: 'My thesis' });
    const creating = createProject();
    await vi.advanceTimersByTimeAsync(0);
    expect(app.opening).toBe('My thesis');
    const before = startScreen.focusNonce;
    await stopOpening();
    await creating;
    expect([app.opening, newProject.creating, newProject.error, startScreen.message]).toEqual([null, false, null, null]);
    expect([startScreen.page, newProject.step, newProject.values.title]).toEqual(['new', 'fill', 'My thesis']);
    expect(startScreen.focusNonce).toBe(before + 1); // the Stop button that had it is gone
    expect(only('openProject')).toEqual([]);
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
    const before = startScreen.focusNonce;
    await createProject();
    expect([startScreen.message?.text, newProject.step, newProject.values.title]).toEqual(['no', 'choose', undefined]);
    expect(startScreen.focusNonce).toBe(before + 1); // Create, which had the focus, is gone
  });

  it('Change… to a folder that is itself a document says so on the page', async () => {
    await fillIn({ title: 'T' });
    const before = startScreen.preferences;
    answers.pickParentFolder = () => '/thesis';
    answers.setPreferences = () => {
      throw 'placeIsADocument';
    };
    await changePlace();
    expect([newProject.error, startScreen.preferences]).toEqual([words.placeIsADocument, before]);
    answers.setPreferences = () => ({ ...noPreferences, place: { path: '/usb', parts: ['usb'] } });
    await changePlace();
    expect(newProject.error).toBe(null);
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

describe('opening what people already have (S17.8)', () => {
  const tree = ['main.tex', 'bees.tex', 'chapters/intro.tex'].map((path) => ({ name: path, path, isDir: false, children: [] }));
  beforeEach(() => {
    goToPage('open', 'first');
    answers.openProject = (path: string) => ({
      ...project, rootDir: path, tree, documentFiles: ['main.tex', 'chapters/intro.tex'],
    });
  });

  function chosen(choice: object) {
    answers.pickFile = () => '/picked';
    answers.pickFolder = () => '/picked';
    answers.inspectChoice = () => choice;
  }

  it('opens the document a chosen chapter belongs to, with the chapter in front', async () => {
    chosen({ kind: 'document', folder: '/t', file: 'chapters/intro.tex' });
    await chooseFile();
    expect(only('inspectChoice')).toEqual([['inspectChoice', '/picked']]);
    expect([app.project?.rootDir, app.activePath, app.notice]).toEqual(['/t', 'chapters/intro.tex', null]);
    expect([...app.openTabs]).toContain('main.tex');
  });

  it('a .tex that is not the main file is shown beside it, and the main file stays', async () => {
    chosen({ kind: 'document', folder: '/t', file: 'bees.tex' });
    await chooseFile();
    expect(app.activePath).toBe('bees.tex');
    expect(only('setRootFile')).toEqual([]);
    expect(app.notice).toBe(words.pdfShowsAnother('main.tex', 'bees.tex'));
  });

  it('says nothing of the sort when not every part of the document is known', async () => {
    answers.openProject = (path: string) => ({ ...project, rootDir: path, tree, documentFilesComplete: false });
    chosen({ kind: 'document', folder: '/t', file: 'bees.tex' });
    await chooseFile();
    expect(app.notice).toBeNull();
  });

  it('opens a chosen folder', async () => {
    chosen({ kind: 'document', folder: '/picked', file: null });
    await chooseFolder();
    expect(opened).toEqual(['/picked']);
  });

  it('refuses a crowded folder and offers to choose again', async () => {
    chosen({ kind: 'crowdedFolder', place: ['Downloads'] });
    await chooseFolder();
    expect(startScreen.choice).toEqual({ kind: 'crowdedFolder', place: ['Downloads'] });
    expect([opened, startScreen.visible]).toEqual([[], true]);
  });

  it('opens a folder with no .tex only when asked again', async () => {
    chosen({ kind: 'noTex', folder: '/empty' });
    await chooseFolder();
    expect(opened).toEqual([]);
    await openAnyway();
    expect(opened).toEqual(['/empty']);
    startScreen.choice = { kind: 'zip', file: '/a.zip' };
    await openAnyway();
    expect(opened).toEqual(['/empty']);
  });

  describe('a zip (S17.9)', () => {
    const thesis = '/home/ana/Documents/Abstract-Tex/thesis';
    beforeEach(() => {
      answers.pickFile = () => '/d/thesis.zip';
      answers.inspectChoice = () => ({ kind: 'zip', file: '/d/thesis.zip' });
      answers.unpackZip = () => ({ kind: 'unpacked', folder: thesis });
    });

    it.each(UNPACK_REFUSALS)('has a sentence for one refused as %s', (why) =>
      expect(words.zipRefused[why]('x')).toBeTruthy(),
    );

    it('names nothing when two things have the very same name', () =>
      expect(words.zipRefused.sameName('')).not.toMatch(/“”|capital/));

    it('opens what was unpacked', async () => {
      await chooseFile();
      expect(only('unpackZip')).toEqual([['unpackZip', '/d/thesis.zip']]);
      expect(opened).toEqual([thesis]);
      expect(startScreen.visible).toBe(false);
    });

    it('is being opened while it is unpacked', async () => {
      let answer!: (unpacked: object) => void;
      answers.unpackZip = () => new Promise((resolve) => (answer = resolve));
      const choosing = chooseFile();
      await vi.advanceTimersByTimeAsync(0);
      expect(app.opening).toBe('thesis.zip');
      answer({ kind: 'unpacked', folder: thesis });
      await choosing;
      expect([app.opening, opened]).toEqual([null, [thesis]]);
    });

    it('a refusal is said here, and opens nothing', async () => {
      const name = 'x'.repeat(500);
      answers.unpackZip = () => ({ kind: 'refused', why: 'unsafeName', name });
      await chooseFile();
      expect(startScreen.message?.text).toBe(words.zipRefused.unsafeName('x'.repeat(80)));
      expect([opened, app.opening, startScreen.visible, startScreen.page]).toEqual([[], null, true, 'open']);
    });

    it('a stopped unpacking says nothing and opens nothing', async () => {
      let answer!: (unpacked: object) => void;
      answers.unpackZip = () => new Promise((resolve) => (answer = resolve));
      answers.stopOpening = () => answer({ kind: 'stopped' });
      const choosing = chooseFile();
      await vi.advanceTimersByTimeAsync(0);
      await stopOpening();
      await choosing;
      expect([startScreen.message, opened, app.opening]).toEqual([null, [], null]);
    });

    it('nothing is unpacked while the open document cannot be left', async () => {
      app.conflict = { path: 'main.tex', diskText: 'theirs' };
      await chooseFile();
      app.conflict = null;
      expect(only('unpackZip')).toEqual([]);
      expect(startScreen.message?.text).toContain('main.tex');
    });
  });

  describe('a .tex among other things (S17.10)', () => {
    const stray = (others: number) => ({
      kind: 'strayFile' as const, file: '/home/ana/Downloads/paper.tex', place: ['Documents', 'Abstract-Tex'], others,
    });
    beforeEach(() => {
      chosen(stray(3));
      answers.copyStray = () => '/home/ana/Documents/Abstract-Tex/paper';
    });

    it('asks before copying, with the number of files', async () => {
      await chooseFile();
      expect(startScreen.choice).toEqual(stray(3));
      expect([only('copyStray'), opened]).toEqual([[], []]);
      const question = strayQuestion(stray(3));
      expect(question).toContain('paper.tex is in your Downloads folder');
      expect(question).toContain('in Documents › Abstract-Tex');
      expect(question).toContain('with the 3 files it uses?');
      expect(strayQuestion(stray(1))).toContain('with the one file it uses?');
      expect(strayQuestion(stray(0))).toContain('copy paper.tex there?');
    });

    it('names the folder of a Windows path, and the drive it is in when it is in no folder', () => {
      expect(strayQuestion({ ...stray(0), file: 'C:\\Users\\Ana\\Desktop\\a.tex' })).toContain('a.tex is in your Desktop folder');
      expect(strayQuestion({ ...stray(0), file: 'E:\\a.tex' })).toContain('a.tex is in E:');
      expect(strayQuestion({ ...stray(0), file: '/a.tex' })).toContain('a.tex is in /');
    });

    it('Cancel copies nothing', async () => {
      await chooseFile();
      const focused = startScreen.focusNonce;
      cancelChoice();
      expect([startScreen.choice, only('copyStray'), opened]).toEqual([null, [], []]);
      expect([startScreen.focusTarget, startScreen.focusNonce]).toEqual(['first', focused + 1]);
    });

    it('opens the copy, not the original', async () => {
      await chooseFile();
      await copyAndOpen();
      expect(only('copyStray')).toEqual([['copyStray', '/home/ana/Downloads/paper.tex']]);
      expect(opened).toEqual(['/home/ana/Documents/Abstract-Tex/paper']);
    });

    it('puts the chosen file in front when it is not the main one', async () => {
      answers.openProject = (path: string) => ({
        ...project, rootDir: path, documentFiles: ['main.tex', 'paper.tex'],
        tree: ['main.tex', 'paper.tex'].map((name) => ({ name, path: name, isDir: false, children: [] })),
      });
      await chooseFile();
      await copyAndOpen();
      expect(app.activePath).toBe('paper.tex');
    });

    it('a copy that cannot be made is said, and the question stays', async () => {
      answers.copyStray = () => {
        throw 'the disk is full';
      };
      await chooseFile();
      await copyAndOpen();
      expect([startScreen.message?.text, startScreen.choice, opened]).toEqual(['the disk is full', stray(3), []]);
    });

    it('copies nothing that was not asked about', async () => {
      for (const choice of [null, { kind: 'word' }, { kind: 'zip', file: '/a.zip' }] as const) {
        startScreen.choice = choice;
        await copyAndOpen();
      }
      expect(only('copyStray')).toEqual([]);
    });
  });

  it.each(['word', 'pdf', 'otherArchive', 'otherFile', 'gone'] as const)('has a sentence for %s', (kind) =>
    expect(words.refused[kind]).toBeTruthy(),
  );

  it('a cancelled picker changes nothing', async () => {
    chosen({ kind: 'word' });
    await chooseFile();
    answers.pickFile = () => null;
    await chooseFile();
    expect([startScreen.choice, only('inspectChoice').length]).toEqual([{ kind: 'word' }, 1]);
  });

  it('what is being looked at is said, and an older look that answers late changes nothing', async () => {
    let answerOlder!: (choice: object) => void;
    answers.pickFile = () => '/slow/a.tex';
    answers.inspectChoice = () => new Promise((resolve) => (answerOlder = resolve));
    const older = chooseFile();
    await vi.advanceTimersByTimeAsync(0);
    expect(startScreen.looking).toBe('a.tex');
    const answer = answerOlder;
    chosen({ kind: 'word' });
    await chooseFile();
    answer({ kind: 'document', folder: '/t', file: null });
    await older;
    expect([startScreen.choice, startScreen.looking, opened]).toEqual([{ kind: 'word' }, null, []]);
  });

  it('an older look that answers does not take down what is said of the newer', async () => {
    const waiting: Array<(choice: object) => void> = [];
    answers.inspectChoice = () => new Promise((resolve) => waiting.push(resolve));
    answers.pickFile = () => '/a.tex';
    const older = chooseFile();
    await vi.advanceTimersByTimeAsync(0);
    answers.pickFile = () => '/b.tex';
    void chooseFile();
    await vi.advanceTimersByTimeAsync(0);
    waiting[0]!({ kind: 'word' });
    await older;
    expect([startScreen.looking, startScreen.choice]).toEqual(['b.tex', null]);
  });

  it('a look that answers after going away changes nothing', async () => {
    let answer!: (choice: object) => void;
    answers.pickFile = () => '/slow/a.tex';
    answers.inspectChoice = () => new Promise((resolve) => (answer = resolve));
    const looking = chooseFile();
    await vi.advanceTimersByTimeAsync(0);
    goToPage('home', 'heading');
    answer({ kind: 'document', folder: '/t', file: null });
    await looking;
    expect([startScreen.looking, opened]).toEqual([null, []]);
  });

  it('a look that fails is said', async () => {
    answers.pickFile = () => '/a.tex';
    answers.inspectChoice = () => {
      throw 'no';
    };
    await chooseFile();
    expect([startScreen.message?.text, startScreen.looking]).toEqual(['no', null]);
  });

  it('nothing is chosen while a document is being opened', async () => {
    app.opening = 'x';
    chosen({ kind: 'word' });
    await chooseFile();
    await chooseFolder();
    expect(only('pickFile', 'pickFolder')).toEqual([]);
  });

  it('Find it… asks the same questions, on the Open page', async () => {
    goToPage('home', 'first');
    chosen({ kind: 'crowdedFolder', place: ['Downloads'] });
    await findRecent('/a');
    expect([startScreen.page, startScreen.choice?.kind, opened]).toEqual(['open', 'crowdedFolder', []]);
    expect(only('recentMoved')).toEqual([]);
  });

  it('Ctrl O focuses the Open page’s first button', () => {
    startScreen.visible = false;
    showStart('open', false);
    expect([startScreen.page, startScreen.focusTarget]).toEqual(['open', 'first']);
  });
});
