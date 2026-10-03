import { describe, expect, it } from 'vitest';
import { defaultFile, snapshotLabel } from './snapshots.svelte';
import type { SnapshotRow } from './ipc';

const row = (over: Partial<SnapshotRow> = {}): SnapshotRow => ({ id: 'a'.repeat(40), shortId: 'aaaaaaa', time: 1_000, words: 3210, ...over });

describe('snapshotLabel', () => {
  it('says when and how much', () => {
    expect(snapshotLabel(row({ time: 1_000 }), 1_000 * 1000 + 4 * 60 * 1000)).toBe('4 minutes ago · 3,210 words');
  });

  it('says "1 word" and "0 words" correctly', () => {
    expect(snapshotLabel(row({ words: 1 }), 1_000_000)).toMatch(/· 1 word$/);
    expect(snapshotLabel(row({ words: 0 }), 1_000_000)).toMatch(/· 0 words$/);
  });
});

describe('defaultFile', () => {
  it('prefers the root file', () => {
    expect(defaultFile(['chapters/a.tex', 'main.tex'], 'main.tex')).toBe('main.tex');
  });

  it('falls back to the first file when the root is not in that version', () => {
    expect(defaultFile(['chapters/a.tex', 'z.tex'], 'main.tex')).toBe('chapters/a.tex');
    expect(defaultFile(['chapters/a.tex'], null)).toBe('chapters/a.tex');
  });

  it('is nothing for a snapshot with no text files', () => {
    expect(defaultFile([], 'main.tex')).toBeNull();
  });
});
