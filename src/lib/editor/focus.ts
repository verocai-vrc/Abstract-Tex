//! Focus mode (S4.5): dim every paragraph except the one holding the cursor. Purely a view
//! effect — CodeMirror decorations and nothing else. This module must never read or write the
//! Yjs text or touch what gets saved to disk; DESIGN.md §1.3 rules out anything WYSIWYG-shaped,
//! and this is editor behaviour, not a document transform.

import { RangeSetBuilder } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

/** A line with only whitespace ends a paragraph, same rule LaTeX itself uses for a blank line. */
function isBlank(text: string): boolean {
  return text.trim().length === 0;
}

/**
 * The `[from, to)` character range of the contiguous run of non-blank lines surrounding
 * `cursorOffset` in `docText` — the "current paragraph". If the cursor's own line is blank, the
 * paragraph is just that line: there is no prose around it to keep lit.
 *
 * This is the testable core the card asks for: plain strings and numbers in, a range out, no
 * CodeMirror types anywhere in the signature. `buildDecorations` below is the only caller, and it
 * supplies the one thing this function cannot know on its own — which lines exist and where the
 * line breaks are — by walking `docText` with `indexOf`/`lastIndexOf` rather than materialising
 * an array of every line, so the cost stays close to "the size of this paragraph" rather than
 * "the size of the document" even on a long thesis.
 */
export function currentParagraphRange(docText: string, cursorOffset: number): { from: number; to: number } {
  const clampedOffset = Math.max(0, Math.min(cursorOffset, docText.length));
  const lineStart = docText.lastIndexOf('\n', clampedOffset - 1) + 1;
  let lineEnd = docText.indexOf('\n', clampedOffset);
  if (lineEnd === -1) lineEnd = docText.length;

  if (isBlank(docText.slice(lineStart, lineEnd))) {
    return { from: lineStart, to: lineEnd };
  }

  let from = lineStart;
  while (from > 0) {
    const previousLineEnd = from - 1; // the '\n' immediately before this line
    const previousLineStart = docText.lastIndexOf('\n', previousLineEnd - 1) + 1;
    if (isBlank(docText.slice(previousLineStart, previousLineEnd))) break;
    from = previousLineStart;
  }

  let to = lineEnd;
  while (to < docText.length) {
    const nextLineStart = to + 1;
    let nextLineEnd = docText.indexOf('\n', nextLineStart);
    if (nextLineEnd === -1) nextLineEnd = docText.length;
    if (isBlank(docText.slice(nextLineStart, nextLineEnd))) break;
    to = nextLineEnd;
  }

  return { from, to };
}

/** Applied to every line outside the current paragraph. `app.css` supplies the actual dimming
 * (an opacity against the theme's own colours, never a hard-coded one). */
const dimmedLine = Decoration.line({ class: 'cm-focus-dim' });

/** Every line of `view`'s document outside `[from, to)` gets `dimmedLine`; the paragraph itself
 * gets nothing, which is what leaves it at full opacity against everything dimmed around it. */
function buildDecorations(view: EditorView, from: number, to: number): DecorationSet {
  const doc = view.state.doc;
  const builder = new RangeSetBuilder<Decoration>();
  for (let lineNumber = 1; lineNumber <= doc.lines; lineNumber += 1) {
    const line = doc.line(lineNumber);
    if (line.from >= from && line.from <= to) continue; // inside (or opening) the lit paragraph
    builder.add(line.from, line.from, dimmedLine);
  }
  return builder.finish();
}

/**
 * A `ViewPlugin` — CodeMirror's mechanism for attaching per-view state with a lifecycle
 * (`update` runs after every change to the document or the selection) — that recomputes the
 * dimming set on typing and on moving the cursor to a different paragraph alike. The document
 * text is re-read only when the document itself changed; a cursor move within the same document
 * reuses it, since `currentParagraphRange` only needs a fresh string when the text underneath it
 * has.
 */
export function focusModePlugin() {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      private docText: string;

      constructor(view: EditorView) {
        this.docText = view.state.doc.toString();
        this.decorations = this.recompute(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged) this.docText = update.view.state.doc.toString();
        if (update.docChanged || update.selectionSet) {
          this.decorations = this.recompute(update.view);
        }
      }

      private recompute(view: EditorView): DecorationSet {
        const { from, to } = currentParagraphRange(this.docText, view.state.selection.main.head);
        return buildDecorations(view, from, to);
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}

/** Focus mode as a CodeMirror extension, on or off. `setup.ts` holds this behind a `Compartment`
 * so a toggle mid-session does not need to rebuild the editor. */
export function focusModeExtension(enabled: boolean) {
  return enabled ? [focusModePlugin()] : [];
}
