import { describe, expect, it } from 'vitest';
import { mergeOutline, scanOutline } from './outline';
import type { FlatSymbol } from './editor/symbols';

describe('scanOutline', () => {
  it('returns the five kinds in document order', () => {
    const text = [
      '\\section*{Introduction}',
      'Some text with an escaped 100\\% and no comment here.',
      '\\begin{figure}',
      '\\caption{A nice plot}',
      '\\label{fig:nice}',
      '\\end{figure}',
      '% TODO: fix the caption above',
      '\\subsection{Background}',
    ].join('\n');

    const items = scanOutline(text);

    expect(items).toEqual([
      { kind: 'section', title: 'Introduction', line: 1, level: 2 },
      { kind: 'figure', title: 'A nice plot', line: 3, level: 0 },
      { kind: 'label', title: 'fig:nice', line: 5, level: 0 },
      { kind: 'todo', title: 'TODO: fix the caption above', line: 7, level: 0 },
      { kind: 'section', title: 'Background', line: 8, level: 3 },
    ]);
  });

  it('does not treat an escaped percent sign as a comment', () => {
    const items = scanOutline('Discount: 100\\% off. % TODO: verify this number');
    // The escaped `\%` must not itself start a comment; only the real `%` later does.
    expect(items).toEqual([{ kind: 'todo', title: 'TODO: verify this number', line: 1, level: 0 }]);
  });

  it('balances nested braces in a section title', () => {
    const items = scanOutline('\\section{A \\emph{nested} title}');
    expect(items).toEqual([{ kind: 'section', title: 'A \\emph{nested} title', line: 1, level: 2 }]);
  });

  it('reads every sectioning level and both starred and unstarred forms', () => {
    const text = [
      '\\part{One}',
      '\\chapter{Two}',
      '\\section{Three}',
      '\\subsection{Four}',
      '\\subsubsection{Five}',
      '\\paragraph{Six}',
      '\\section*{Starred}',
    ].join('\n');
    const items = scanOutline(text);
    expect(items.map((i) => i.level)).toEqual([0, 1, 2, 3, 4, 5, 2]);
    expect(items.map((i) => i.title)).toEqual(['One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Starred']);
  });

  it('does not mistake \\partial or \\paragraphindent for a sectioning command', () => {
    // Both are real commands whose names start with a sectioning keyword but continue with more
    // letters (`\part`+`ial`, `\paragraph`+`indent`) — a scan with no word boundary matches the
    // prefix and leaves an empty-titled row behind.
    const items = scanOutline('$\\partial x / \\partial t$ and \\paragraphindent0pt');
    expect(items).toEqual([]);
  });

  it('falls back to the bare kind name for a figure with no caption', () => {
    const items = scanOutline('\\begin{table}\n1 & 2 \\\\\n\\end{table}');
    expect(items).toEqual([{ kind: 'table', title: 'Table', line: 1, level: 0 }]);
  });

  it('handles a starred figure environment', () => {
    const items = scanOutline('\\begin{figure*}\n\\caption{Wide plot}\n\\end{figure*}');
    expect(items).toEqual([{ kind: 'figure', title: 'Wide plot', line: 1, level: 0 }]);
  });

  it('recognises FIXME as well as TODO', () => {
    const items = scanOutline('% FIXME: this citation is wrong');
    expect(items).toEqual([{ kind: 'todo', title: 'FIXME: this citation is wrong', line: 1, level: 0 }]);
  });

  it('ignores a comment that does not start with TODO or FIXME', () => {
    expect(scanOutline('% just a note')).toEqual([]);
  });

  it('returns an empty list for text with none of the five kinds', () => {
    expect(scanOutline('Just prose.\nMore prose.\n')).toEqual([]);
  });
});

describe('mergeOutline', () => {
  const scanned = [
    { kind: 'section' as const, title: 'Intro', line: 1, level: 2 },
    { kind: 'figure' as const, title: 'A plot', line: 5, level: 0 },
    { kind: 'todo' as const, title: 'TODO: revisit', line: 5, level: 0 },
  ];

  it('returns the scan unchanged for an empty symbol list', () => {
    expect(mergeOutline(scanned, [])).toEqual(scanned);
  });

  it('prefers the server symbol on a shared line, but keeps TODOs and unclaimed scan items', () => {
    const symbols: FlatSymbol[] = [{ name: 'Introduction', detail: null, kind: 1, line: 1, depth: 0 }];

    const merged = mergeOutline(scanned, symbols);

    expect(merged).toEqual([
      { kind: 'section', title: 'Introduction', line: 1, level: 0 },
      { kind: 'figure', title: 'A plot', line: 5, level: 0 },
      { kind: 'todo', title: 'TODO: revisit', line: 5, level: 0 },
    ]);
  });
});
