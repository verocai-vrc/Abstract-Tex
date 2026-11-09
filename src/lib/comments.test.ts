import { describe, expect, it } from 'vitest';
import type { CommentRow } from './ipc';
import { groupComments, lineAt, locateInBuffer, problemSentence, quotePreview } from './comments';

function row(over: Partial<CommentRow> = {}): CommentRow {
  return {
    id: 'a',
    file: 'main.tex',
    author: 'ada',
    body: 'says who?',
    quote: 'result',
    created: 1,
    resolved: false,
    state: 'anchored',
    from: 4,
    to: 10,
    ...over,
  };
}

describe('locateInBuffer', () => {
  it('trusts the offsets while the buffer still holds the quote there', () => {
    expect(locateInBuffer('the result stands', row())).toEqual({ from: 4, to: 10 });
  });
  it('finds the quote again after unsaved edits moved it', () => {
    expect(locateInBuffer('A new line.\nthe result stands', row())).toEqual({ from: 16, to: 22 });
  });
  it('gives up rather than guess when the quote is gone or now repeated', () => {
    expect(locateInBuffer('nothing here', row())).toBeNull();
    expect(locateInBuffer('result and result', row({ from: 99, to: 105 }))).toBeNull();
  });
  it('has nowhere to look for an orphaned comment with no offsets', () => {
    expect(locateInBuffer('the result', row({ state: 'orphaned', from: null, to: null }))).toEqual({
      from: 4,
      to: 10,
    });
  });
});

describe('lineAt', () => {
  it('counts lines from one', () => {
    expect(lineAt('a\nb\nc', 0)).toBe(1);
    expect(lineAt('a\nb\nc', 2)).toBe(2);
    expect(lineAt('a\nb\nc', 99)).toBe(3);
  });
});

describe('groupComments', () => {
  it('puts the active file first among open comments and resolved ones apart', () => {
    const rows = [
      row({ id: '1', file: 'b.tex' }),
      row({ id: '2', file: 'main.tex' }),
      row({ id: '3', resolved: true }),
    ];
    const groups = groupComments(rows, 'main.tex');
    expect(groups.open.map((r) => r.id)).toEqual(['2', '1']);
    expect(groups.resolved.map((r) => r.id)).toEqual(['3']);
  });
});

describe('problemSentence', () => {
  it('says nothing for an anchored comment and something readable otherwise', () => {
    expect(problemSentence(row())).toBeNull();
    expect(problemSentence(row({ state: 'orphaned' }))).toMatch(/has changed/);
    expect(problemSentence(row({ state: 'fileMissing', file: 'old.tex' }))).toBe('old.tex is no longer in the project.');
  });
});

describe('quotePreview', () => {
  it('flattens whitespace and cuts long quotes', () => {
    expect(quotePreview('a\n  b')).toBe('a b');
    expect(quotePreview('x'.repeat(100), 10)).toBe('xxxxxxxxx…');
  });
});
