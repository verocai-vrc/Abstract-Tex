import { describe, expect, it } from 'vitest';
import { isDiffable, sideLabels } from './diffview.svelte';

describe('isDiffable', () => {
  it('is for the files an author writes', () => {
    expect(isDiffable('main.tex')).toBe(true);
    expect(isDiffable('chapters/Intro.TEX')).toBe(true);
    expect(isDiffable('refs.bib')).toBe(true);
  });

  it('is not for anything else', () => {
    expect(isDiffable('figure.png')).toBe(false);
    expect(isDiffable('style.sty')).toBe(false);
    expect(isDiffable('abstract-tex.toml')).toBe(false);
    expect(isDiffable('notes.tex.bak')).toBe(false);
    expect(isDiffable('tex')).toBe(false);
  });
});

describe('sideLabels', () => {
  it('names the sides for what Stage and Commit do', () => {
    expect(sideLabels(false)).toEqual({ before: 'Before staging', after: 'Your file' });
    expect(sideLabels(true)).toEqual({ before: 'Last commit', after: 'Staged' });
  });
});
