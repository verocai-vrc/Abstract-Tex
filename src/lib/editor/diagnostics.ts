// Gutter markers for the last build's diagnostics (S2.7) and the language server's (S3.3b): a
// dot beside each line something is wrong on, coloured by severity, with the title as its
// tooltip. The drawer is where an explanation lives; the gutter only says "here".
//
// S3.3d adds a second visual for the LSP set only: a wavy underline drawn under the diagnostic's
// own range (`EditorDiagnostic.from`/`to`, kept unread since S3.3b for exactly this) and a hover
// tooltip over that underline showing the message. The gutter dot answers "which line"; the
// squiggle answers "which word", the way a spell-checker does. Build diagnostics stay gutter-only
// — a texlog line claim has no column, so there is nothing to underline.
//
// Two sources feed one gutter, and they are kept in two separate `StateField`s rather than one
// merged list because they have different lifetimes. A texlog set is replaced per build, every
// few seconds at most; an LSP set is replaced per publish, which can be per keystroke burst. One
// field would mean each source clearing the other's dots every time it spoke. Only the `markers`
// callback ever sees the two together.
//
// Note what does *not* happen here: an LSP diagnostic never reaches `app.compile.diagnostics`,
// the drawer, or `errorCount`/`warningCount` (the S3.3b decision — the reasoning is on
// `EditorDiagnostic` in `lsp-diagnostics.ts`). If you came looking for the drawer row, that is
// why there isn't one.
//
// CodeMirror state is immutable, so "the current diagnostics" is a `StateField` whose value
// is replaced by dispatching a `StateEffect` — the same pattern as its own lint gutter. The
// field maps its ranges through every edit, so a dot stays on its line while the author types
// above it, until the next build replaces the set.

import { RangeSet, RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, gutter, GutterMarker, hoverTooltip, type Tooltip } from '@codemirror/view';
import type { Diagnostic } from '../ipc';
import type { EditorDiagnostic } from '../lsp-diagnostics';
import { positionToOffset } from './positions';

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

/** Replace the build's marker set. Dispatch with `applyDiagnostics`, not by hand. */
const setDiagnostics = StateEffect.define<Diagnostic[]>();

/** Replace the language server's marker set. Dispatch with `applyLspDiagnostics`. A second
 * effect rather than a flag on the first, so a publish and a build can never clear each other's
 * dots. */
const setLspDiagnostics = StateEffect.define<EditorDiagnostic[]>();

const markers = StateField.define<RangeSet<DiagnosticMarker>>({
  create: () => RangeSet.empty,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setDiagnostics)) return build(effect.value, transaction.state.doc.lines, (n) => transaction.state.doc.line(n).from);
    }
    return value.map(transaction.changes);
  },
});

const lspMarkers = StateField.define<RangeSet<DiagnosticMarker>>({
  create: () => RangeSet.empty,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setLspDiagnostics))
        return buildLsp(effect.value, transaction.state.doc.lines, (n) => transaction.state.doc.line(n).from);
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

/** The same, for the language server's rows. A separate function rather than a parameter on
 * `build`, because the two inputs have different shapes: a texlog `Diagnostic` carries a
 * possibly-`null` 1-based `line` guessed from the log, an `EditorDiagnostic` carries a
 * `startLine` the server was certain about. Clamping is still needed — the document can have
 * shrunk between the publish and this dispatch. */
export function buildLsp(
  diagnostics: EditorDiagnostic[],
  lineCount: number,
  lineStart: (line: number) => number,
): RangeSet<DiagnosticMarker> {
  const byLine = new Map<number, EditorDiagnostic>();
  for (const diagnostic of diagnostics) {
    const line = Math.max(1, Math.min(diagnostic.startLine, lineCount));
    const existing = byLine.get(line);
    if (!existing || (existing.severity === 'warning' && diagnostic.severity === 'error')) byLine.set(line, diagnostic);
  }
  const ranges = [...byLine].map(([line, d]) => new DiagnosticMarker(d.severity, d.title).range(lineStart(line)));
  return RangeSet.of(ranges, true);
}

/** Combine the two sets into the one the gutter draws, at most one dot per position.
 *
 * Precedence: an error beats a warning, and on an exact tie `fromLog` wins. That tie-break is
 * DESIGN.md §5.2 in one line — the log parser's title is the *explained* sentence written by the
 * rule catalog, the server's is a raw string. Where both have something to say about a line, the
 * author should see the one written for them.
 *
 * Takes `RangeSet`s rather than the two diagnostic arrays, so a test can merge whatever `build`
 * and `buildLsp` actually produced instead of a hand-built parallel of them. */
export function mergeMarkers(
  fromLog: RangeSet<DiagnosticMarker>,
  fromLsp: RangeSet<DiagnosticMarker>,
): RangeSet<DiagnosticMarker> {
  const byPosition = new Map<number, DiagnosticMarker>();
  // LSP first, so the texlog pass below is the one holding the tie-break.
  //
  // The two rules are kept apart on purpose. Reusing `build`'s single condition here
  // (`existing is a warning && incoming is an error`) looks like it says the same thing, and
  // does for two markers from one source — but it also quietly makes an *equal* severity never
  // displace, which hands the tie to whichever set was read first. That was a real bug in this
  // function, caught by the test below it (see `bugs-issues-fixes.md`, S3.3b). So: severity
  // decides, and only when severity is equal does the later pass — texlog — win.
  for (const [set, isFromLog] of [
    [fromLsp, false],
    [fromLog, true],
  ] as const) {
    const cursor = set.iter();
    while (cursor.value) {
      const existing = byPosition.get(cursor.from);
      const incoming = cursor.value;
      const moreSevere = existing?.severity === 'warning' && incoming.severity === 'error';
      const equalAndFromLog = isFromLog && existing?.severity === incoming.severity;
      if (!existing || moreSevere || equalAndFromLog) byPosition.set(cursor.from, incoming);
      cursor.next();
    }
  }
  // `RangeSet.of` requires its input sorted by position; the map's insertion order is two
  // interleaved sets, so sorting here is not optional.
  const ranges = [...byPosition].sort((a, b) => a[0] - b[0]).map(([position, marker]) => marker.range(position));
  return RangeSet.of(ranges, true);
}

const theme = EditorView.baseTheme({
  '.cm-diag-gutter .cm-gutterElement': { padding: '0 2px 0 4px', fontSize: '9px' },
  '.cm-diag-error': { color: 'var(--error)' },
  '.cm-diag-warning': { color: 'var(--warn)' },
});

/** A wavy underline drawn with a `linear-gradient` background rather than `text-decoration:
 * wavy` — the CSS wavy underline's amplitude and colour are not controllable across browsers,
 * and this app already leans on Chromium-only rendering (it ships inside WebView2/WebKitGTK, not
 * a general browser), so a hand-drawn SVG-in-`background` wave is the more predictable choice. */
const squiggleTheme = EditorView.baseTheme({
  '.cm-diag-squiggle': {
    backgroundRepeat: 'repeat-x',
    backgroundPosition: 'left bottom',
    paddingBottom: '1px',
  },
  '.cm-diag-squiggle-error': {
    backgroundImage:
      "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='6' height='3'%3E%3Cpath d='M0 2.5 Q1.5 0.5 3 2.5 T6 2.5' fill='none' stroke='%23b0322a' stroke-width='1'/%3E%3C/svg%3E\")",
  },
  '.cm-diag-squiggle-warning': {
    backgroundImage:
      "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='6' height='3'%3E%3Cpath d='M0 2.5 Q1.5 0.5 3 2.5 T6 2.5' fill='none' stroke='%23a35f00' stroke-width='1'/%3E%3C/svg%3E\")",
  },
  '.cm-diag-hover': {
    maxWidth: '480px',
    padding: '4px 8px',
    fontFamily: 'var(--font-mono)',
    fontSize: '12px',
    whiteSpace: 'pre-wrap',
  },
});

/** One `Decoration.mark` per LSP diagnostic, positioned at its own range rather than its line —
 * `build`/`buildLsp` above answer "which line" for the gutter; this answers "which word" the way
 * a spell-checker underlines one. Exported for its test; `positionToOffset` is injected the same
 * way `lineStart` is injected into `build`, so the test needs no document.
 *
 * `RangeSetBuilder` is used instead of `RangeSet.of` (as `build`/`buildLsp` do) because
 * `Decoration.mark` ranges must be added in sorted, non-overlapping-at-the-same-start order for
 * the builder to accept them; sorting once here keeps that CodeMirror requirement in one place
 * rather than trusting every future caller to pass diagnostics pre-sorted. */
export function buildSquiggles(diagnostics: EditorDiagnostic[], toOffset: (position: EditorDiagnostic['from']) => number): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const sorted = [...diagnostics].sort((a, b) => toOffset(a.from) - toOffset(b.from));
  for (const diagnostic of sorted) {
    const from = toOffset(diagnostic.from);
    // `to` can equal `from` when the server sent a zero-width range; a mark decoration requires
    // `to > from` or CodeMirror throws, so widen a point range by one character rather than
    // dropping the diagnostic's underline entirely.
    const to = Math.max(toOffset(diagnostic.to), from + 1);
    builder.add(
      from,
      to,
      Decoration.mark({ class: `cm-diag-squiggle cm-diag-squiggle-${diagnostic.severity}`, diagnostic }),
    );
  }
  return builder.finish();
}

const setSquiggles = StateEffect.define<EditorDiagnostic[]>();

const squiggles = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setSquiggles))
        return buildSquiggles(effect.value, (position) => positionToOffset(transaction.state.doc, position));
    }
    return value.map(transaction.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** The diagnostic, if any, whose squiggle covers `pos`. Exported for its test — the same split as
 * `hoverSource`/`lspHoverSource` in `hover.ts`, so the search itself is callable without driving
 * `hoverTooltip`'s real lifecycle (idle timers, pointer tracking) from Vitest. */
export function diagnosticAt(decorations: DecorationSet, pos: number): EditorDiagnostic | null {
  let found: EditorDiagnostic | null = null;
  decorations.between(pos, pos, (_from, _to, decoration) => {
    found = (decoration.spec as { diagnostic: EditorDiagnostic }).diagnostic;
    return false; // stop at the first match
  });
  return found;
}

/** `hoverTooltip`'s source for the squiggle layer. Reads the `squiggles` field CodeMirror already
 * maintains rather than re-deriving anything, and resolves synchronously — no request goes out,
 * the message is already sitting in the decoration from the last publish. */
function diagnosticHoverSource(view: EditorView, pos: number): Tooltip | null {
  const diagnostic = diagnosticAt(view.state.field(squiggles), pos);
  if (!diagnostic) return null;
  return {
    pos,
    above: true,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'cm-diag-hover';
      dom.textContent = diagnostic.title;
      return { dom };
    },
  };
}

/** The extension to add to an editor. */
export function diagnosticGutter() {
  return [
    markers,
    lspMarkers,
    squiggles,
    gutter({
      class: 'cm-diag-gutter',
      markers: (view) => mergeMarkers(view.state.field(markers), view.state.field(lspMarkers)),
    }),
    hoverTooltip(diagnosticHoverSource),
    theme,
    squiggleTheme,
  ];
}

/** Show the last build's diagnostics in `view`'s gutter, replacing whatever the *build* put
 * there. Leaves the language server's dots alone. */
export function applyDiagnostics(view: EditorView, diagnostics: Diagnostic[]): void {
  view.dispatch({ effects: setDiagnostics.of(diagnostics) });
}

/** The same for the language server's, replacing whatever the *server* put there. Leaves the
 * build's dots alone. Dispatches both the gutter's effect and the squiggle layer's in one
 * transaction — they are two views of the one LSP publish, not two independent stores, so there
 * is no window where a caller could see one updated and not the other. */
export function applyLspDiagnostics(view: EditorView, diagnostics: EditorDiagnostic[]): void {
  view.dispatch({ effects: [setLspDiagnostics.of(diagnostics), setSquiggles.of(diagnostics)] });
}
