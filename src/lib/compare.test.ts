import { describe, expect, it } from 'vitest';
import {
  NO_COMPARISON,
  NO_MARKS,
  describeEnd,
  foldDiffEvent,
  inDiffMode,
  markOf,
  markRow,
  startedComparison,
  type ComparisonView,
} from './compare.svelte';
import type { CompileEvent } from './ipc';

const toUrl = (path: string) => `asset://${path}`;

describe('markRow', () => {
  it('marks the first click as from and starts nothing', () => {
    const step = markRow(NO_MARKS, 'a');
    expect(step.marks).toEqual({ from: 'a', to: null });
    expect(step.compare).toBeNull();
  });

  it('marks the second click as to and asks for the comparison at once', () => {
    const step = markRow({ from: 'a', to: null }, 'b');
    expect(step.marks).toEqual({ from: 'a', to: 'b' });
    expect(step.compare).toEqual(['a', 'b']);
  });

  it('unmarks a lone from when it is clicked again', () => {
    expect(markRow({ from: 'a', to: null }, 'a')).toEqual({ marks: NO_MARKS, compare: null });
  });

  it('starts over on a third click on an unmarked row', () => {
    const step = markRow({ from: 'a', to: 'b' }, 'c');
    expect(step.marks).toEqual({ from: 'c', to: null });
    expect(step.compare).toBeNull();
  });

  it('removes a marked row from a pair and keeps the other as from', () => {
    expect(markRow({ from: 'a', to: 'b' }, 'a').marks).toEqual({ from: 'b', to: null });
    expect(markRow({ from: 'a', to: 'b' }, 'b').marks).toEqual({ from: 'a', to: null });
  });

  it('can be driven with the keyboard exactly as with the mouse: it has no idea which was used', () => {
    // `Enter` and a click both call `markRow(marks, id)`; this is the whole contract.
    let marks = NO_MARKS;
    const asked: Array<[string, string]> = [];
    for (const id of ['x', 'y', 'z', 'x']) {
      const step = markRow(marks, id);
      marks = step.marks;
      if (step.compare) asked.push(step.compare);
    }
    // x, y: a pair. z: starts over. x: completes a second pair with z.
    expect(asked).toEqual([
      ['x', 'y'],
      ['z', 'x'],
    ]);
    expect(marks).toEqual({ from: 'z', to: 'x' });
  });
});

describe('markOf', () => {
  it('names the mark a row carries', () => {
    const marks = { from: 'a', to: 'b' };
    expect(markOf(marks, 'a')).toBe('from');
    expect(markOf(marks, 'b')).toBe('to');
    expect(markOf(marks, 'c')).toBeNull();
  });
});

describe('foldDiffEvent', () => {
  const building: ComparisonView = { ...startedComparison(), older: 'o'.repeat(40), newer: 'n'.repeat(40), generation: 7 };

  const finished = (over: Partial<Extract<CompileEvent, { status: 'finished' }>> = {}): CompileEvent => ({
    status: 'finished',
    generation: 7,
    success: true,
    pdfPath: '/p/.abstract-tex/latexdiff/x/build/main.pdf',
    logPath: null,
    diagnostics: [],
    durationMs: 1200,
    stderr: '',
    ...over,
  });

  it('is diff mode from the moment it is asked for until it is left', () => {
    expect(inDiffMode(NO_COMPARISON)).toBe(false);
    expect(inDiffMode(startedComparison())).toBe(true);
  });

  it('shows the PDF of a finished build of its own generation', () => {
    const view = foldDiffEvent(building, finished(), toUrl);
    expect(view.phase).toBe('ready');
    expect(view.pdfUrl).toBe('asset:///p/.abstract-tex/latexdiff/x/build/main.pdf?v=7');
  });

  it('ignores every event of another generation', () => {
    expect(foldDiffEvent(building, finished({ generation: 6 }), toUrl)).toBe(building);
    expect(foldDiffEvent(building, { status: 'progress', generation: 8, message: 'x' }, toUrl)).toBe(building);
  });

  it('ignores events before the answer has named a generation', () => {
    const waiting = startedComparison();
    expect(foldDiffEvent(waiting, finished(), toUrl)).toBe(waiting);
  });

  it('ignores events once the comparison is not building any more', () => {
    expect(foldDiffEvent(NO_COMPARISON, finished(), toUrl)).toBe(NO_COMPARISON);
  });

  it('carries progress while building', () => {
    const view = foldDiffEvent(building, { status: 'progress', generation: 7, message: 'Downloading…' }, toUrl);
    expect(view.phase).toBe('building');
    expect(view.progress).toBe('Downloading…');
  });

  it('turns a failed build into a sentence and the diff build’s own diagnostics', () => {
    const diagnostic = { title: 'Undefined control sequence', severity: 'error', file: 'main.tex', line: 3 } as never;
    const view = foldDiffEvent(building, finished({ success: false, pdfPath: null, diagnostics: [diagnostic], stderr: 'boom' }), toUrl);
    expect(view.phase).toBe('failed');
    expect(view.message).toMatch(/not in yours/);
    expect(view.diagnostics).toHaveLength(1);
    expect(view.stderr).toBe('boom');
    expect(view.pdfUrl).toBeNull();
  });

  it('says so when TeX failed without a diagnostic', () => {
    const view = foldDiffEvent(building, finished({ success: false, pdfPath: null }), toUrl);
    expect(view.phase).toBe('failed');
    expect(view.message).toMatch(/did not say why/);
  });

  it('turns a build that could not run into its own message', () => {
    const view = foldDiffEvent(building, { status: 'failed', generation: 7, message: 'No engine.' }, toUrl);
    expect(view.phase).toBe('failed');
    expect(view.message).toBe('No engine.');
  });

  it('never lets a draft change a comparison', () => {
    const draft: CompileEvent = { status: 'draft', generation: 7, chapter: 'a', pdfPath: '/x.pdf', durationMs: 1 };
    expect(foldDiffEvent(building, draft, toUrl)).toBe(building);
  });
});

describe('describeEnd', () => {
  it('adds the age when there is one', () => {
    expect(describeEnd('a1b2c3d', '3 days ago')).toBe('a1b2c3d (3 days ago)');
    expect(describeEnd('a1b2c3d', null)).toBe('a1b2c3d');
  });
});
