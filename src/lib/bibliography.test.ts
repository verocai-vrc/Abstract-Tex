// The two lookups over the index (S7.2). The store itself is exercised end to end in
// `controller.test.ts`; these are the pure helpers with a literal index.

import { describe, expect, it } from 'vitest';
import { citationsByKey, entriesByKey } from './bibliography.svelte';
import type { BibEntrySummary, BibliographyIndex } from './ipc';

function entry(key: string, file = 'refs.bib'): BibEntrySummary {
  return { key, entryType: 'article', author: null, year: null, title: null, file, span: { start: 0, end: 1 } };
}

const index: BibliographyIndex = {
  files: [],
  entries: [entry('a'), entry('b'), entry('a', 'more.bib')],
  citations: [
    { key: 'a', file: 'main.tex', line: 1 },
    { key: 'c', file: 'main.tex', line: 2 },
    { key: 'a', file: 'intro.tex', line: 5 },
  ],
};

describe('entriesByKey', () => {
  it('keeps the first definition of a duplicated key, as BibTeX does', () => {
    const map = entriesByKey(index);
    expect([...map.keys()]).toEqual(['a', 'b']);
    expect(map.get('a')?.file).toBe('refs.bib');
  });

  it('is empty before an index has arrived', () => {
    expect(entriesByKey(null).size).toBe(0);
  });
});

describe('citationsByKey', () => {
  it('groups every citation under its key in document order', () => {
    const map = citationsByKey(index);
    expect(map.get('a')?.map((c) => `${c.file}:${c.line}`)).toEqual(['main.tex:1', 'intro.tex:5']);
    // A cited key with no entry is still listed: that is the "undefined citation" S8.3 reports.
    expect(map.get('c')).toHaveLength(1);
    expect(map.has('b')).toBe(false);
  });
});
