import { readFileSync } from 'node:fs';
import path from 'node:path';
import { EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import { identify, pasteCiteHandler, pasteTracking, trackPaste, type Identified } from './paste';

// The cases `texbib::acquire::identify` is tested against, read from the same file. The editor
// swallows a paste when *this* copy says it is an identifier, and then asks Rust, which re-runs
// the real `identify`; a paste this copy accepts and Rust refuses is lost. So the two must agree
// on every row, and a row added for one is checked against the other (review finding R8.18).
const sharedCases = (
  JSON.parse(readFileSync(path.resolve('crates/texbib/tests/identify-cases.json'), 'utf8')) as {
    cases: { paste: string; identified: Identified | null }[];
  }
).cases;

describe('identify, on the cases it shares with texbib', () => {
  it('has the table', () => {
    expect(sharedCases.length).toBeGreaterThan(20);
  });

  for (const { paste, identified } of sharedCases) {
    it(`answers ${JSON.stringify(paste)} as texbib does`, () => {
      expect(identify(paste)).toEqual(identified);
    });
  }
});

describe('identify', () => {
  it('recognises a doi.org URL, normalized to the bare DOI', () => {
    expect(identify('https://doi.org/10.1109/tcbb.2019.000001')).toEqual({ kind: 'doi', normalized: '10.1109/tcbb.2019.000001' });
  });

  it('recognises a bare DOI', () => {
    expect(identify('10.1371/journal.pcbi.1000387')).toEqual({ kind: 'doi', normalized: '10.1371/journal.pcbi.1000387' });
  });

  it('recognises a new-style arXiv id with the arXiv: scheme', () => {
    expect(identify('arXiv:1706.03762')).toEqual({ kind: 'arxiv', normalized: '1706.03762' });
  });

  it('keeps a version suffix on a new-style arXiv id', () => {
    expect(identify('1706.03762v7')).toEqual({ kind: 'arxiv', normalized: '1706.03762v7' });
  });

  it('recognises an old-style arXiv id', () => {
    expect(identify('hep-th/9901001')).toEqual({ kind: 'arxiv', normalized: 'hep-th/9901001' });
  });

  it('recognises an arXiv abs URL', () => {
    expect(identify('https://arxiv.org/abs/2101.00001')).toEqual({ kind: 'arxiv', normalized: '2101.00001' });
  });

  it('recognises a 13-digit ISBN with hyphens', () => {
    expect(identify('978-0-262-03384-8')).toEqual({ kind: 'isbn', normalized: '9780262033848' });
  });

  it('recognises a 10-digit ISBN with an X check digit', () => {
    expect(identify('0-8044-2957-x')).toEqual({ kind: 'isbn', normalized: '080442957X' });
  });

  it('identifies a plain sentence as nothing', () => {
    expect(identify('A Probabilistic Model of DNA Folding')).toBeNull();
  });

  it('identifies an empty or whitespace paste as nothing', () => {
    expect(identify('')).toBeNull();
    expect(identify('   ')).toBeNull();
  });

  it('identifies a run of the wrong digit count as nothing', () => {
    expect(identify('12345')).toBeNull();
    expect(identify('123456789012345')).toBeNull();
  });
});

describe('pasteCiteHandler', () => {
  function clipboardEvent(text: string): ClipboardEvent {
    return {
      clipboardData: { getData: (type: string) => (type === 'text/plain' ? text : '') },
      preventDefault: vi.fn(),
    } as unknown as ClipboardEvent;
  }

  function viewWithSelection(from: number, to: number) {
    return { state: { selection: { main: { from, to } } } } as unknown as never;
  }

  it('intercepts a recognised paste and calls the requester with the captured selection', () => {
    const request = vi.fn().mockResolvedValue(undefined);
    const handler = pasteCiteHandler(request);
    const event = clipboardEvent('10.1109/tcbb.2019.000001');
    const view = viewWithSelection(5, 9);
    const handled = handler(event, view);
    expect(handled).toBe(true);
    expect(event.preventDefault).toHaveBeenCalled();
    expect(request).toHaveBeenCalledWith('10.1109/tcbb.2019.000001', view, 5, 9);
  });

  it('leaves an ordinary paste alone', () => {
    const request = vi.fn();
    const handler = pasteCiteHandler(request);
    const event = clipboardEvent('some prose the author is pasting in');
    const handled = handler(event, viewWithSelection(0, 0));
    expect(handled).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(request).not.toHaveBeenCalled();
  });
});

// Review finding R6.19. `pasteCite` fetched for as long as the network took and then dispatched
// `{ from, to }` as they were at paste time, on the comment's belief that CodeMirror maps a position
// captured earlier. It does not: positions in a `dispatch` are in the document as it is *now*, so
// anything typed before the paste point in the meantime moved the cite onto other text.
describe('keeping the place of a paste while its answer is awaited (R6.19)', () => {
  /** A view reduced to what `trackPaste` uses, with CodeMirror's real transaction semantics. */
  function viewOf(doc: string, from: number, to: number, tracked = true) {
    const view = {
      state: EditorState.create({ doc, selection: { anchor: from, head: to }, extensions: tracked ? [pasteTracking] : [] }),
      dispatch(spec: Parameters<EditorState['update']>[0]) {
        view.state = view.state.update(spec).state;
      },
    };
    return view as unknown as EditorView & { state: EditorState };
  }
  const type = (view: EditorView, at: number, text: string) => view.dispatch({ changes: { from: at, insert: text } });

  it('follows the paste point through text typed before it', () => {
    const view = viewOf('see 10.1000/x and more', 4, 12);
    const place = trackPaste(view, 4, 12);
    type(view, 0, 'ABC');
    expect(place.range()).toEqual({ from: 7, to: 15 });
    expect(view.state.sliceDoc(7, 15)).toBe('10.1000/');
  });

  it('leaves it alone for text typed after it', () => {
    const view = viewOf('see 10.1000/x and more', 4, 12);
    const place = trackPaste(view, 4, 12);
    type(view, 20, 'ZZZ');
    expect(place.range()).toEqual({ from: 4, to: 12 });
  });

  it('puts the cite after text typed at the very spot of an empty paste point', () => {
    const view = viewOf('ab', 1, 1);
    const place = trackPaste(view, 1, 1);
    type(view, 1, 'x');
    expect(place.range()).toEqual({ from: 2, to: 2 });
  });

  it('does not widen a selection to take in text typed at its edges', () => {
    const view = viewOf('0123456789', 3, 6);
    const place = trackPaste(view, 3, 6);
    type(view, 3, 'a');
    type(view, 7, 'b'); // the old position 6, shifted by the first insertion
    expect(place.range()).toEqual({ from: 4, to: 7 });
    expect(view.state.sliceDoc(4, 7)).toBe('345');
  });

  it('collapses a range whose text was deleted instead of inverting it', () => {
    const view = viewOf('0123456789', 3, 6);
    const place = trackPaste(view, 3, 6);
    view.dispatch({ changes: { from: 2, to: 8 } });
    const { from, to } = place.range();
    expect(from).toBe(to);
    expect(from).toBeLessThanOrEqual(view.state.doc.length);
  });

  it('keeps each paste separate, and forgets one once it is released', () => {
    const view = viewOf('0123456789', 2, 2);
    const first = trackPaste(view, 2, 2);
    const second = trackPaste(view, 8, 8);
    type(view, 0, 'AB');
    expect(first.range()).toEqual({ from: 4, to: 4 });
    expect(second.range()).toEqual({ from: 10, to: 10 });
    first.release();
    expect(view.state.field(pasteTracking).size).toBe(1);
  });

  it('answers the range it was given when the editor has no tracker installed', () => {
    const view = viewOf('0123456789', 2, 5, false);
    const place = trackPaste(view, 2, 5);
    type(view, 0, 'AB');
    expect(place.range()).toEqual({ from: 2, to: 5 });
    place.release(); // does nothing, and does not throw
  });
});
