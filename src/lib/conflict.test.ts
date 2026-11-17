// S11.2b: turning real conflict markers into clean runs and hunks, and back.

import { describe, expect, it } from 'vitest';
import { hunkCount, parseConflictMarkers, readConflictFile, reassembleConflictSections, writeResolvedFile } from './conflict';

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

// Review finding R6.10. The file is whatever Git left, and the app lists conflicts made by a terminal's
// `git merge` too, so it has to read the shapes Git's own settings produce.
describe('reading the shapes Git leaves behind (R6.10)', () => {
  const diff3 = (style: string) =>
    ['Before.', '<<<<<<< HEAD', 'My sentence.', `${style} merged common ancestors`, 'The original sentence.', '=======', 'Their sentence.', '>>>>>>> origin/main', 'After.'].join('\n');

  it('leaves the common ancestor out of "yours" when merge.conflictStyle is diff3', () => {
    expect(parseConflictMarkers(diff3('|||||||'))).toEqual([
      { kind: 'clean', text: 'Before.' },
      { kind: 'conflict', ours: 'My sentence.', theirs: 'Their sentence.' },
      { kind: 'clean', text: 'After.' },
    ]);
  });

  it('a longer line of equals signs inside a side is text, not the separator', () => {
    const text = ['<<<<<<< HEAD', 'A heading', '=============', 'its body', '=======', 'Theirs', '>>>>>>> origin/main'].join('\n');
    expect(parseConflictMarkers(text)).toEqual([
      { kind: 'conflict', ours: 'A heading\n=============\nits body', theirs: 'Theirs' },
    ]);
  });

  it('reads a file with Windows line endings, and keeps its lines as they were', () => {
    const text = ['Before.', '<<<<<<< HEAD', 'Mine.', '=======', 'Theirs.', '>>>>>>> origin/main', 'After.'].join('\r\n');
    const sections = parseConflictMarkers(text)!;
    expect(hunkCount(sections)).toBe(1);
    expect(sections[1]).toEqual({ kind: 'conflict', ours: 'Mine.\r', theirs: 'Theirs.\r' });
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

// R8.3. The resolver worked on the file's own text. A file with Windows line endings reached its
// textareas with a `\r` on the end of every hunk line; a textarea shows a lone `\r` as a line break and
// reports every line break as `\n`, so editing a hunk put a blank line (a paragraph break) after it and a
// bare LF in the file. A byte-order mark before a first-line hunk hid the opening marker altogether.
describe('a conflicted file with its own line endings and byte-order mark (R8.3)', () => {
  const mark = '\uFEFF';
  /** What `git merge` leaves in a CRLF file: the markers end in CRLF as well. */
  const crlfFile = [
    '\\documentclass{article}',
    '\\begin{document}',
    'alpha',
    '<<<<<<< HEAD',
    'beta from main',
    '=======',
    'beta from other',
    '>>>>>>> other',
    'gamma',
    '\\end{document}',
    '',
  ].join('\r\n');
  const resolvedWith = (beta: string, ending: string) =>
    ['\\documentclass{article}', '\\begin{document}', 'alpha', beta, 'gamma', '\\end{document}', ''].join(ending);

  it('is read as plain text: no carriage return reaches a hunk or a clean run', () => {
    const file = readConflictFile(crlfFile)!;
    expect(file.sections).toEqual([
      { kind: 'clean', text: '\\documentclass{article}\n\\begin{document}\nalpha' },
      { kind: 'conflict', ours: 'beta from main', theirs: 'beta from other' },
      { kind: 'clean', text: 'gamma\n\\end{document}\n' },
    ]);
  });

  it('writes a hand-edited hunk back with the file\'s own endings and no extra line', () => {
    const file = readConflictFile(crlfFile)!;
    const written = writeResolvedFile(file, ['beta from main EDITED']);
    expect(written).toBe(resolvedWith('beta from main EDITED', '\r\n'));
    expect(written).not.toMatch(/(?<!\r)\n/); // not one bare LF
  });

  it('writes a hunk kept as it was read back exactly as Git would have left it', () => {
    const file = readConflictFile(crlfFile)!;
    const theirs = file.sections.find((section) => section.kind === 'conflict')!;
    expect(writeResolvedFile(file, [theirs.kind === 'conflict' ? theirs.theirs : ''])).toBe(
      resolvedWith('beta from other', '\r\n'),
    );
  });

  it('keeps an LF file LF', () => {
    const file = readConflictFile(crlfFile.replace(/\r\n/g, '\n'))!;
    expect(writeResolvedFile(file, ['beta mine'])).toBe(resolvedWith('beta mine', '\n'));
  });

  it('reads a conflict whose first hunk is the first line of a file with a mark, and puts the mark first again', () => {
    const text = `${mark}${['<<<<<<< HEAD', 'mine', '=======', 'theirs', '>>>>>>> other', 'tail', ''].join('\n')}`;
    const file = readConflictFile(text);
    expect(file).not.toBeNull();
    expect(writeResolvedFile(file!, ['mine, edited'])).toBe(`${mark}mine, edited\ntail\n`);
  });

  it('keeps a mark and CRLF endings together', () => {
    const file = readConflictFile(`${mark}${crlfFile}`)!;
    expect(writeResolvedFile(file, ['beta mine'])).toBe(`${mark}${resolvedWith('beta mine', '\r\n')}`);
  });

  it('says nothing can be read, as the parser does, when the markers do not nest', () => {
    expect(readConflictFile('<<<<<<< HEAD\nmine\n')).toBeNull();
  });
});
