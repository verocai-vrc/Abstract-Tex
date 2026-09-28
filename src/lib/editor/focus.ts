//! Focus mode (S4.5): dim every paragraph except the one holding the cursor. Purely a view
//! effect — CodeMirror decorations and nothing else. This module must never read or write the
//! Yjs text or touch what gets saved to disk; DESIGN.md §1.3 rules out anything WYSIWYG-shaped,
//! and this is editor behaviour, not a document transform.

import { RangeSetBuilder, StateField, type EditorState, type Text, type Transaction } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';

/** A line with only whitespace ends a paragraph, same rule LaTeX itself uses for a blank line. */
function isBlank(text: string): boolean {
  return text.trim().length === 0;
}

/**
 * The `[from, to)` character range of the contiguous run of non-blank lines surrounding
 * `cursorOffset` in `doc` — the "current paragraph". If the cursor's own line is blank, the
 * paragraph is just that line: there is no prose around it to keep lit.
 *
 * Reads CodeMirror's `Text` directly (S9.6): `lineAt` and `line` are lookups in its own tree of
 * lines, so the cost is the size of this paragraph. It used to take the document as one string,
 * which meant copying the whole document into a new string on every keystroke to find it.
 */
export function currentParagraphRange(doc: Text, cursorOffset: number): { from: number; to: number } {
  const line = doc.lineAt(Math.max(0, Math.min(cursorOffset, doc.length)));
  if (isBlank(line.text)) return { from: line.from, to: line.to };

  let first = line.number;
  while (first > 1 && !isBlank(doc.line(first - 1).text)) first -= 1;
  let last = line.number;
  while (last < doc.lines && !isBlank(doc.line(last + 1).text)) last += 1;
  return { from: doc.line(first).from, to: doc.line(last).to };
}

/** Applied to every line outside the current paragraph. `app.css` supplies the actual dimming
 * (an opacity against the theme's own colours, never a hard-coded one). */
const dimmedLine = Decoration.line({ class: 'cm-focus-dim' });

/** Every line of `doc` outside `[from, to)` gets `dimmedLine`; the paragraph itself gets nothing,
 * which is what leaves it at full opacity against everything dimmed around it. This is the one
 * step that visits every line, so `focusField` below runs it only when it must. */
function buildDecorations(doc: Text, range: { from: number; to: number }): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (let lineNumber = 1; lineNumber <= doc.lines; lineNumber += 1) {
    const line = doc.line(lineNumber);
    if (line.from >= range.from && line.from <= range.to) continue; // inside (or opening) the lit paragraph
    builder.add(line.from, line.from, dimmedLine);
  }
  return builder.finish();
}

/** What focus mode keeps between transactions. `rebuilds` counts `buildDecorations` calls: it is
 * how the tests prove typing does not rebuild, since there is no view here to time. */
export interface FocusState {
  range: { from: number; to: number };
  decorations: DecorationSet;
  rebuilds: number;
}

function sameRange(a: { from: number; to: number }, b: { from: number; to: number }): boolean {
  return a.from === b.from && a.to === b.to;
}

function rebuilt(state: EditorState, range: FocusState['range'], rebuilds: number): FocusState {
  return { range, decorations: buildDecorations(state.doc, range), rebuilds: rebuilds + 1 };
}

/**
 * Focus mode's state (S9.6). A `StateField` rather than the `ViewPlugin` it used to be: the
 * decorations follow from the document and the cursor alone, and a field can be exercised in the
 * Node test environment, which has no DOM to host a view.
 *
 * Three cases, cheapest first:
 * - the cursor moved inside the lit paragraph and nothing was typed: keep everything;
 * - text was typed inside the lit paragraph and it is still the same paragraph: shift the
 *   existing dimming with the text — new lines inside the paragraph need no decoration;
 * - anything else (another paragraph, a blank line typed, an edit elsewhere — a collaborator's,
 *   or an external change arriving through the CRDT): rebuild.
 */
export const focusField = StateField.define<FocusState>({
  create: (state) => rebuilt(state, currentParagraphRange(state.doc, state.selection.main.head), 0),
  update(value: FocusState, transaction: Transaction): FocusState {
    if (!transaction.docChanged && !transaction.selection) return value;
    const { state } = transaction;
    const range = currentParagraphRange(state.doc, state.selection.main.head);
    if (!transaction.docChanged) {
      return sameRange(range, value.range) ? value : rebuilt(state, range, value.rebuilds);
    }
    // The old paragraph's bounds after the edit: an insertion at its start stays outside it
    // (-1), one at its end extends it (+1), exactly as typing at either edge does.
    const shifted = {
      from: transaction.changes.mapPos(value.range.from, -1),
      to: transaction.changes.mapPos(value.range.to, 1),
    };
    let editsInside = true;
    transaction.changes.iterChangedRanges((_fromA, _toA, fromB, toB) => {
      if (fromB < range.from || toB > range.to) editsInside = false;
    });
    if (editsInside && sameRange(shifted, range)) {
      return { range, decorations: value.decorations.map(transaction.changes), rebuilds: value.rebuilds };
    }
    return rebuilt(state, range, value.rebuilds);
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
});

/** Focus mode as a CodeMirror extension, on or off. `setup.ts` holds this behind a `Compartment`
 * so a toggle mid-session does not need to rebuild the editor. */
export function focusModeExtension(enabled: boolean) {
  return enabled ? [focusField] : [];
}
