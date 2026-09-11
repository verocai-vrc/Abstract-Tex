// CodeMirror 6 configuration for a LaTeX buffer bound to a Yjs document.
//
// Deliberately not `basicSetup`: that bundle includes CodeMirror's own history, which would
// fight the Yjs undo manager. Everything else from it is listed explicitly below so a reader
// can see what the editor is made of.

import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { defaultKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, HighlightStyle, indentOnInput, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import { stex } from '@codemirror/legacy-modes/mode/stex';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { EditorState } from '@codemirror/state';
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { yCollab } from 'y-codemirror.next';
import type { OpenDocument } from '../document';

// Colours come from the CSS custom properties in app.css so light and dark both work.
const latexHighlight = HighlightStyle.define([
  { tag: tags.tagName, color: 'var(--syn-command)' },
  { tag: tags.keyword, color: 'var(--syn-keyword)', fontWeight: '600' },
  { tag: tags.atom, color: 'var(--syn-math)' },
  { tag: tags.comment, color: 'var(--syn-comment)', fontStyle: 'italic' },
  { tag: tags.bracket, color: 'var(--syn-bracket)' },
  { tag: tags.number, color: 'var(--syn-number)' },
  { tag: tags.string, color: 'var(--syn-string)' },
  { tag: tags.variableName, color: 'var(--syn-command)' },
]);

const theme = EditorView.theme({
  '&': { height: '100%', fontSize: '14px', backgroundColor: 'var(--bg-editor)', color: 'var(--fg)' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.65' },
  '.cm-content': { padding: '12px 0', caretColor: 'var(--accent)' },
  '.cm-gutters': {
    backgroundColor: 'var(--bg-editor)',
    color: 'var(--fg-muted)',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--bg-active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--bg-active-line)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--selection)',
  },
  '.cm-matchingBracket': { outline: '1px solid var(--syn-bracket)', backgroundColor: 'transparent' },
});

export function createEditor(parent: HTMLElement, doc: OpenDocument): EditorView {
  const state = EditorState.create({
    // y-codemirror requires the initial CodeMirror document to equal the Y.Text content.
    doc: doc.text(),
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      EditorView.lineWrapping,
      StreamLanguage.define(stex),
      syntaxHighlighting(latexHighlight),
      // No app shortcuts here: `Ctrl S`, `Ctrl B`, `F5` and the rest are handled once, on the
      // window, by App.svelte (S2.4). A binding in this keymap does not stop propagation, so a
      // copy here would fire the action twice for a keypress inside the editor.
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        indentWithTab,
      ]),
      // No awareness yet (that is v0.8); the undo manager is the document's own.
      yCollab(doc.ytext, null, { undoManager: doc.undo }),
      theme,
    ],
  });
  return new EditorView({ state, parent });
}

/** Move the cursor to a 1-based line, centre it, and focus the editor. */
export function goToLine(view: EditorView, line: number): void {
  const clamped = Math.max(1, Math.min(line, view.state.doc.lines));
  const { from } = view.state.doc.line(clamped);
  view.dispatch({
    selection: { anchor: from },
    effects: EditorView.scrollIntoView(from, { y: 'center' }),
  });
  view.focus();
}
