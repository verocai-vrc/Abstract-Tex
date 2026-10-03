import { describe, expect, it } from 'vitest';
import {
  LspDiagnosticStore,
  severityOf,
  titleOf,
  toEditorDiagnostic,
  type EditorDiagnostic,
} from './lsp-diagnostics';
import { normalizeUri, pathToUri } from './lsp';
import type { LspDiagnostic, LspSeverity, PublishDiagnosticsParams } from './lsp-protocol';

/** One wire diagnostic. `line` is 0-based, the way the server sends it. */
function wire(line: number, severity?: LspSeverity, message = 'Undefined reference.'): LspDiagnostic {
  return {
    range: { start: { line, character: 4 }, end: { line, character: 12 } },
    severity,
    message,
    source: 'texlab',
  };
}

function params(uri: string, diagnostics: LspDiagnostic[]): PublishDiagnosticsParams {
  return { uri, diagnostics };
}

describe('severityOf', () => {
  it('maps 1 to error and 2 to warning', () => {
    expect(severityOf(1)).toBe('error');
    expect(severityOf(2)).toBe('warning');
  });

  it('drops Information and Hint, which have no visual yet', () => {
    expect(severityOf(3)).toBeNull();
    expect(severityOf(4)).toBeNull();
  });

  /** The spec leaves an absent severity to the client. A nameless problem gets the quieter
   * colour: an unexplained red dot is the thing DESIGN.md §6 calls shouting. */
  it('treats an absent severity as a warning, not an error', () => {
    expect(severityOf(undefined)).toBe('warning');
  });
});

describe('titleOf', () => {
  it('keeps a one-sentence message as it is', () => {
    expect(titleOf('Undefined reference.')).toBe('Undefined reference.');
  });

  it('takes only the first sentence of a longer message', () => {
    expect(titleOf('Undefined reference. Did you mean \\ref{fig:one}?')).toBe('Undefined reference.');
  });

  it('does not cut at a full stop inside a label', () => {
    expect(titleOf('\\ref{fig.1} is undefined')).toBe('\\ref{fig.1} is undefined');
  });

  it('takes only the first line of a multi-line message', () => {
    expect(titleOf('Command terminated\nwith an error')).toBe('Command terminated');
  });

  it('caps a long sentence rather than letting a paragraph hover over the text', () => {
    const long = `${'x'.repeat(400)}.`;
    const title = titleOf(long);
    expect(title.length).toBeLessThanOrEqual(120);
    expect(title.endsWith('…')).toBe(true);
  });

  it('survives a message that is not a string', () => {
    expect(titleOf(undefined)).toBe('');
    expect(titleOf(42)).toBe('');
  });
});

describe('toEditorDiagnostic', () => {
  /** The risk the architect flagged: LSP counts lines from 0, CodeMirror from 1. */
  it('turns a 0-based LSP line into a 1-based CodeMirror line', () => {
    expect(toEditorDiagnostic(wire(0, 1))?.startLine).toBe(1);
    expect(toEditorDiagnostic(wire(41, 1))?.startLine).toBe(42);
  });

  it('keeps the original 0-based range for S3.3d and the source string', () => {
    const row = toEditorDiagnostic(wire(7, 2))!;
    expect(row.from).toEqual({ line: 7, character: 4 });
    expect(row.to).toEqual({ line: 7, character: 12 });
    expect(row.source).toBe('texlab');
  });

  it('drops a Hint rather than inventing a colour for it', () => {
    expect(toEditorDiagnostic(wire(3, 4))).toBeNull();
  });

  it('drops a diagnostic with no usable range instead of throwing', () => {
    expect(toEditorDiagnostic({ message: 'no range' } as unknown as LspDiagnostic)).toBeNull();
  });
});

describe('normalizeUri', () => {
  it('folds the drive letter to lower case and the colon to a plain one', () => {
    expect(normalizeUri('file:///C:/Proj/main.tex')).toBe('file:///c:/Proj/main.tex');
    expect(normalizeUri('file:///c%3A/Proj/main.tex')).toBe('file:///c:/Proj/main.tex');
    expect(normalizeUri('file:///C%3a/Proj/main.tex')).toBe('file:///c:/Proj/main.tex');
  });

  it('leaves the rest of the path, and any other system’s paths, alone', () => {
    expect(normalizeUri('file:///home/Ada/Main.tex')).toBe('file:///home/Ada/Main.tex');
    expect(normalizeUri('file:///C:/Proj/C:/x')).toBe('file:///c:/Proj/C:/x');
  });
});

describe('LspDiagnosticStore', () => {
  const main = '/proj/main.tex';
  const included = '/proj/sections/results.tex';

  it('stores a publish and returns it for that path', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(4, 1)]));

    const rows = store.forPath(main);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.startLine).toBe(5);
    expect(rows[0]?.severity).toBe('error');
  });

  /** The ledger's third Windows-path bug: the picker says `c:\Proj`, the server publishes for
   * `C:/Proj`, and the lookup used to miss without a word. */
  it('finds a publish whatever case the drive letter was spelt in', () => {
    const store = new LspDiagnosticStore();
    store.publish(params('file:///C:/Proj/main.tex', [wire(4, 1)]));
    expect(store.forPath('c:\\Proj\\main.tex')).toHaveLength(1);
    expect(store.forPath('C:\\Proj\\main.tex')).toHaveLength(1);

    store.publish(params('file:///c%3A/Proj/main.tex', []));
    expect(store.forPath('C:\\Proj\\main.tex')).toEqual([]);
  });

  /** The case that rots silently: a second publish saying the file is clean must remove the
   * dots, not be skipped as "nothing to do". */
  it('a publish with an empty array clears that file', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(4, 1)]));
    expect(store.forPath(main)).toHaveLength(1);

    store.publish(params(pathToUri(main), []));
    expect(store.forPath(main)).toEqual([]);
    expect(store.uris()).toEqual([]);
  });

  it('a publish replaces the file wholesale rather than accumulating', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(4, 1), wire(9, 2)]));
    store.publish(params(pathToUri(main), [wire(20, 2)]));

    expect(store.forPath(main).map((r) => r.startLine)).toEqual([21]);
  });

  /** TexLab publishes for `\\input` files nobody has opened; storage is cheap and S4.x wants it. */
  it('stores a publish for a file with no open tab, without disturbing another file', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(0, 1)]));
    store.publish(params(pathToUri(included), [wire(2, 2)]));

    expect(store.forPath(included).map((r) => r.startLine)).toEqual([3]);
    expect(store.forPath(main).map((r) => r.startLine)).toEqual([1]);
  });

  it('returns an empty array for a file the server has said nothing about', () => {
    const store = new LspDiagnosticStore();
    expect(store.forPath('/proj/never-mentioned.tex')).toEqual([]);
  });

  /** `pathToUri` must be the one URI producer, or a hand-spelled key would never be found
   * again. This pins the round trip rather than the spelling. */
  it('looks up through pathToUri, including a path with a space in it', () => {
    const store = new LspDiagnosticStore();
    const spaced = '/My Thesis/main.tex';
    store.publish(params(pathToUri(spaced), [wire(1, 1)]));

    expect(store.uris()[0]).toBe('file:///My%20Thesis/main.tex');
    expect(store.forPath(spaced)).toHaveLength(1);
  });

  it('drops the rows a severity filter removes, and clears the file when none survive', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(1, 3), wire(2, 4)]));
    expect(store.forPath(main)).toEqual([]);
    expect(store.uris()).toEqual([]);
  });

  it('clear() empties every file', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(4, 1)]));
    store.publish(params(pathToUri(included), [wire(4, 2)]));

    store.clear();
    expect(store.uris()).toEqual([]);
    expect(store.forPath(main)).toEqual([]);
    expect(store.forPath(included)).toEqual([]);
  });

  it('bumps version on every publish, and on a clear only when there was something to clear', () => {
    const store = new LspDiagnosticStore();
    expect(store.version).toBe(0);

    store.publish(params(pathToUri(main), [wire(4, 1)]));
    expect(store.version).toBe(1);
    // Even the "now clean" publish bumps: the gutter has to be told to remove the dot.
    store.publish(params(pathToUri(main), []));
    expect(store.version).toBe(2);

    store.clear();
    expect(store.version).toBe(2); // nothing was there
    store.publish(params(pathToUri(main), [wire(4, 1)]));
    store.clear();
    expect(store.version).toBe(4);
  });

  it('is typed as EditorDiagnostic, the shape the gutter consumes', () => {
    const store = new LspDiagnosticStore();
    store.publish(params(pathToUri(main), [wire(0, 1, 'Unknown environment.')]));
    const row: EditorDiagnostic = store.forPath(main)[0]!;
    expect(row).toEqual({
      severity: 'error',
      title: 'Unknown environment.',
      startLine: 1,
      from: { line: 0, character: 4 },
      to: { line: 0, character: 12 },
      source: 'texlab',
    });
  });
});
