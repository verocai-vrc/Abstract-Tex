import { describe, expect, it } from 'vitest';
import type { ProjectInfo } from './ipc';
import { baseName, isCitingSource, isTexSource, listFiles, shouldCompileFor, toRelative } from './paths';

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

describe('shouldCompileFor', () => {
  type DocumentGraph = Pick<ProjectInfo, 'documentFiles' | 'documentFilesComplete'>;
  const complete = (documentFiles: string[]): DocumentGraph => ({ documentFiles, documentFilesComplete: true });

  it('always compiles for a non-.tex source the document depends on', () => {
    expect(shouldCompileFor('refs.bib', complete(['main.tex']))).toBe(true);
    expect(shouldCompileFor('preamble.sty', complete(['main.tex']))).toBe(true);
  });

  it('never compiles for a file the pipeline has no opinion about', () => {
    expect(shouldCompileFor('figures/plot.png', complete(['main.tex']))).toBe(false);
  });

  it('compiles a .tex file the graph knows is part of the document', () => {
    expect(shouldCompileFor('sections/intro.tex', complete(['main.tex', 'sections/intro.tex']))).toBe(true);
  });

  it('does not compile a .tex file the graph knows is not included', () => {
    expect(shouldCompileFor('figures/plot.tex', complete(['main.tex', 'sections/intro.tex']))).toBe(false);
  });

  it('compiles every .tex file when the graph is incomplete', () => {
    const incomplete: DocumentGraph = { documentFiles: ['main.tex'], documentFilesComplete: false };
    expect(shouldCompileFor('anything/at/all.tex', incomplete)).toBe(true);
  });
});

describe('listFiles', () => {
  it('flattens files depth-first and leaves directories out', () => {
    const tree = [
      { name: 'main.tex', path: 'main.tex', isDir: false, children: [] },
      {
        name: 'sections',
        path: 'sections',
        isDir: true,
        children: [
          { name: 'intro.tex', path: 'sections/intro.tex', isDir: false, children: [] },
          { name: 'figs', path: 'sections/figs', isDir: true, children: [] },
        ],
      },
      { name: 'refs.bib', path: 'refs.bib', isDir: false, children: [] },
    ];
    expect(listFiles(tree)).toEqual(['main.tex', 'sections/intro.tex', 'refs.bib']);
  });
});

// Review finding R8.20: the paste-to-cite handler was installed for every open file, so a DOI pasted into
// the `doi = {...}` field of a `.bib` was replaced by `\cite{key}` and a new entry was appended.
describe('isCitingSource', () => {
  it('is true for the files a document is written in', () => {
    expect(isCitingSource('main.tex')).toBe(true);
    expect(isCitingSource('chapters/intro.tex')).toBe(true);
    expect(isCitingSource('MAIN.TEX')).toBe(true);
    expect(isCitingSource('paper.ltx')).toBe(true);
  });

  it('is false for a bibliography, where a pasted DOI belongs in a field as text', () => {
    expect(isCitingSource('refs.bib')).toBe(false);
    expect(isCitingSource('zotero/My Library.bib')).toBe(false);
  });

  it('is false for style and class files and for everything that is not LaTeX', () => {
    for (const path of ['style.sty', 'thesis.cls', 'notes.txt', 'data.csv', 'README.md', 'main.tex.bak', 'tex', '']) {
      expect(isCitingSource(path), path).toBe(false);
    }
  });
});
