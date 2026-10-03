// Tells the app where the author's selection is, so an assistant action knows what to act on (S12.3b).
//
// Reports offsets only, never text: the app reads the text from the document's own `Y.Text`, the
// one editable copy (S2.3), and a selection can be a whole chapter. The report is made on each
// selection change, which is cheap — two numbers — and an empty selection is reported as `from === to`.

import { EditorView } from '@codemirror/view';

export function selectionReporter(report: (from: number, to: number) => void) {
  return EditorView.updateListener.of((update) => {
    if (!update.selectionSet && !update.docChanged) return;
    const { from, to } = update.state.selection.main;
    report(from, to);
  });
}
