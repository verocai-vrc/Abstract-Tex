// Gutter markers for the last build's diagnostics (S2.7): a dot beside each line the drawer
// knows about, coloured by severity, with the title as its tooltip. The drawer is where the
// explanation lives; the gutter only says "here".
//
// CodeMirror state is immutable, so "the current diagnostics" is a `StateField` whose value
// is replaced by dispatching a `StateEffect` — the same pattern as its own lint gutter. The
// field maps its ranges through every edit, so a dot stays on its line while the author types
// above it, until the next build replaces the set.

import { RangeSet, StateEffect, StateField } from '@codemirror/state';
import { EditorView, gutter, GutterMarker } from '@codemirror/view';
import type { Diagnostic } from '../ipc';

class DiagnosticMarker extends GutterMarker {
  constructor(
    readonly severity: Diagnostic['severity'],
    readonly title: string,
  ) {
    super();
  }

  // CodeMirror redraws a gutter element only when `eq` says its marker changed.
  eq(other: DiagnosticMarker): boolean {
    return other.severity === this.severity && other.title === this.title;
  }

  toDOM(): Node {
    const dot = document.createElement('span');
    dot.className = `cm-diag cm-diag-${this.severity}`;
    dot.title = this.title;
    dot.textContent = '●';
    return dot;
  }
}

/** Replace the marker set. Dispatch with `applyDiagnostics`, not by hand. */
const setDiagnostics = StateEffect.define<Diagnostic[]>();

const markers = StateField.define<RangeSet<DiagnosticMarker>>({
  create: () => RangeSet.empty,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setDiagnostics)) return build(effect.value, transaction.state.doc.lines, (n) => transaction.state.doc.line(n).from);
    }
    return value.map(transaction.changes);
  },
});

/** One marker per line, keeping the error if a line has both an error and a warning. Exported
 * for its test; `lineStart` is injected so the test needs no document. */
export function build(diagnostics: Diagnostic[], lineCount: number, lineStart: (line: number) => number): RangeSet<DiagnosticMarker> {
  const byLine = new Map<number, Diagnostic>();
  for (const diagnostic of diagnostics) {
    if (diagnostic.line === null) continue;
    // TeX's line claim can run past the end of the file, e.g. for an error at `\end{document}`.
    const line = Math.max(1, Math.min(diagnostic.line, lineCount));
    const existing = byLine.get(line);
    if (!existing || (existing.severity === 'warning' && diagnostic.severity === 'error')) byLine.set(line, diagnostic);
  }
  const ranges = [...byLine].map(([line, d]) => new DiagnosticMarker(d.severity, d.title).range(lineStart(line)));
  return RangeSet.of(ranges, true);
}

const theme = EditorView.baseTheme({
  '.cm-diag-gutter .cm-gutterElement': { padding: '0 2px 0 4px', fontSize: '9px' },
  '.cm-diag-error': { color: 'var(--error)' },
  '.cm-diag-warning': { color: 'var(--warn)' },
});

/** The extension to add to an editor. */
export function diagnosticGutter() {
  return [markers, gutter({ class: 'cm-diag-gutter', markers: (view) => view.state.field(markers) }), theme];
}

/** Show these diagnostics in `view`'s gutter, replacing whatever was there. */
export function applyDiagnostics(view: EditorView, diagnostics: Diagnostic[]): void {
  view.dispatch({ effects: setDiagnostics.of(diagnostics) });
}
