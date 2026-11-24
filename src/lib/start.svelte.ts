// The start screen's state (S17.4), and its decisions as plain functions so that `start.test.ts`
// exercises them with literals.
//
// Only the controllers write to `startScreen`: `controller.svelte.ts` says whether it is up and what
// it has to say, `start-controller.svelte.ts` the rest. Components read it.

import type { Choice, Preferences, RecentDocument, SavedPdf, TemplateInfo, UnpackRefusal } from './ipc';
import type { Action } from './shortcuts';
import { words } from './words';

export type StartPage = 'home' | 'new' | 'open' | 'look' | 'help';
export type Presence = 'here' | 'missing';
export type HomeSection = 'welcome' | 'carry-on' | 'templates' | 'recents' | 'open';

/** Every reason a zip is refused for, so that a test can ask for each one's sentence (S17.9). */
export const UNPACK_REFUSALS = [
  'notAZip', 'locked', 'unsupported', 'tooMany', 'tooBig', 'unsafeName', 'sameName', 'noTex', 'io',
] as const satisfies readonly UnpackRefusal[];

/** The list gets a search box from this many documents up: fewer are read at a glance. */
export const SEARCH_FROM = 7;
/** How many templates Home shows before *See all*. */
const HOME_TEMPLATES = 5;
const FAVOURITES = ['blank', 'essay', 'report', 'paper', 'thesis'];

class StartScreenState {
  visible = $state(false);
  page = $state<StartPage>('home');

  /** Newest first. `null` until it has been read. Replaced whole, never edited. */
  recents = $state.raw<RecentDocument[] | null>(null);
  /** By folder. A folder that is not in it is still being checked. */
  presence = $state.raw<ReadonlyMap<string, Presence>>(new Map());
  /** Whether it has been decided which document *Carry on* offers. */
  decided = $state(false);
  /** The folder *Carry on* offers, or `null` for none. Once decided it is never another one: a card
   * must not change to another document under someone's finger. */
  carryOn = $state<string | null>(null);
  query = $state('');

  /** What was chosen on the Open page, when it cannot simply be opened. */
  choice = $state.raw<Exclude<Choice, { kind: 'document' }> | null>(null);
  /** The name of what is being looked at: its disk may be slow. */
  looking = $state<string | null>(null);

  /** `null` until they have been read. */
  preferences = $state.raw<Preferences | null>(null);

  message = $state<{ kind: 'status' | 'refusal'; text: string } | null>(null);

  /** Where the keyboard focus goes when a page is shown: its heading, or the first thing to press. */
  focusTarget = $state<'heading' | 'first'>('first');
  /** Bumped at each request, so that going to the page one is already on moves the focus too. */
  focusNonce = $state(0);
}

export const startScreen = new StartScreenState();

/** The newest document that is not known to be missing. One whose folder has not answered counts. */
export function documentToCarryOn(
  recents: readonly RecentDocument[],
  presence: ReadonlyMap<string, Presence>,
): RecentDocument | null {
  return recents.find((recent) => presence.get(recent.folder) !== 'missing') ?? null;
}

/** Lower case, and without accents: someone looking for "Café" may well type "cafe". */
function plain(text: string): string {
  return text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** The documents that have every word of `query` in their title or their place. */
export function searchRecents(recents: readonly RecentDocument[], query: string): RecentDocument[] {
  const wanted = plain(query).split(/\s+/).filter(Boolean);
  return recents.filter((recent) => {
    const text = plain([recent.title, ...recent.place].join(' '));
    return wanted.every((word) => text.includes(word));
  });
}

export function placeText(parts: readonly string[]): string {
  return parts.length === 0 ? words.homeFolder : parts.join(' › ');
}

export function savedPdfSentence(saved: SavedPdf): string {
  const sentence = saved.isNewest ? words.savedPdf : words.savedOlderPdf;
  return sentence(saved.name, placeText(saved.place));
}

export function homeSections(recentCount: number, hasDocument: boolean): HomeSection[] {
  if (hasDocument) return recentCount === 0 ? ['templates'] : ['templates', 'recents'];
  return recentCount === 0 ? ['welcome', 'templates', 'open'] : ['carry-on', 'templates', 'recents'];
}

/**
 * What a chord does while the start screen is up (S17.5). One that acts on the document goes back to it
 * first: nothing may change in a document nobody can see, except *Save*, which changes nothing in it.
 */
export function chordOnStartScreen(action: Action, hasDocument: boolean): 'in-place' | 'back-first' | 'nothing' {
  switch (action) {
    case 'home':
    case 'new-project':
    case 'open-folder':
    case 'command-palette':
      return 'in-place';
    case 'save':
      return hasDocument ? 'in-place' : 'nothing';
    default:
      return hasDocument ? 'back-first' : 'nothing';
  }
}

/** The five templates Home shows: the ones most people want, then whatever the catalog has. */
export function homeTemplates(catalog: readonly TemplateInfo[]): TemplateInfo[] {
  const wanted = FAVOURITES.flatMap((id) => catalog.filter((template) => template.id === id));
  const others = catalog.filter((template) => !FAVOURITES.includes(template.id));
  return [...wanted, ...others].slice(0, HOME_TEMPLATES);
}
