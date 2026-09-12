import { Text } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { offsetToPosition, positionToOffset } from './positions';

describe('offsetToPosition / positionToOffset', () => {
  it('round-trips a plain multi-line document', () => {
    const doc = Text.of(['\\documentclass{article}', '\\begin{document}', 'hello']);
    const offset = doc.line(2).from + 3; // inside "\begin"
    const position = offsetToPosition(doc, offset);
    expect(position).toEqual({ line: 1, character: 3 });
    expect(positionToOffset(doc, position)).toBe(offset);
  });

  it('puts line 0 first, matching LSP\'s zero-based lines against CodeMirror\'s one-based ones', () => {
    const doc = Text.of(['abc', 'def']);
    expect(offsetToPosition(doc, 0)).toEqual({ line: 0, character: 0 });
    expect(offsetToPosition(doc, 4)).toEqual({ line: 1, character: 0 }); // 'a','b','c','\n'
  });

  /** An astral emoji is two UTF-16 code units (a surrogate pair). LSP counts `character` in
   * UTF-16 units and so does JavaScript string indexing that CodeMirror's `Text` is built on, so
   * this file needs no special-casing — but that is exactly the kind of thing that looks obvious
   * and is wrong the one time a server disagrees, so it is asserted rather than assumed. */
  it('counts an astral emoji as two UTF-16 units, matching LSP', () => {
    const line = '✅😀x'; // ✅ is one unit, 😀 is a surrogate pair (two units), then 'x'
    expect(line.length).toBe(4);
    const doc = Text.of([line]);

    // The offset just after the emoji is 3 UTF-16 units in: ✅ (1) + 😀 (2).
    const position = offsetToPosition(doc, 3);
    expect(position).toEqual({ line: 0, character: 3 });
    expect(positionToOffset(doc, position)).toBe(3);

    const xPosition = offsetToPosition(doc, 4);
    expect(xPosition).toEqual({ line: 0, character: 4 });
    expect(positionToOffset(doc, xPosition)).toBe(4);
  });

  /** CJK characters are one UTF-16 unit each despite being visually wide, unlike the emoji above.
   * A line/column scheme that (wrongly) counted "visual width" or code points differently from
   * UTF-16 would misplace every position on a Chinese/Japanese/Korean line; this pins the
   * boundary against that mistake as well as the emoji case above. */
  it('counts each CJK character as one UTF-16 unit', () => {
    const doc = Text.of(['第一行', '第二行abc']);
    expect(doc.line(1).length).toBe(3);

    const position = offsetToPosition(doc, doc.line(2).from + 2);
    expect(position).toEqual({ line: 1, character: 2 });
    expect(positionToOffset(doc, position)).toBe(doc.line(2).from + 2);
  });

  it('clamps a position past the end of its line to the line length', () => {
    const doc = Text.of(['abc', 'de']);
    const offset = positionToOffset(doc, { line: 1, character: 99 });
    expect(offset).toBe(doc.line(2).to);
  });

  it('clamps a line number past the end of the document to the last line', () => {
    const doc = Text.of(['abc']);
    const offset = positionToOffset(doc, { line: 50, character: 0 });
    expect(offset).toBe(doc.line(1).from);
  });
});
