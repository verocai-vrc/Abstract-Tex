import { describe, expect, it } from 'vitest';
import { baseName, isTexSource, toRelative } from './paths';

describe('toRelative', () => {
  it('strips the root and normalises separators', () => {
    expect(toRelative('C:\\thesis\\sections\\intro.tex', 'C:\\thesis')).toBe('sections/intro.tex');
    expect(toRelative('/home/a/thesis/main.tex', '/home/a/thesis')).toBe('main.tex');
  });

  it('is case-insensitive about the root, as Windows is', () => {
    expect(toRelative('c:\\Thesis\\main.tex', 'C:\\thesis')).toBe('main.tex');
  });

  it('returns null for paths outside the project and empty for the root itself', () => {
    expect(toRelative('C:\\other\\main.tex', 'C:\\thesis')).toBeNull();
    expect(toRelative('C:\\thesis-2\\main.tex', 'C:\\thesis')).toBeNull();
    expect(toRelative('C:\\thesis', 'C:\\thesis')).toBe('');
  });
});

describe('helpers', () => {
  it('baseName handles both separators', () => {
    expect(baseName('a/b/c.tex')).toBe('c.tex');
    expect(baseName('a\\b\\c.tex')).toBe('c.tex');
  });

  it('isTexSource recognises the files a compile depends on', () => {
    expect(isTexSource('main.tex')).toBe(true);
    expect(isTexSource('refs.bib')).toBe(true);
    expect(isTexSource('figure.png')).toBe(false);
  });
});
