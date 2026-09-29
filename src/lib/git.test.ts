// The pure half of `git.svelte.ts` (S10.3a): the row shape DESIGN.md §6 asks for, and the count
// the activity bar's badge shows. Everything here is a function of its arguments, so no Svelte
// runtime and no `invoke` are involved — the store itself is exercised through the controller in
// `controller.test.ts`.

import { describe, expect, it } from 'vitest';
import type { FileChange, GitStatus } from './ipc';
import { changedFileCount, letterFor, rowsOf, splitPath } from './git.svelte';

function change(path: string, kind: FileChange['kind'], renamedFrom: string | null = null): FileChange {
  return { path, kind, renamedFrom };
}

describe('the row shape', () => {
  it('splits a path into the name and the folder shown after it', () => {
    expect(splitPath('sections/methods/intro.tex')).toEqual(['intro.tex', 'sections/methods']);
  });

  it('leaves a file at the project root with no folder to show', () => {
    expect(splitPath('main.tex')).toEqual(['main.tex', '']);
  });

  it('gives every kind the letter §6 asks for, and a conflict the one mark that is not a letter', () => {
    expect(letterFor('modified')).toBe('M');
    expect(letterFor('added')).toBe('A');
    expect(letterFor('deleted')).toBe('D');
    expect(letterFor('renamed')).toBe('R');
    expect(letterFor('untracked')).toBe('U');
    expect(letterFor('conflicted')).toBe('!');
  });

  it('keeps where a rename came from, so a row can say it', () => {
    const [row] = rowsOf([change('chapters/two.tex', 'renamed', 'chapters/second.tex')]);
    expect(row).toMatchObject({ name: 'two.tex', dir: 'chapters', letter: 'R', renamedFrom: 'chapters/second.tex' });
  });
});

describe("the badge's count", () => {
  const staged_then_edited_again: GitStatus = {
    staged: [change('main.tex', 'modified')],
    unstaged: [change('main.tex', 'modified')],
    conflicted: [],
  };

  it('counts one changed file once, however many lists it is in', () => {
    expect(staged_then_edited_again.staged.length + staged_then_edited_again.unstaged.length).toBe(2);
    expect(changedFileCount(staged_then_edited_again)).toBe(1);
  });

  it('counts conflicts too — they are changed files whatever else they are', () => {
    expect(
      changedFileCount({ staged: [], unstaged: [change('a.tex', 'modified')], conflicted: [change('b.tex', 'conflicted')] }),
    ).toBe(2);
  });

  it('is zero on a clean tree', () => {
    expect(changedFileCount({ staged: [], unstaged: [], conflicted: [] })).toBe(0);
  });
});
