// Comments, as the panel needs them (S14.3c): plain functions over the rows Rust sends
// (`ipc.CommentRow`), so `comments.test.ts` runs them with literals and no Svelte runtime.

import type { CommentRow } from './ipc';

/** Where a comment's words are in the editor's buffer right now, in UTF-16 units, or `null`.
 *
 * Rust anchored the comment against the file *on disk*; the buffer may have unsaved edits since.
 * So its offsets are trusted only if the buffer still holds the quote there; otherwise the quote
 * is searched for, and a quote that has gone from the buffer (or that the buffer now repeats, which
 * would be a guess) gives `null` rather than a jump to the wrong sentence. */
export function locateInBuffer(text: string, row: CommentRow): { from: number; to: number } | null {
  if (row.from !== null && row.to !== null && text.slice(row.from, row.to) === row.quote) {
    return { from: row.from, to: row.to };
  }
  if (row.quote === '') return null;
  const first = text.indexOf(row.quote);
  if (first === -1 || text.indexOf(row.quote, first + 1) !== -1) return null;
  return { from: first, to: first + row.quote.length };
}

/** The 1-based line `offset` is on. */
export function lineAt(text: string, offset: number): number {
  let line = 1;
  const end = Math.min(offset, text.length);
  for (let index = 0; index < end; index += 1) {
    if (text.charCodeAt(index) === 10) line += 1;
  }
  return line;
}

export interface CommentGroups {
  open: CommentRow[];
  resolved: CommentRow[];
}

/** Open comments first, those on the file in the editor ahead of the others; resolved ones apart.
 * Within each, the order Rust sent (oldest first) is kept. */
export function groupComments(rows: readonly CommentRow[], activePath: string | null): CommentGroups {
  const open = rows.filter((row) => !row.resolved);
  return {
    open: [...open.filter((row) => row.file === activePath), ...open.filter((row) => row.file !== activePath)],
    resolved: rows.filter((row) => row.resolved),
  };
}

/** A sentence for a comment whose words could not be found, or `null` when they were. */
export function problemSentence(row: CommentRow): string | null {
  switch (row.state) {
    case 'anchored':
      return null;
    case 'orphaned':
      return 'The text this was about has changed, or now appears more than once.';
    case 'fileMissing':
      return `${row.file} is no longer in the project.`;
  }
}

/** A comment's quote for a list row: one line, cut at `limit` characters. */
export function quotePreview(quote: string, limit = 60): string {
  const flat = quote.replace(/\s+/g, ' ').trim();
  return flat.length <= limit ? flat : `${flat.slice(0, limit - 1)}…`;
}
