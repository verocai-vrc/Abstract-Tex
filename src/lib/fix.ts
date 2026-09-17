// Locating a rule catalog's one-click fix (S5.5's `Fix`) in real document text (S6.2).
//
// `texlog` never reads the `.tex` source — `find`/`replace` are literal substrings on the
// diagnosed line, not byte offsets (`ipc.ts`'s own `Fix` doc comment). This module is the other
// half: it does have the source, so it turns a `Fix` plus a 1-indexed line number into the exact
// character range to replace, or declines when the line does not actually hold what the fix
// expects. A diagnostic's `line` can be approximate, or point at the wrong physical line
// entirely (`crates/texlog/src/rules.rs`'s own doc comment on `explain_line_end_with_nothing_
// before_it` names a real case) — silently doing nothing is the safe outcome there, not an edit
// to whatever happens to be on that line instead.

import type { Fix } from './ipc';

export interface LineEdit {
  from: number;
  to: number;
  insert: string;
}

/** Where in `text` to apply `fix`, or `null` if `line` does not exist or its content does not
 * contain `fix.find`. Pure and testable with no CodeMirror or Yjs types, the same split
 * `editor/focus.ts` and `editor/positions.ts` already use between a plain-data core and the
 * stateful thing that calls it. */
export function locateFix(text: string, line: number, fix: Fix): LineEdit | null {
  const lines = text.split('\n');
  const index = line - 1;
  if (index < 0 || index >= lines.length) return null;
  const lineText = lines[index]!;
  const column = lineText.indexOf(fix.find);
  if (column === -1) return null;
  let lineStart = 0;
  for (let i = 0; i < index; i++) lineStart += lines[i]!.length + 1;
  return { from: lineStart + column, to: lineStart + column + fix.find.length, insert: fix.replace };
}
