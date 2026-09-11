import { describe, expect, it } from 'vitest';
import { fuzzyMatch, rank } from './fuzzy';

const files = ['main.tex', 'refs.bib', 'sections/intro.tex', 'sections/results.tex', 'resources/figure.tex', 'domain-notes.tex', 'main-old.tex'];

function order(query: string): string[] {
  return rank(query, files, (f) => f).map((r) => r.item);
}

describe('fuzzyMatch', () => {
  it('matches a subsequence, case-insensitively, and says where', () => {
    const m = fuzzyMatch('SecRes', 'sections/results.tex');
    expect(m).not.toBeNull();
    expect(m!.positions.map((i) => 'sections/results.tex'[i]).join('')).toBe('secres');
  });

  it('is null when a character is missing or out of order', () => {
    expect(fuzzyMatch('xyz', 'main.tex')).toBeNull();
    expect(fuzzyMatch('tm', 'main.tex')).toBeNull(); // both letters exist, but no 'm' follows the 't'
  });

  it('an empty query matches everything with no score', () => {
    expect(fuzzyMatch('', 'anything')).toEqual({ score: 0, positions: [] });
  });
});

describe('rank', () => {
  it('an empty query keeps the given order', () => {
    expect(order('')).toEqual(files);
  });

  it('prefers the basename over the same letters spread across a path', () => {
    expect(order('res')[0]).toBe('sections/results.tex');
  });

  it('prefers the exact shorter name over a longer one that also matches', () => {
    expect(order('main')[0]).toBe('main.tex');
    expect(order('main')[1]).toBe('main-old.tex');
    // `domain-notes.tex` contains m-a-i-n too, buried mid-word.
    expect(order('main')).toContain('domain-notes.tex');
    expect(order('main').indexOf('domain-notes.tex')).toBe(2);
  });

  it('drops candidates that do not match at all', () => {
    expect(order('bib')).toEqual(['refs.bib']);
  });

  it('word starts beat mid-word letters', () => {
    // 'in' starts `intro`; in `main.tex` it is the tail of the word.
    expect(order('in')[0]).toBe('sections/intro.tex');
  });
});
