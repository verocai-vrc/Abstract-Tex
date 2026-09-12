// Converting between a CodeMirror document offset and an LSP `Position`.
//
// This is line/column arithmetic, not an encoding conversion: LSP positions are UTF-16 code
// units by default (a server may announce `positionEncoding: "utf-8"` in its `initialize`
// result, in which case this file would be wrong and would need to re-encode each line — TexLab
// does not do this, and `controller.svelte.ts` records which encoding `lspStart()` returned so a
// later loop can act on it if that ever changes). CodeMirror's `Text` also counts in UTF-16 code
// units, so the two line up without conversion; the surrogate-pair test below exists to prove
// that, not to exercise a conversion this file doesn't do.

import type { Text } from '@codemirror/state';
import type { Position } from '../lsp-protocol';

/** A CodeMirror document offset → the LSP `{line, character}` it falls on. `doc.lineAt` is
 * CodeMirror's own line lookup (binary search over its line-length tree), so this stays O(log n)
 * rather than scanning from the start of the document. */
export function offsetToPosition(doc: Text, offset: number): Position {
  const line = doc.lineAt(offset);
  return { line: line.number - 1, character: offset - line.from };
}

/** The inverse: an LSP `Position` → the CodeMirror offset it names. Clamped to the document's
 * actual line count and line length, because a server answer can point just past the end of a
 * line (a completion range that reaches the end-of-line) or, in principle, past the end of a
 * document that has shrunk since the request was sent. */
export function positionToOffset(doc: Text, position: Position): number {
  const lineNumber = Math.min(Math.max(position.line + 1, 1), doc.lines);
  const line = doc.line(lineNumber);
  const character = Math.min(Math.max(position.character, 0), line.length);
  return line.from + character;
}
