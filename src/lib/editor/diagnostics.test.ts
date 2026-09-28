import { describe, expect, it } from 'vitest';
import { EditorSelection, EditorState } from '@codemirror/state';
import { build, buildLsp, buildSquiggles, diagnosticAt, diagnosticGutter, mergeMarkers, mergedMarkers, setDiagnostics } from './diagnostics';
import type { Diagnostic } from '../ipc';
import type { EditorDiagnostic } from '../lsp-diagnostics';
import type { Position } from '../lsp-protocol';

function diag(line: number | null, severity: Diagnostic['severity'], title = 't'): Diagnostic {
  return { title, explanation: '', line, file: null, severity, rule: null, rawMessage: '', fix: null };
}

/** One row as the language server's side produces them. `startLine` is already 1-based;
 * `lsp-diagnostics.test.ts` is where the 0-to-1 conversion itself is pinned. */
function lspDiag(startLine: number, severity: EditorDiagnostic['severity'], title = 'L'): EditorDiagnostic {
  return {
    severity,
    title,
    startLine,
    from: { line: startLine - 1, character: 0 },
    to: { line: startLine - 1, character: 3 },
    source: 'texlab',
  };
}

/** Pretend every line is 10 characters long. */
const lineStart = (line: number) => (line - 1) * 10;

function lines(set: ReturnType<typeof build>): Array<[number, string, string]> {
  const out: Array<[number, string, string]> = [];
  const cursor = set.iter();
  while (cursor.value) {
    out.push([cursor.from / 10 + 1, cursor.value.severity, cursor.value.title]);
    cursor.next();
  }
  return out;
}

describe('diagnostic gutter markers', () => {
  it('places one dot per line, sorted, and skips diagnostics with no line', () => {
    const set = build([diag(5, 'warning'), diag(null, 'error'), diag(2, 'error')], 10, lineStart);
    expect(lines(set)).toEqual([
      [2, 'error', 't'],
      [5, 'warning', 't'],
    ]);
  });

  it('an error wins over a warning on the same line', () => {
    const set = build([diag(3, 'warning', 'w'), diag(3, 'error', 'e')], 10, lineStart);
    expect(lines(set)).toEqual([[3, 'error', 'e']]);
    const reversed = build([diag(3, 'error', 'e'), diag(3, 'warning', 'w')], 10, lineStart);
    expect(lines(reversed)).toEqual([[3, 'error', 'e']]);
  });

  it('clamps a line TeX claims past the end of the file onto the last line', () => {
    const set = build([diag(99, 'error')], 10, lineStart);
    expect(lines(set)).toEqual([[10, 'error', 't']]);
  });
});

describe('buildLsp', () => {
  /** The off-by-one risk lands here: a marker for CodeMirror line 2 must sit at `lineStart(2)`,
   * not `lineStart(1)`. A two-line document makes a wrong answer visible rather than merely
   * shifted. */
  it('puts a dot at the start of the 1-based line it names', () => {
    const set = buildLsp([lspDiag(2, 'error')], 2, lineStart);
    expect(lines(set)).toEqual([[2, 'error', 'L']]);
  });

  it('places one dot per line, sorted', () => {
    const set = buildLsp([lspDiag(5, 'warning'), lspDiag(2, 'error')], 10, lineStart);
    expect(lines(set)).toEqual([
      [2, 'error', 'L'],
      [5, 'warning', 'L'],
    ]);
  });

  it('an error wins over a warning on the same line, whichever came first', () => {
    expect(lines(buildLsp([lspDiag(3, 'warning', 'w'), lspDiag(3, 'error', 'e')], 10, lineStart))).toEqual([
      [3, 'error', 'e'],
    ]);
    expect(lines(buildLsp([lspDiag(3, 'error', 'e'), lspDiag(3, 'warning', 'w')], 10, lineStart))).toEqual([
      [3, 'error', 'e'],
    ]);
  });

  /** The document can shrink between a publish and the dispatch that draws it. */
  it('clamps a line past the end of a document that has since shrunk', () => {
    expect(lines(buildLsp([lspDiag(99, 'error')], 10, lineStart))).toEqual([[10, 'error', 'L']]);
  });

  it('draws nothing for an empty publish', () => {
    expect(lines(buildLsp([], 10, lineStart))).toEqual([]);
  });
});

describe('mergeMarkers', () => {
  it('keeps both sources when they land on different lines', () => {
    const merged = mergeMarkers(
      build([diag(2, 'error', 'log')], 10, lineStart),
      buildLsp([lspDiag(6, 'warning', 'lsp')], 10, lineStart),
    );
    expect(lines(merged)).toEqual([
      [2, 'error', 'log'],
      [6, 'warning', 'lsp'],
    ]);
  });

  /** DESIGN.md §5.2 in one assertion: on an exact tie the *explained* sentence from the rule
   * catalog wins over TexLab's raw string. */
  it('texlog beats LSP on an exact severity tie', () => {
    const merged = mergeMarkers(
      build([diag(4, 'error', 'log')], 10, lineStart),
      buildLsp([lspDiag(4, 'error', 'lsp')], 10, lineStart),
    );
    expect(lines(merged)).toEqual([[4, 'error', 'log']]);
  });

  it('an error beats a warning whichever source it came from', () => {
    const lspErrorWins = mergeMarkers(
      build([diag(4, 'warning', 'log')], 10, lineStart),
      buildLsp([lspDiag(4, 'error', 'lsp')], 10, lineStart),
    );
    expect(lines(lspErrorWins)).toEqual([[4, 'error', 'lsp']]);

    const logErrorWins = mergeMarkers(
      build([diag(4, 'error', 'log')], 10, lineStart),
      buildLsp([lspDiag(4, 'warning', 'lsp')], 10, lineStart),
    );
    expect(lines(logErrorWins)).toEqual([[4, 'error', 'log']]);
  });

  it('emits positions in order even when the two sources interleave', () => {
    const merged = mergeMarkers(
      build([diag(7, 'error', 'log'), diag(1, 'warning', 'log')], 10, lineStart),
      buildLsp([lspDiag(4, 'warning', 'lsp'), lspDiag(9, 'error', 'lsp')], 10, lineStart),
    );
    expect(lines(merged).map(([line]) => line)).toEqual([1, 4, 7, 9]);
  });

  it('an empty side leaves the other untouched — one source clearing must not clear both', () => {
    const logOnly = mergeMarkers(build([diag(3, 'error', 'log')], 10, lineStart), buildLsp([], 10, lineStart));
    expect(lines(logOnly)).toEqual([[3, 'error', 'log']]);

    const lspOnly = mergeMarkers(build([], 10, lineStart), buildLsp([lspDiag(3, 'warning', 'lsp')], 10, lineStart));
    expect(lines(lspOnly)).toEqual([[3, 'warning', 'lsp']]);

    expect(lines(mergeMarkers(build([], 10, lineStart), buildLsp([], 10, lineStart)))).toEqual([]);
  });
});

/** Pretend every line is 10 characters long, matching the rest of this file's `lineStart`, so a
 * `Position` converts to an offset the same simple way: `line * 10 + character`. */
const toOffset = (position: Position) => position.line * 10 + position.character;

/** A diagnostic whose `from`/`to` sit on one line, matching what `toEditorDiagnostic` actually
 * produces (LSP never spans lines for the cases this app underlines). */
function rangedDiag(
  line: number,
  fromChar: number,
  toChar: number,
  severity: EditorDiagnostic['severity'] = 'error',
  title = 'squiggle',
): EditorDiagnostic {
  return {
    severity,
    title,
    startLine: line + 1,
    from: { line, character: fromChar },
    to: { line, character: toChar },
    source: 'texlab',
  };
}

describe('buildSquiggles', () => {
  it("marks the diagnostic's own range, not its whole line", () => {
    const set = buildSquiggles([rangedDiag(0, 4, 12)], toOffset);
    const cursor = set.iter();
    expect(cursor.from).toBe(4);
    expect(cursor.to).toBe(12);
    cursor.next();
    expect(cursor.value).toBeNull();
  });

  it('widens a zero-width range by one character rather than dropping it', () => {
    const set = buildSquiggles([rangedDiag(0, 5, 5)], toOffset);
    const cursor = set.iter();
    expect(cursor.from).toBe(5);
    expect(cursor.to).toBe(6);
  });

  it('sorts out-of-order diagnostics before building, since RangeSetBuilder requires it', () => {
    const set = buildSquiggles([rangedDiag(2, 0, 3), rangedDiag(0, 0, 3)], toOffset);
    const starts: number[] = [];
    const cursor = set.iter();
    while (cursor.value) {
      starts.push(cursor.from);
      cursor.next();
    }
    expect(starts).toEqual([0, 20]);
  });

  it('carries the severity into the decoration class', () => {
    const set = buildSquiggles([rangedDiag(0, 0, 3, 'warning')], toOffset);
    expect(set.iter().value?.spec.class).toBe('cm-diag-squiggle cm-diag-squiggle-warning');
  });

  it('builds an empty set for no diagnostics', () => {
    expect(buildSquiggles([], toOffset).iter().value).toBeNull();
  });
});

describe('diagnosticAt', () => {
  it('finds the diagnostic whose range covers the position', () => {
    const set = buildSquiggles([rangedDiag(0, 4, 12, 'error', 'undefined control sequence')], toOffset);
    expect(diagnosticAt(set, 6)?.title).toBe('undefined control sequence');
  });

  it('returns null just outside the range', () => {
    const set = buildSquiggles([rangedDiag(0, 4, 12)], toOffset);
    expect(diagnosticAt(set, 20)).toBeNull();
  });

  it('returns null when there are no squiggles at all', () => {
    expect(diagnosticAt(buildSquiggles([], toOffset), 0)).toBeNull();
  });

  it('picks one diagnostic when two ranges overlap, rather than throwing', () => {
    const set = buildSquiggles([rangedDiag(0, 0, 10, 'warning', 'a'), rangedDiag(0, 4, 6, 'error', 'b')], toOffset);
    expect(diagnosticAt(set, 5)).not.toBeNull();
  });
});

describe('mergedMarkers (S9.6)', () => {
  const doc = 'line one\nline two\nline three\nline four';
  const error = (line: number): Diagnostic => ({
    title: 'Error',
    explanation: 'An error.',
    line,
    file: 'main.tex',
    severity: 'error',
    rule: null,
    rawMessage: 'x',
    fix: null,
  });

  function withDiagnostics(lines: number[]): EditorState {
    const state = EditorState.create({ doc, extensions: diagnosticGutter() });
    return state.update({ effects: setDiagnostics.of(lines.map(error)) }).state;
  }

  function markerLines(state: EditorState): number[] {
    const lines: number[] = [];
    for (const cursor = state.field(mergedMarkers).iter(); cursor.value; cursor.next()) {
      lines.push(state.doc.lineAt(cursor.from).number);
    }
    return lines;
  }

  it('merges when a source is replaced', () => {
    expect(markerLines(withDiagnostics([2, 4]))).toEqual([2, 4]);
  });

  it('returns the very same set when only the cursor moves', () => {
    const state = withDiagnostics([2]);
    const moved = state.update({ selection: EditorSelection.cursor(5) }).state;
    expect(moved.field(mergedMarkers)).toBe(state.field(mergedMarkers));
  });

  it('shifts with the text when a line is typed above a marker', () => {
    const state = withDiagnostics([2]);
    const typed = state.update({ changes: { from: 0, insert: 'new first line\n' } }).state;
    expect(markerLines(typed)).toEqual([3]);
  });

  it('keeps one marker per line when a deleted line break joins two marked lines', () => {
    const state = withDiagnostics([2, 3]);
    const lineTwoEnd = state.doc.line(2).to;
    const joined = state.update({ changes: { from: lineTwoEnd, to: lineTwoEnd + 1 } }).state;
    expect(markerLines(joined)).toEqual([2]);
  });
});
