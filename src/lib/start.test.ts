// S17.4: the start screen's decisions, with literals and no window.
import { describe, expect, it } from 'vitest';
import type { RecentDocument } from './ipc';
import { chordOnStartScreen, documentToCarryOn, homeSections, homeTemplates, placeText, searchRecents } from './start.svelte';
import { words } from './words';

function recent(folder: string, title: string, place: string[] = []): RecentDocument {
  return { folder, rootFile: 'main.tex', title, place, usedAt: 1, picture: null };
}
const a = recent('/a', 'A');
const b = recent('/b', 'B');

describe('the document to carry on', () => {
  it('is the newest that is not known to be missing', () => {
    expect(documentToCarryOn([a, b], new Map([['/a', 'missing']]))?.folder).toBe('/b');
    expect(documentToCarryOn([a, b], new Map())?.folder).toBe('/a'); // a check still running does not skip
    expect(documentToCarryOn([a], new Map([['/a', 'missing']]))).toBeNull();
    expect(documentToCarryOn([], new Map())).toBeNull();
  });
});

describe('searching the recent documents', () => {
  const thesis = recent('/t', 'Thesis draft', ['Documents', 'Abstract-Tex']);
  const letter = recent('/l', 'Letter', ['Documents', 'Abstract-Tex']);

  it('wants every word, in the title or the place', () => {
    expect(searchRecents([thesis, letter], 'abstract thes')).toEqual([thesis]);
    expect(searchRecents([thesis, letter], 'ABSTRACT')).toEqual([thesis, letter]);
    expect(searchRecents([thesis, letter], '  ')).toEqual([thesis, letter]);
    expect(searchRecents([thesis, letter], 'thesis zebra')).toEqual([]);
  });

  it('finds an accented title typed without its accents', () => {
    const cafe = recent('/c', 'Café à São Paulo');
    expect(searchRecents([cafe, letter], 'cafe sao')).toEqual([cafe]);
  });
});

it('a place is read from the home folder down', () => {
  expect(placeText(['Documents', 'Abstract-Tex'])).toBe('Documents › Abstract-Tex');
  expect(placeText([])).toBe(words.homeFolder);
});

it('what Home shows, and in which order', () => {
  expect(homeSections(0, false)).toEqual(['welcome', 'templates', 'open']);
  expect(homeSections(3, false)).toEqual(['carry-on', 'templates', 'recents']);
  expect(homeSections(3, true)).toEqual(['templates', 'recents']);
  expect(homeSections(0, true)).toEqual(['templates']);
});

it('Home shows five templates, and the first five when those are not there', () => {
  const template = (id: string) => ({ id, name: id, category: 'x', description: '', previewUrl: '', fields: [] });
  const all = ['blank', 'essay', 'report', 'letter', 'paper', 'cv', 'thesis', 'slides'].map(template);
  expect(homeTemplates(all).map((t) => t.id)).toEqual(['blank', 'essay', 'report', 'paper', 'thesis']);
  expect(homeTemplates(all.slice(3)).map((t) => t.id)).toEqual(['paper', 'thesis', 'letter', 'cv', 'slides']);
});

it('what a chord does while the start screen is up', () => {
  for (const action of ['home', 'new-project', 'open-folder', 'command-palette'] as const)
    expect([chordOnStartScreen(action, true), chordOnStartScreen(action, false)]).toEqual(['in-place', 'in-place']);
  for (const action of ['compile', 'next-placeholder', 'quick-open', 'view-files', 'view-source-control', 'find-in-pdf'] as const)
    expect([chordOnStartScreen(action, true), chordOnStartScreen(action, false)]).toEqual(['back-first', 'nothing']);
  expect([chordOnStartScreen('save', true), chordOnStartScreen('save', false)]).toEqual(['in-place', 'nothing']);
});
