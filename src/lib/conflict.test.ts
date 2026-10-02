// S11.2b: turning real conflict markers into clean runs and hunks, and back.

import { describe, expect, it } from 'vitest';
import { hunkCount, parseConflictMarkers, reassembleConflictSections } from './conflict';

/** A two-paragraph document with one conflict in the middle, exactly as a real `git merge` would
 * leave it — the shape `abstract-tex-git`'s own `against_real_git` fixtures produce. */
const ONE_HUNK = ['Before the conflict.', '<<<<<<< HEAD', 'My version of the sentence.', '=======', "Their version of the sentence.", '>>>>>>> origin/main', 'After the conflict.'].join('\n');

describe('parsing a real conflict (S11.2b)', () => {
  it('splits clean runs and the hunk, in order', () => {
    const sections = parseConflictMarkers(ONE_HUNK);
    expect(sections).toEqual([
      { kind: 'clean', text: 'Before the conflict.' },
      { kind: 'conflict', ours: 'My version of the sentence.', theirs: 'Their version of the sentence.' },
      { kind: 'clean', text: 'After the conflict.' },
    ]);
  });

  it('counts the hunks a parse found', () => {
    expect(hunkCount(parseConflictMarkers(ONE_HUNK)!)).toBe(1);
    expect(hunkCount([{ kind: 'clean', text: 'nothing to resolve' }])).toBe(0);
  });

  it('finds every hunk in a file with more than one', () => {
    const text = [
      '<<<<<<< HEAD',
      'mine one',
      '=======',
      'theirs one',
      '>>>>>>> origin/main',
      'between the two',
      '<<<<<<< HEAD',
      'mine two',
      '=======',
      'theirs two',
      '>>>>>>> origin/main',
    ].join('\n');

    const sections = parseConflictMarkers(text)!;

    expect(hunkCount(sections)).toBe(2);
    expect(sections[0]).toEqual({ kind: 'conflict', ours: 'mine one', theirs: 'theirs one' });
    expect(sections[1]).toEqual({ kind: 'clean', text: 'between the two' });
    expect(sections[2]).toEqual({ kind: 'conflict', ours: 'mine two', theirs: 'theirs two' });
  });

  it('a hunk can span more than one line on each side', () => {
    const text = ['<<<<<<< HEAD', 'mine, line one', 'mine, line two', '=======', 'theirs, only line', '>>>>>>> origin/main'].join('\n');

    const sections = parseConflictMarkers(text)!;

    expect(sections).toEqual([{ kind: 'conflict', ours: 'mine, line one\nmine, line two', theirs: 'theirs, only line' }]);
  });

  it('a file with no markers at all is one clean section, trailing newline included', () => {
    // `split('\n')` turns a trailing newline into a trailing empty element, which is exactly
    // what makes `reassembleConflictSections` below round-trip it rather than lose it.
    expect(parseConflictMarkers('nothing here was conflicted\n')).toEqual([{ kind: 'clean', text: 'nothing here was conflicted\n' }]);
  });

  it('refuses rather than guesses when a marker has no match', () => {
    expect(parseConflictMarkers(['<<<<<<< HEAD', 'mine', '======='].join('\n'))).toBeNull();
    expect(parseConflictMarkers(['<<<<<<< HEAD', 'mine', 'no closing marker at all'].join('\n'))).toBeNull();
  });
});

describe('reassembling a resolution (S11.2b)', () => {
  it('round-trips a kept side exactly', () => {
    const sections = parseConflictMarkers(ONE_HUNK)!;

    const keptMine = reassembleConflictSections(sections, ['My version of the sentence.']);
    expect(keptMine).toBe(['Before the conflict.', 'My version of the sentence.', 'After the conflict.'].join('\n'));

    const keptTheirs = reassembleConflictSections(sections, ['Their version of the sentence.']);
    expect(keptTheirs).toBe(['Before the conflict.', 'Their version of the sentence.', 'After the conflict.'].join('\n'));
  });

  it('a hand-edited combination is just another string', () => {
    const sections = parseConflictMarkers(ONE_HUNK)!;

    const edited = reassembleConflictSections(sections, ['A blend of both sentences.']);

    expect(edited).toBe(['Before the conflict.', 'A blend of both sentences.', 'After the conflict.'].join('\n'));
  });

  it('nothing is not a blank line: an empty resolution leaves no trace of the hunk', () => {
    const sections = parseConflictMarkers(ONE_HUNK)!;

    const deleted = reassembleConflictSections(sections, ['']);

    expect(deleted).toBe(['Before the conflict.', 'After the conflict.'].join('\n'));
  });

  it('resolves each hunk with its own answer, in order', () => {
    const text = ['<<<<<<< HEAD', 'mine one', '=======', 'theirs one', '>>>>>>> origin/main', 'between', '<<<<<<< HEAD', 'mine two', '=======', 'theirs two', '>>>>>>> origin/main'].join(
      '\n',
    );
    const sections = parseConflictMarkers(text)!;

    const result = reassembleConflictSections(sections, ['mine one', 'theirs two']);

    expect(result).toBe(['mine one', 'between', 'theirs two'].join('\n'));
  });

  it('a multi-line resolution keeps its own internal line breaks', () => {
    const sections = parseConflictMarkers(ONE_HUNK)!;

    const result = reassembleConflictSections(sections, ['Line one of the rewrite.\nLine two of the rewrite.']);

    expect(result).toBe(['Before the conflict.', 'Line one of the rewrite.', 'Line two of the rewrite.', 'After the conflict.'].join('\n'));
  });
});
