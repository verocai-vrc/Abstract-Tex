// The lookups over the index (S7.2) and the health findings (S8.3). The store itself is
// exercised end to end in `controller.test.ts`; these are the pure helpers with a literal index.

import { describe, expect, it } from 'vitest';
import { citationsByKey, entriesByKey, findingsByFile, lineAtByteOffset } from './bibliography.svelte';
import type { BibEntrySummary, BibliographyIndex, Finding } from './ipc';

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
  hasNociteStar: false,
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

describe('lineAtByteOffset (S8.3)', () => {
  it('counts newlines up to the offset', () => {
    const text = 'line one\nline two\nline three';
    expect(lineAtByteOffset(text, 0)).toBe(1);
    expect(lineAtByteOffset(text, 'line one\n'.length)).toBe(2);
    expect(lineAtByteOffset(text, text.length)).toBe(3);
  });

  it('counts UTF-8 bytes, not UTF-16 code units, so a non-ASCII author name does not throw off a later line', () => {
    // "Ærø" is 3 characters but 5 UTF-8 bytes (Æ and ø are each 2 bytes in UTF-8). The byte
    // offset of the second line, computed the same way `texbib` computes a `Span`, is bigger
    // than the string's own length in JS characters would suggest — a caller that mixed the two
    // up would ask for a line before the newline has actually been counted.
    const firstLine = 'author = {Ærø, Søren}\n';
    const text = `${firstLine}year = 2019`;
    const byteOffsetOfSecondLine = new TextEncoder().encode(firstLine).length;
    expect(byteOffsetOfSecondLine).toBeGreaterThan(firstLine.length); // proves the two units differ here
    expect(lineAtByteOffset(text, byteOffsetOfSecondLine)).toBe(2);
  });
});

describe('findingsByFile (S8.3)', () => {
  function finding(file: string, message: string): Finding {
    return { rule: 'missing-field', severity: 'error', message, jump: { kind: 'bibEntry', file, span: { start: 0, end: 1 } } };
  }

  it('groups findings by file in first-appearance order', () => {
    const findings = [finding('a.bib', '1'), finding('b.bib', '2'), finding('a.bib', '3')];
    const groups = findingsByFile(findings);
    expect(groups.map((g) => g.file)).toEqual(['a.bib', 'b.bib']);
    expect(groups[0]?.findings.map((f) => f.message)).toEqual(['1', '3']);
  });

  it('is empty for no findings', () => {
    expect(findingsByFile([])).toEqual([]);
  });

  it('groups a missing linked export under its own path (S8.6)', () => {
    const missing: Finding = {
      rule: 'missing-bib-file',
      severity: 'warning',
      message: 'not on disk yet',
      jump: { kind: 'missingFile', file: 'zotero/Thesis.bib' },
    };
    const groups = findingsByFile([finding('refs.bib', '1'), missing]);
    expect(groups.map((g) => g.file)).toEqual(['refs.bib', 'zotero/Thesis.bib']);
  });
});
