import { describe, expect, it } from 'vitest';
import { counts, defaultChoices, previewText, segments, summaryLine, toggled } from './assistant-review';
import type { AssistantHunk } from './ipc';

const original = 'Prior work is very very old. Done.';
const hunks: AssistantHunk[] = [
  { start: 14, end: 24, replacement: '', refusal: null }, // "very very "
  { start: 24, end: 24, replacement: '\\cite{invented}', refusal: ['The edit cites “invented”, which is not an entry in your .bib files, so it was held back.'] },
];

describe('segments', () => {
  it('cuts the selection at the hunks and keeps the text between them', () => {
    const parts = segments(original, hunks);
    expect(parts.map((part) => part.kind)).toEqual(['same', 'change', 'change', 'same']);
    expect(parts[0]).toEqual({ kind: 'same', text: 'Prior work is ' });
    expect(parts[1]).toMatchObject({ kind: 'change', index: 0, removed: 'very very ', added: '' });
    expect(parts[2]).toMatchObject({ kind: 'change', index: 1, removed: '', added: '\\cite{invented}' });
    expect(parts[3]).toEqual({ kind: 'same', text: 'old. Done.' });
  });

  it('skips a hunk that is out of order or out of range rather than slicing backwards', () => {
    const bad: AssistantHunk[] = [
      { start: 10, end: 12, replacement: 'x', refusal: null },
      { start: 5, end: 8, replacement: 'y', refusal: null },
      { start: 30, end: 999, replacement: 'z', refusal: null },
    ];
    const parts = segments(original, bad);
    expect(parts.filter((part) => part.kind === 'change')).toHaveLength(1);
    expect(previewText(original, bad, [false, false, false])).toBe(original);
  });

  it('is the whole text when there are no changes', () => {
    expect(segments('Same.', [])).toEqual([{ kind: 'same', text: 'Same.' }]);
    expect(segments('', [])).toEqual([]);
  });
});

describe('choices', () => {
  it('start accepted for the allowed hunks only', () => {
    expect(defaultChoices(hunks)).toEqual([true, false]);
  });

  it('can be flipped, and a refused hunk can never be turned on', () => {
    expect(toggled(hunks, [true, false], 0)).toEqual([false, false]);
    expect(toggled(hunks, [true, false], 1)).toEqual([true, false]);
    expect(toggled(hunks, [true, false], 9)).toEqual([true, false]);
  });

  it('are counted against what was allowed, and a refused hunk never counts as accepted', () => {
    expect(counts(hunks, [true, true])).toEqual({ accepted: 1, allowed: 1 });
    expect(counts(hunks, [false, false])).toEqual({ accepted: 0, allowed: 1 });
  });
});

describe('previewText', () => {
  it('shows the chosen changes and the original where a change is left', () => {
    expect(previewText(original, hunks, [true, false])).toBe('Prior work is old. Done.');
    expect(previewText(original, hunks, [false, false])).toBe(original);
  });

  it('counts UTF-16 units, as the hunks do', () => {
    const text = '𝒳 very good'; // the astral letter is two units
    const astral: AssistantHunk[] = [{ start: 3, end: 8, replacement: '', refusal: null }];
    expect(previewText(text, astral, [true])).toBe('𝒳 good');
  });
});

describe('summaryLine', () => {
  it('says how many are accepted and how many were held back, and never "0 of 0"', () => {
    expect(summaryLine(hunks, [true, false])).toBe('1 of 1 change accepted; 1 held back.');
    expect(summaryLine([hunks[0]!], [true])).toBe('1 of 1 change accepted.');
    expect(summaryLine([], [])).toBe('The assistant suggests no changes.');
    expect(summaryLine([hunks[0]!, { ...hunks[0]!, start: 24, end: 24 }], [true, false])).toBe('1 of 2 changes accepted.');
  });
});
