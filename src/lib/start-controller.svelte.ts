// What the start screen does (S17.4). Beside `controller.svelte.ts`, which it imports and which never
// imports it: the document's flows know nothing of the screen in front of them, except whether it is up.

import { allCommands, registerCommand, type Command } from './commands';
import { openFolder, tell, whyNotToLeaveProject } from './controller.svelte';
import { ipc, type Preferences } from './ipc';
import { documentToCarryOn, startScreen, type Presence, type StartPage } from './start.svelte';
import { app } from './state.svelte';
import { filterTemplates, neighbour, newProject } from './templates.svelte';
import { words } from './words';

/** How long the folders get to answer before *Carry on* is drawn. After it, one that has not answered
 * is taken to be there. */
const CARRY_ON_WAITS_MS = 150;

/** Show a page of the start screen, with the focus on its heading or on the first thing to press. */
export function goToPage(page: StartPage, focus: 'heading' | 'first'): void {
  startScreen.page = page;
  startScreen.message = null;
  if (page === 'new' && !newProject.creating) startNewAfresh();
  moveFocus(focus);
}

function moveFocus(target: 'heading' | 'first'): void {
  startScreen.focusTarget = target;
  startScreen.focusNonce += 1;
}

/**
 * Show a page of the start screen, over the document if there is one (S17.5). Not while a dialog is up:
 * it would be left open over a page it has nothing to do with, or under it.
 */
export function showStart(page: StartPage, modalIsUp: boolean): void {
  if (modalIsUp || app.launching || app.opening !== null) return;
  startScreen.visible = true;
  goToPage(page, page === 'home' ? 'heading' : 'first');
}

/** Back to the document, with the cursor where it was. Nothing happens if there is none. */
export function backToDocument(): void {
  if (!app.project || app.opening !== null) return;
  startScreen.visible = false;
  startScreen.message = null;
  app.editorFocusRequest += 1;
}

/** One step back (Esc): from New's questions to its cards, and from any page to the document, or with
 * none open to Home. */
export function startBack(): void {
  if (app.opening !== null) return; // `OpeningNotice` hears the same key as Stop
  if (startScreen.page === 'new' && newProject.step === 'fill') {
    if (newProject.creating) return;
    newProject.step = 'choose';
    newProject.error = null;
    moveFocus('first');
    return;
  }
  if (app.project) backToDocument();
  else if (startScreen.page !== 'home') goToPage('home', 'heading');
}

/** Run a palette entry. One that is about the document goes back to it first. */
export function runFromPalette(id: string, entries: readonly Command[] = allCommands()): void {
  const command = entries.find((entry) => entry.id === id);
  if (!command) return;
  if (startScreen.visible && !command.withoutDocument) backToDocument();
  command.run();
}

/** Bumped at each load, so that what an older one still hears changes nothing. */
let loadEpoch = 0;

/**
 * Read the list and check each folder. The rows are drawn from the list alone; each check then changes
 * its own row and nothing else. A check takes 2 seconds at most (Rust's limit).
 */
export async function loadRecents(): Promise<void> {
  const epoch = ++loadEpoch;
  if (newProject.catalog === null) void loadTemplates();
  const recents = await ipc.recentDocuments().catch(() => []);
  if (epoch !== loadEpoch) return;
  startScreen.recents = recents;
  startScreen.presence = new Map();
  const checks = recents.map(async ({ folder }) => {
    const isHere = await ipc.recentIsHere(folder).catch(() => false);
    if (epoch !== loadEpoch) return;
    const presence: Presence = isHere ? 'here' : 'missing';
    startScreen.presence = new Map(startScreen.presence).set(folder, presence);
  });
  await Promise.race([Promise.all(checks), new Promise((resolve) => setTimeout(resolve, CARRY_ON_WAITS_MS))]);
  if (epoch !== loadEpoch) return;
  startScreen.carryOn = documentToCarryOn(recents, startScreen.presence)?.folder ?? null;
  startScreen.decided = true;
}

function titleOf(folder: string): string | undefined {
  return startScreen.recents?.find((recent) => recent.folder === folder)?.title;
}

/** Open a recent document, or go back to it if it is the one that is open. A refusal is a sentence on
 * this screen, which stays. */
export async function openRecent(folder: string): Promise<void> {
  if (folder === app.project?.rootDir) return backToDocument();
  await openFolder(folder, { name: titleOf(folder) });
}

/** *Find it…*: pick where a missing document is now, open it, and correct its row. Only once it is
 * open: a folder that turns out not to be a document must not take the row's place. */
export async function findRecent(folder: string): Promise<void> {
  if (app.opening !== null) return;
  const found = await ipc.pickFolder().catch(() => null);
  if (!found) return;
  if (!(await openFolder(found, { name: titleOf(folder) }))) return;
  await ipc.recentMoved(folder, found).catch((error) => tell(String(error)));
}

async function changeList(change: () => Promise<void>): Promise<void> {
  try {
    await change();
  } catch (error) {
    tell(String(error));
    return;
  }
  await loadRecents();
}

/** Take one off the list. Its files are not touched. */
export function forgetRecent(folder: string): Promise<void> {
  return changeList(() => ipc.forgetRecent(folder));
}

export function clearRecents(): Promise<void> {
  return changeList(() => ipc.clearRecents());
}

export async function showRecentInFolder(folder: string): Promise<void> {
  await ipc.showRecentInFolder(folder).catch((error) => tell(String(error)));
}

// ---- A new document (S11.11, a page since S17.6) ------------------------------------------------

/** What was typed belongs to a document that has been made, so the next one starts without it. */
let typedIsSpent = false;

function startNewAfresh(): void {
  newProject.step = 'choose';
  newProject.error = null;
  newProject.query = '';
  newProject.category = 'all';
  if (typedIsSpent) newProject.values = {};
  typedIsSpent = false;
  if (newProject.catalog === null) void loadTemplates();
  else newProject.selectedId = newProject.catalog[0]?.id ?? null;
  // A failure is said when it matters: *Create* reads them again.
  void loadPreferences().then(offerAuthor, () => {});
}

/** The name given last time, unless this page already has an answer, be it an empty one. */
function offerAuthor(): void {
  const author = startScreen.preferences?.author;
  if (author && newProject.values.author === undefined) newProject.values.author = author;
}

async function loadPreferences(): Promise<Preferences> {
  const preferences = await ipc.preferences();
  startScreen.preferences = preferences;
  return preferences;
}

/** Ask Rust for the catalog. It is compiled into the app, so a failure here is a broken build and
 * says so in a sentence rather than showing an empty picker. */
async function loadTemplates(): Promise<void> {
  newProject.loadError = null;
  try {
    newProject.catalog = await ipc.templateList();
    newProject.selectedId ??= newProject.catalog[0]?.id ?? null;
  } catch (error) {
    newProject.loadError = String(error);
  }
}

/** Move the highlight along the cards that survive the filter, or to the first when the
 * highlighted one has been filtered away. */
export function moveTemplateSelection(delta: number): void {
  if (newProject.catalog === null) return;
  const shown = filterTemplates(newProject.catalog, newProject.category, newProject.query);
  newProject.selectedId = neighbour(shown, newProject.selectedId, delta);
}

/** Re-aim the highlight after the filter changed: keep it if the card is still shown. */
export function templateFilterChanged(): void {
  if (newProject.catalog === null) return;
  const shown = filterTemplates(newProject.catalog, newProject.category, newProject.query);
  if (!shown.some((template) => template.id === newProject.selectedId)) newProject.selectedId = shown[0]?.id ?? null;
}

/** Choose a card: on to the questions. */
export function chooseTemplate(id: string): void {
  newProject.selectedId = id;
  newProject.step = 'fill';
  newProject.error = null;
  offerAuthor();
  moveFocus('first');
}

/** A template on Home: straight to its questions. */
export function startFromTemplate(id: string): void {
  if (app.opening !== null) return;
  goToPage('new', 'first');
  chooseTemplate(id);
}

/** *Change…*: keep new documents somewhere else from now on. */
export async function changePlace(): Promise<void> {
  try {
    const place = await ipc.pickParentFolder(words.changePlaceTitle);
    if (place) startScreen.preferences = await ipc.setPreferences({ place });
  } catch (error) {
    newProject.error = String(error);
  }
}

/**
 * *Create*: make the document in the place new documents are kept and open it like any other, which
 * builds it at once, so the first thing its author sees is a PDF. A refusal (a taken name above all) is a
 * sentence on the page, which keeps everything typed.
 */
export async function createProject(): Promise<void> {
  const template = newProject.catalog?.find((candidate) => candidate.id === newProject.selectedId);
  if (!template || newProject.creating || app.opening !== null) return;
  const answers: Record<string, string> = {};
  for (const field of template.fields) answers[field.id] = (newProject.values[field.id] ?? '').trim();
  if ('title' in answers && answers.title === '') {
    newProject.error = words.titleNeeded;
    moveFocus('first');
    return;
  }
  newProject.error = null;
  newProject.creating = true;
  let folder: string;
  try {
    // Asked before anything is made: the result is opened at once, and opening is refused while a
    // tab holds text that cannot be saved (R6.9).
    const staying = await whyNotToLeaveProject();
    if (staying !== null) throw staying;
    const preferences = startScreen.preferences ?? (await loadPreferences());
    folder = await ipc.templateCreate(template.id, preferences.place.path, answers);
    if (answers.author && answers.author !== preferences.author) void rememberAuthor(answers.author);
  } catch (error) {
    newProject.error = String(error);
    return;
  } finally {
    newProject.creating = false;
  }
  typedIsSpent = true;
  if (await openFolder(folder, { atFirstPlaceholder: true, name: answers.title })) return;
  // It is made, and listed on Home. *Create* again would only say that its name is taken.
  newProject.values = {};
  typedIsSpent = false;
  newProject.step = 'choose';
}

/** A convenience, so a failure is not worth a word. */
async function rememberAuthor(author: string): Promise<void> {
  startScreen.preferences = await ipc.setPreferences({ author }).catch(() => startScreen.preferences);
}

// The palette is gone by the time its entry runs, so there is no dialog for these to be wary of.
registerCommand({
  id: 'home',
  title: 'Home',
  category: 'action',
  shortcut: 'Ctrl Shift H',
  withoutDocument: true,
  run: () => showStart('home', false),
});
registerCommand({
  id: 'new-project',
  title: 'New document…',
  category: 'action',
  shortcut: 'Ctrl Shift N',
  withoutDocument: true,
  run: () => showStart('new', false),
});
registerCommand({
  id: 'open-folder',
  title: 'Open a document…',
  category: 'action',
  shortcut: 'Ctrl O',
  withoutDocument: true,
  run: () => showStart('open', false),
});
