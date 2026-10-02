// The pure half of `git.svelte.ts`: the row shape DESIGN.md §6 asks for and the count the
// activity bar's badge shows (S10.3a), then the branch line, the sync arrows and a commit row's
// age (S10.3b). Everything here is a function of its arguments, so no Svelte
// runtime and no `invoke` are involved — the store itself is exercised through the controller in
// `controller.test.ts`.

import { describe, expect, it } from 'vitest';
import type { BranchState, FileChange, GitStatus, ProseSummary } from './ipc';
import {
  branchLabel,
  canAmend,
  changedFileCount,
  hasSyncWork,
  letterFor,
  outgoingCount,
  relativeTime,
  rowsOf,
  splitPath,
  suggestedMessage,
  syncArrows,
  syncOutcomeSentence,
  wordDeltaLabel,
} from './git.svelte';

function branch(over: Partial<BranchState> = {}): BranchState {
  return { name: 'main', aheadBehind: null, unborn: false, head: 'abcdef1234', ...over };
}

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

describe("the status bar's branch line (S10.3b)", () => {
  it('names the branch', () => {
    expect(branchLabel(branch())).toBe('main');
  });

  it('says so before the first commit, rather than looking like a branch with a history', () => {
    expect(branchLabel(branch({ unborn: true }))).toBe('main · no commits yet');
  });

  it('is empty where there is no repository, so the bar says nothing about Git at all', () => {
    expect(branchLabel(null)).toBe('');
  });

  it('names a detached HEAD instead of pretending there is a branch', () => {
    expect(branchLabel(branch({ name: null }))).toBe('detached HEAD');
  });
});

describe('the sync arrows (S10.3b)', () => {
  it('shows each side that is not zero', () => {
    expect(syncArrows(branch({ aheadBehind: [2, 1] }))).toBe('↑2 ↓1');
    expect(syncArrows(branch({ aheadBehind: [3, 0] }))).toBe('↑3');
    expect(syncArrows(branch({ aheadBehind: [0, 4] }))).toBe('↓4');
  });

  it('is quiet both when there is nothing to sync and when there is nowhere to sync to', () => {
    // Two different states — `[0, 0]` has an upstream and agrees with it, `null` has none — and
    // the difference matters to S11.1's button, not to this line.
    expect(syncArrows(branch({ aheadBehind: [0, 0] }))).toBe('');
    expect(syncArrows(branch({ aheadBehind: null }))).toBe('');
    expect(syncArrows(null)).toBe('');
  });

  it("counts nothing as outgoing when there is nowhere for it to go", () => {
    expect(outgoingCount(branch({ aheadBehind: null }))).toBe(0);
    expect(outgoingCount(branch({ aheadBehind: [2, 1] }))).toBe(2);
  });
});

describe('the Sync button (S11.1b)', () => {
  it('belongs on screen whenever either side is non-zero, and nowhere else', () => {
    expect(hasSyncWork(branch({ aheadBehind: [2, 1] }))).toBe(true);
    expect(hasSyncWork(branch({ aheadBehind: [3, 0] }))).toBe(true);
    expect(hasSyncWork(branch({ aheadBehind: [0, 4] }))).toBe(true);
    // The same two "nothing to show" states `syncArrows` already tells apart: an upstream the
    // branch agrees with, and no upstream at all.
    expect(hasSyncWork(branch({ aheadBehind: [0, 0] }))).toBe(false);
    expect(hasSyncWork(branch({ aheadBehind: null }))).toBe(false);
    expect(hasSyncWork(null)).toBe(false);
  });

  it('spells out singular and plural, so the sentence reads like one', () => {
    expect(syncOutcomeSentence({ kind: 'upToDate' })).toBe('Already up to date.');
    expect(syncOutcomeSentence({ kind: 'pushed', ahead: 1 })).toBe('Pushed 1 commit.');
    expect(syncOutcomeSentence({ kind: 'pushed', ahead: 3 })).toBe('Pushed 3 commits.');
    expect(syncOutcomeSentence({ kind: 'fastForwarded', behind: 1 })).toBe('Pulled 1 commit.');
    expect(syncOutcomeSentence({ kind: 'fastForwarded', behind: 2 })).toBe('Pulled 2 commits.');
  });
});

describe('the Amend item (S11.1c)', () => {
  it('is hidden once HEAD already reached a real upstream', () => {
    expect(canAmend(branch({ aheadBehind: [0, 0] }))).toBe(false);
    expect(canAmend(branch({ aheadBehind: [0, 3] }))).toBe(false);
  });

  it('is offered whenever HEAD has not reached the upstream, or there is none to reach', () => {
    expect(canAmend(branch({ aheadBehind: [1, 0] }))).toBe(true);
    expect(canAmend(branch({ aheadBehind: [2, 1] }))).toBe(true);
    // No upstream at all: nothing has ever been shared, so amending costs nothing.
    expect(canAmend(branch({ aheadBehind: null }))).toBe(true);
    expect(canAmend(null)).toBe(true);
  });
});

describe("a commit row's age", () => {
  const now = Date.UTC(2026, 8, 29, 12, 0, 0);
  const secondsAgo = (n: number) => (now - n * 1000) / 1000;

  it('reads in the units a person would use', () => {
    expect(relativeTime(secondsAgo(20), now)).toBe('just now');
    expect(relativeTime(secondsAgo(60), now)).toBe('1 minute ago');
    expect(relativeTime(secondsAgo(40 * 60), now)).toBe('40 minutes ago');
    expect(relativeTime(secondsAgo(3 * 3600), now)).toBe('3 hours ago');
    expect(relativeTime(secondsAgo(26 * 3600), now)).toBe('yesterday');
    expect(relativeTime(secondsAgo(4 * 86_400), now)).toBe('4 days ago');
  });

  it('gives up on "ago" past a week, because a date is not arithmetic the reader has to do', () => {
    expect(relativeTime(secondsAgo(60 * 86_400), now)).toBe(new Date(secondsAgo(60 * 86_400) * 1000).toLocaleDateString());
  });

  it('never reads as the future, however skewed the clock that wrote the commit', () => {
    expect(relativeTime(secondsAgo(-3600), now)).toBe('just now');
  });
});

describe('the suggested commit message (S10.3c)', () => {
  function summary(over: Partial<ProseSummary> = {}): ProseSummary {
    return { wordsBefore: 0, wordsAfter: 0, sections: [], paths: [], addedPaths: [], ...over };
  }

  it('names the section and the words, as §6 writes it', () => {
    expect(
      suggestedMessage(summary({ wordsBefore: 1200, wordsAfter: 1440, sections: ['Methods'], paths: ['main.tex'] })),
    ).toBe('Revised Methods, +240 words');
  });

  it('names two sections, and counts them past two', () => {
    expect(suggestedMessage(summary({ sections: ['Methods', 'Results'], paths: ['main.tex'] }))).toBe(
      'Revised Methods and Results',
    );
    expect(suggestedMessage(summary({ sections: ['A', 'B', 'C'], paths: ['main.tex'] }))).toBe('Revised 3 sections');
  });

  it('says a cut is a cut, in ASCII a commit message can carry', () => {
    const cut = summary({ wordsBefore: 900, wordsAfter: 830, sections: ['Discussion'], paths: ['main.tex'] });
    expect(suggestedMessage(cut)).toBe('Revised Discussion, -70 words');
  });

  it('names a new file rather than a section nobody has seen before', () => {
    expect(
      suggestedMessage(summary({ wordsAfter: 40, paths: ['notes.tex'], addedPaths: ['notes.tex'], sections: ['Notes'] })),
    ).toBe('Added notes.tex, +40 words');
  });

  it('falls back to the file when no section could be attributed — a preamble edit, say', () => {
    expect(suggestedMessage(summary({ wordsBefore: 10, wordsAfter: 12, paths: ['preamble.tex'] }))).toBe(
      'Revised preamble.tex, +2 words',
    );
    expect(suggestedMessage(summary({ paths: ['a.tex', 'b.tex'] }))).toBe('Revised 2 files');
  });

  it('suggests nothing when there is nothing true to say, rather than filling the box', () => {
    expect(suggestedMessage(summary())).toBe('');
    expect(suggestedMessage(null)).toBe('');
  });

  it('leaves the count out when the words did not move — a reword is still a revision', () => {
    expect(suggestedMessage(summary({ wordsBefore: 500, wordsAfter: 500, sections: ['Methods'], paths: ['m.tex'] }))).toBe(
      'Revised Methods',
    );
    expect(wordDeltaLabel(0)).toBe('');
  });
});
