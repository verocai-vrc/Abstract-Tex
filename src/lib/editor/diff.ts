// The side-by-side view of one file's change (S11.7): CodeMirror's merge view, read-only on both
// sides. The one place `@codemirror/merge` is used, so the rest of the app never imports it.
//
// Read-only on both sides on purpose, and a departure from the card (SPRINTS.md S11.7b): an open
// tab's `Y.Text` is the file's single editable copy while the file is open (S2.3), and a second
// editable view of the same file would be a second writer with no way to reconcile. *Open file* in
// the dialog is the way to edit.

import { StreamLanguage, syntaxHighlighting, HighlightStyle } from '@codemirror/language';
import { stex } from '@codemirror/legacy-modes/mode/stex';
import { MergeView } from '@codemirror/merge';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

const theme = EditorView.theme({
  '&': { fontSize: '13px', backgroundColor: 'var(--bg-editor)', color: 'var(--fg)' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.55' },
  '.cm-gutters': { backgroundColor: 'var(--bg-editor)', color: 'var(--fg-muted)', borderRight: '1px solid var(--border)' },
});

/** The same colours the editor's LaTeX highlighting uses would be nicer; this is the small subset
 * that makes a diff of `.tex` readable without importing the editor's whole setup. */
const highlight = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--syn-keyword, var(--accent))' },
  { tag: tags.comment, color: 'var(--fg-muted)', fontStyle: 'italic' },
]);

const readOnly = [EditorState.readOnly.of(true), EditorView.editable.of(false), EditorView.lineWrapping];

/** Build the view in `parent`. Unchanged runs are folded away, so a one-line edit to a long file
 * shows the line and its neighbours, not forty pages of agreement. The caller destroys it. */
export function createDiffView(parent: HTMLElement, before: string, after: string): MergeView {
  const extensions = [...readOnly, StreamLanguage.define(stex), syntaxHighlighting(highlight), theme];
  return new MergeView({
    a: { doc: before, extensions },
    b: { doc: after, extensions },
    parent,
    gutter: true,
    highlightChanges: true,
    collapseUnchanged: { margin: 3, minSize: 6 },
  });
}
