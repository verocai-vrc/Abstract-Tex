import { describe, expect, it } from 'vitest';
import { fuzzyMatch, highlightMatch, rank } from './fuzzy';

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

// R7.6. File names are not always one ASCII letter per UTF-16 unit.
describe('names with accents, decomposed accents and astral characters (R7.6)', () => {
  const LONE_SURROGATE = /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/;

  /** The text of the highlighted runs, joined. */
  function hitText(candidate: string, query: string): string | null {
    const match = fuzzyMatch(query, candidate);
    if (!match) return null;
    return highlightMatch(candidate, match.positions)
      .filter((segment) => segment.hit)
      .map((segment) => segment.text)
      .join('');
  }

  it('finds an accented name by the letters without their accents', () => {
    expect(fuzzyMatch('introducao', 'sections/introdução.tex')).not.toBeNull();
    expect(fuzzyMatch('resume', 'Résumé.tex')).not.toBeNull();
    expect(fuzzyMatch('Résumé', 'resume.tex')).not.toBeNull();
  });

  it('finds a name a file system stored with decomposed accents (macOS) by the accent a keyboard types', () => {
    const decomposed = 'Re\u0301sume\u0301.tex';
    expect(fuzzyMatch('r\u00e9sum\u00e9', decomposed)).not.toBeNull();
    // and the accent travels with its letter in the highlight
    expect(hitText(decomposed, 'r\u00e9sum\u00e9')).toBe('Re\u0301sume\u0301');
  });

  it('highlights exactly the letters that matched in a name with the Turkish capital dotted I', () => {
    // "İ".toLowerCase() is two code units, so positions taken from the lowercased copy drifted by one.
    const candidate = '\u0130\u00e7indekiler.tex';
    expect(hitText(candidate, 'kiler')).toBe('kiler');
    expect(hitText(candidate, 'tex')).toBe('tex');
    expect(hitText('x-\u0130\u0130\u0130-tex.tex', 'tex')).toBe('tex');
  });

  it('never splits a character made of two UTF-16 units across the highlight', () => {
    const candidate = 'a\u{1F600}b.tex';
    const segments = highlightMatch(candidate, fuzzyMatch('\u{1F600}b', candidate)!.positions);
    for (const segment of segments) expect(segment.text).not.toMatch(LONE_SURROGATE);
    expect(hitText(candidate, '\u{1F600}b')).toBe('\u{1F600}b');
    expect(highlightMatch('\u{1F600}', [0]).map((segment) => segment.text)).toEqual(['\u{1F600}']);
  });

  it('still reports positions into the name as it is written, for the ASCII case', () => {
    expect(fuzzyMatch('SecRes', 'sections/results.tex')!.positions).toEqual([0, 1, 2, 9, 10, 11]);
  });
});

describe('the ASCII path and the letter path agree (R7.6)', () => {
  // `fuzzyMatch` takes the cheap path for an ASCII name and the letter path for the rest. A name with
  // one accent forces the second, so the same ASCII name with an accented letter appended must score
  // and place its matches the same way up to that letter.
  const names = ['main.tex', 'sections/results.tex', 'resources/figure.tex', 'domain-notes.tex', 'a/b/c/d.tex'];
  const queries = ['main', 'res', 'secres', 'dn', 'tex', 'x', 'abcd', 'MAIN'];

  it('give the same positions and the same ordering for ASCII names', () => {
    for (const query of queries) {
      for (const name of names) {
        const cheap = fuzzyMatch(query, name);
        const forced = fuzzyMatch(query, name + '\u00e9'); // not ASCII any more: the letter path
        expect(forced === null, `${query} / ${name}`).toBe(cheap === null);
        if (cheap && forced) {
          expect(forced.positions, `${query} / ${name}`).toEqual(cheap.positions);
          expect(forced.score + 0.01, `${query} / ${name}`).toBeCloseTo(cheap.score, 5); // one more character: 0.01 less
        }
      }
    }
  });
});
