// How a file on disk spells its text, and the plain text the editor holds instead (R7.11, R8.1, R8.3).
//
// The editor's buffer is `\n`-only and starts with no byte-order mark, whatever the file does: CodeMirror
// counts a line break as one position however it is written, and a mark is three bytes that say how the
// rest is encoded, not a character the caret may stand in front of. The file's own spelling is remembered
// and put back when the text is written. Pure, so `OpenDocument` (one file in a tab) and the conflict
// resolver (one file on its way to being staged) share one definition of it and neither needs the other.

/** How a file ends its lines. */
export type LineEnding = 'lf' | 'crlf';

/**
 * Which line ending `text` uses: CRLF when more of its line breaks are `\r\n` than bare `\n`, else
 * LF (which is also the answer for text with no line break at all, and a file made here).
 */
export function lineEndingOf(text: string): LineEnding {
  let crlf = 0;
  let lf = 0;
  for (let at = text.indexOf('\n'); at !== -1; at = text.indexOf('\n', at + 1)) {
    if (at > 0 && text.charCodeAt(at - 1) === 13) crlf++;
    else lf++;
  }
  return crlf > lf ? 'crlf' : 'lf';
}

/** `\r\n` as `\n`. The buffer never holds a `\r\n` (see `OpenDocument`). */
export function toLf(text: string): string {
  return text.includes('\r\n') ? text.replace(/\r\n/g, '\n') : text;
}

export function inLineEnding(text: string, ending: LineEnding): string {
  return ending === 'crlf' ? text.replace(/\n/g, '\r\n') : text;
}

/** The byte-order mark, as the character a decoder gives for the three bytes `EF BB BF` (R8.1). */
export const BYTE_ORDER_MARK = '\uFEFF';

export function hasByteOrderMark(text: string): boolean {
  return text.startsWith(BYTE_ORDER_MARK);
}

/** How a file spells what the buffer holds as plain text (R7.11, R8.1). */
export interface DiskForm {
  lineEnding: LineEnding;
  byteOrderMark: boolean;
}

/** How `diskText` is spelled. */
export function diskFormOf(diskText: string): DiskForm {
  return { lineEnding: lineEndingOf(diskText), byteOrderMark: hasByteOrderMark(diskText) };
}

/** The text as the buffer holds it: no byte-order mark and `\n` line endings. */
export function bufferTextOf(diskText: string): string {
  return toLf(hasByteOrderMark(diskText) ? diskText.slice(1) : diskText);
}

/** `bufferText` as a file with this form spells it. The way back from `bufferTextOf`. */
export function inDiskForm(bufferText: string, form: DiskForm): string {
  const text = inLineEnding(bufferText, form.lineEnding);
  return form.byteOrderMark ? BYTE_ORDER_MARK + text : text;
}
