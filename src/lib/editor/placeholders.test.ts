import { describe, expect, it } from 'vitest';
import {
  findPlaceholderLines,
  isPlaceholderLine,
  leftToFillIn,
  nextPlaceholder,
  orderedTexFiles,
  texPathsOf,
} from './placeholders';
import type { TreeNode } from '../ipc';

describe('findPlaceholderLines', () => {
  it('finds the 1-based lines that carry the marker, wherever the comment starts', () => {
    const text = ['\\title{x}', '% FILL IN: say what it is about', 'text', 'more % FILL IN: and this', '  %FILL IN: no space'].join('\n');
    expect(findPlaceholderLines(text)).toEqual([2, 4, 5]);
  });

  it('is not fooled by an escaped percent sign, a different comment, or a different case', () => {
    expect(isPlaceholderLine('50\\% FILL IN: of the time')).toBe(false);
    expect(isPlaceholderLine('% fill in: lower case')).toBe(false);
    expect(isPlaceholderLine('% TODO: something else')).toBe(false);
    expect(isPlaceholderLine('FILL IN: no percent')).toBe(false);
  });

  it('finds none in a project with no markers, and the count falls as markers are deleted', () => {
    const text = '% FILL IN: a\ntext\n% FILL IN: b';
    expect(findPlaceholderLines(text)).toEqual([1, 3]);
    expect(findPlaceholderLines('text\n% FILL IN: b')).toEqual([2]);
    expect(findPlaceholderLines('text\ntext')).toEqual([]);
    expect(findPlaceholderLines('')).toEqual([]);
  });
});

describe('texPathsOf and orderedTexFiles', () => {
  const node = (path: string, children: TreeNode[] = [], isDir = false): TreeNode => ({
    name: path.split('/').pop()!,
    path,
    isDir,
    children,
  });
  const tree = [
    node('main.tex'),
    node('references.bib'),
    node('sections', [node('sections/a.tex'), node('sections/b.tex')], true),
    node('notes.tex'),
  ];

  it('lists the .tex files in tree order, through folders, and nothing else', () => {
    expect(texPathsOf(tree)).toEqual(['main.tex', 'sections/a.tex', 'sections/b.tex', 'notes.tex']);
  });

  it('puts the document files first, in their order, then the rest', () => {
    expect(orderedTexFiles(['main.tex', 'sections/b.tex', 'sections/a.tex'], texPathsOf(tree))).toEqual([
      'main.tex',
      'sections/b.tex',
      'sections/a.tex',
      'notes.tex',
    ]);
  });
});

describe('nextPlaceholder', () => {
  const order = ['main.tex', 'sections/a.tex', 'sections/b.tex'];
  const byFile = new Map<string, number[]>([
    ['main.tex', [4, 9]],
    ['sections/b.tex', [2]],
  ]);

  it('starts at the first one anywhere when there is no cursor', () => {
    expect(nextPlaceholder(order, byFile, null)).toEqual({ path: 'main.tex', line: 4 });
  });

  it('goes to the next one in the same file, strictly after the cursor', () => {
    expect(nextPlaceholder(order, byFile, { path: 'main.tex', line: 1 })).toEqual({ path: 'main.tex', line: 4 });
    expect(nextPlaceholder(order, byFile, { path: 'main.tex', line: 4 })).toEqual({ path: 'main.tex', line: 9 });
  });

  it('crosses into the next file that has one, skipping files that have none', () => {
    expect(nextPlaceholder(order, byFile, { path: 'main.tex', line: 9 })).toEqual({ path: 'sections/b.tex', line: 2 });
    expect(nextPlaceholder(order, byFile, { path: 'sections/a.tex', line: 50 })).toEqual({ path: 'sections/b.tex', line: 2 });
  });

  it('wraps round to the start after the last one', () => {
    expect(nextPlaceholder(order, byFile, { path: 'sections/b.tex', line: 2 })).toEqual({ path: 'main.tex', line: 4 });
  });

  it('comes back to the only one left, and says nothing when there are none', () => {
    const one = new Map([['main.tex', [3]]]);
    expect(nextPlaceholder(order, one, { path: 'main.tex', line: 3 })).toEqual({ path: 'main.tex', line: 3 });
    expect(nextPlaceholder(order, new Map(), null)).toBeNull();
  });

  it('treats a file outside the order as coming last', () => {
    expect(nextPlaceholder(order, byFile, { path: 'elsewhere.tex', line: 1 })).toEqual({ path: 'main.tex', line: 4 });
  });
});

describe('leftToFillIn', () => {
  it('says how many are left, and nothing at all for none', () => {
    expect(leftToFillIn(3)).toBe('3 left to fill in');
    expect(leftToFillIn(1)).toBe('1 left to fill in');
    expect(leftToFillIn(0)).toBe('');
  });
});
