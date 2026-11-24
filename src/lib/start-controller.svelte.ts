// What the start screen does (S17.4). Beside `controller.svelte.ts`, which it imports and which never
// imports it: the document's flows know nothing of the screen in front of them, except whether it is up.

import { registerCommand } from './commands';
import {
  chooseTemplate,
  loadTemplates,
  openFolder,
  pickAndOpenFolder,
  showNewProjectWindow,
  tell,
} from './controller.svelte';
import { ipc } from './ipc';
import { documentToCarryOn, startScreen, type Presence, type StartPage } from './start.svelte';
import { app } from './state.svelte';
import { newProject } from './templates.svelte';

/** How long the folders get to answer before *Carry on* is drawn. After it, one that has not answered
 * is taken to be there. */
const CARRY_ON_WAITS_MS = 150;

/** Show a page of the start screen, with the focus on its heading or on the first thing to press. */
export function goToPage(page: StartPage, focus: 'heading' | 'first'): void {
  startScreen.page = page;
  startScreen.focusTarget = focus;
  startScreen.focusNonce += 1;
  startScreen.message = null;
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

/** Open a recent document. A refusal is a sentence on this screen, which stays. */
export async function openRecent(folder: string): Promise<void> {
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

/** A template on Home: straight to its questions (in today's window, until S17.6). */
export function startFromTemplate(id: string): void {
  if (app.opening !== null) return;
  showNewProjectWindow();
  chooseTemplate(id);
}

/** `Ctrl O`: the Open page while the start screen is up, and the folder picker otherwise (until S17.5). */
export function openDocument(): void {
  if (startScreen.visible) goToPage('open', 'first');
  else void pickAndOpenFolder();
}

registerCommand({
  id: 'open-folder',
  title: 'Open folder…',
  category: 'action',
  shortcut: 'Ctrl O',
  withoutDocument: true,
  run: openDocument,
});
