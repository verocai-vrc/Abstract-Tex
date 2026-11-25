// What Home draws, as a string: there is no DOM here (S17.18).
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { SEARCH_FROM, startScreen } from '../lib/start.svelte';
import { words } from '../lib/words';
import StartHome from './StartHome.svelte';

describe('the search of the recent documents', () => {
  function drawn(count: number): { rows: number; searchBoxes: number } {
    startScreen.recents = Array.from({ length: count }, (_, i) => ({
      folder: `/${i}`, rootFile: 'main.tex', title: `Essay ${i}`, place: [], usedAt: 1, picture: null,
    }));
    startScreen.query = 'zebra';
    const { body } = render(StartHome);
    return { rows: body.split(words.showInFolder + '<').length - 1, searchBoxes: body.split('type="search"').length - 1 };
  }

  it('counts only while its box is there to be cleared', () => {
    expect(drawn(SEARCH_FROM)).toEqual({ rows: 0, searchBoxes: 1 });
    expect(drawn(SEARCH_FROM - 1)).toEqual({ rows: SEARCH_FROM - 1, searchBoxes: 0 });
  });
});
