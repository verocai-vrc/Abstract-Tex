// The Document map's data (DESIGN.md §5.3, first bullet): a flat outline of sections, figures,
// tables, labels and TODOs for one open file. Pure text in, plain data out — no Svelte,
// CodeMirror or Tauri import, the same shape `paths.ts` and `editor/symbols.ts` already use, so
// this module is unit-testable on its own and `controller.svelte.ts` is the only place that
// wires it to a live buffer.
//
// This is an outline, not a renderer: a title is whatever text sits inside the braces, written
// exactly as the author wrote it (`\emph{x}` and friends are left alone) — turning that into
// rendered LaTeX is the WYSIWYG non-goal in DESIGN.md §1.3.
//
// Known limit: a `\verb` or `verbatim` body is not excluded from scanning. A `%` or `\section`
// written literally inside one would be picked up as if it were real markup. Verbatim-aware
// scanning is more lexer than this loop's card asks for; revisit if it turns out to matter on a
// real document.

import type { FlatSymbol } from './editor/symbols';

export type OutlineKind = 'section' | 'figure' | 'table' | 'label' | 'todo';

export interface OutlineItem {
  kind: OutlineKind;
  title: string;
  /** 1-based, the way CodeMirror and the rest of the editor layer count lines. */
  line: number;
  /** 0 for `\part`, rising to 5 for `\paragraph`. Figures, tables, labels and TODOs are not
   * nested under anything, so they all sit at a single fixed indent — see `sectionLevel`. */
  level: number;
}

/** Longest name first: `\subsubsection` must not be read as `\section` by a shorter alternative
 * winning the match at the same starting position. (In practice no two names here share a
 * prefix, but ordering this way is the habit that stays correct if one is ever added.) */
const SECTION_LEVELS: Record<string, number> = {
  part: 0,
  chapter: 1,
  section: 2,
  subsection: 3,
  subsubsection: 4,
  paragraph: 5,
};
// `(?![a-zA-Z])` after the name is not optional decoration: a LaTeX control word is the whole
// run of letters after the backslash, so without it `\partial` matches the `part` alternative
// with `ial` left dangling, and `\paragraphindent` matches `paragraph` with `indent` left
// dangling — both real commands in math-heavy prose, both would otherwise show up as an
// empty-titled "Part"/"Paragraph" row in the Document map.
const SECTION_RE = /\\(subsubsection|subsection|chapter|section|paragraph|part)(?![a-zA-Z])(\*)?/g;
const BEGIN_ENV_RE = /\\begin\{(figure|table)(\*)?\}/g;
const LABEL_RE = /\\label\{/g;
const CAPTION_RE = /\\caption\{/;

/** The index of the first `%` that starts a real comment, or -1. `100\%` is an escaped percent
 * sign, not a comment start, so a `%` preceded by an odd run of backslashes does not count —
 * each backslash escapes the one after it, and an even run cancels out to "no escape" (`\\%`
 * is a literal backslash followed by a real comment). */
function commentIndex(line: string): number {
  for (let i = 0; i < line.length; i++) {
    if (line[i] !== '%') continue;
    let backslashes = 0;
    let j = i - 1;
    while (j >= 0 && line[j] === '\\') {
      backslashes++;
      j--;
    }
    if (backslashes % 2 === 0) return i;
  }
  return -1;
}

/** Extract a `{...}` group starting at `openIndex` (which must hold the `{`), balancing nested
 * braces so `\section{A \emph{nested} title}` does not stop at the first inner `}`. Capped at a
 * single line, per the card's risk note — a title that crosses a line break is a shape this scan
 * does not chase, and returns `null` so the caller can fall back to something plain rather than a
 * truncated fragment. */
function extractBraceGroup(line: string, openIndex: number): { title: string; endIndex: number } | null {
  if (line[openIndex] !== '{') return null;
  let depth = 0;
  const contentStart = openIndex + 1;
  for (let i = openIndex; i < line.length; i++) {
    if (line[i] === '{') depth++;
    else if (line[i] === '}') {
      depth--;
      if (depth === 0) return { title: line.slice(contentStart, i), endIndex: i + 1 };
    }
  }
  return null; // unbalanced within this line — see the module's known-limit note
}

/** Skip an optional `[short title]` LSP-style argument some sectioning commands take before the
 * braced one, e.g. `\section[Short]{Long title}`. Balances brackets for the same reason
 * `extractBraceGroup` balances braces, and is likewise capped at one line. Returns the index to
 * resume scanning from, unchanged if there was no bracket group. */
function skipOptionalBracket(line: string, index: number): number {
  let i = index;
  while (i < line.length && /\s/.test(line[i]!)) i++;
  if (line[i] !== '[') return index;
  let depth = 0;
  for (; i < line.length; i++) {
    if (line[i] === '[') depth++;
    else if (line[i] === ']') {
      depth--;
      if (depth === 0) return i + 1;
    }
  }
  return index; // unbalanced — leave the caller to fail its own brace search rather than guess
}

/** The title after a sectioning command match ends at `afterCommand`, or `''` if the line does
 * not actually have a braced argument (a malformed or line-wrapped `\section`). */
function sectionTitle(line: string, afterCommand: number): string {
  let i = afterCommand;
  while (i < line.length && /\s/.test(line[i]!)) i++;
  i = skipOptionalBracket(line, i);
  while (i < line.length && /\s/.test(line[i]!)) i++;
  const group = extractBraceGroup(line, i);
  return group ? group.title : '';
}

/** The title for a `figure`/`table` environment that began at `lines[beginLineIndex]`: the text
 * of its `\caption{...}` if one appears before the matching `\end`, else the bare kind name. Scans
 * forward line by line rather than the whole document at once, so a caption that never arrives
 * (or belongs to some other, later figure) cannot be attributed to this one. */
function environmentTitle(lines: string[], beginLineIndex: number, afterBegin: number, envName: 'figure' | 'table'): string {
  const fallback = envName === 'figure' ? 'Figure' : 'Table';
  const endRe = new RegExp(`\\\\end\\{${envName}\\*?\\}`);
  for (let i = beginLineIndex; i < lines.length; i++) {
    const active = activeText(lines[i]!);
    const searchFrom = i === beginLineIndex ? afterBegin : 0;
    const text = active.slice(searchFrom);
    const endMatch = endRe.exec(text);
    const capMatch = CAPTION_RE.exec(text);
    if (capMatch && (!endMatch || capMatch.index < endMatch.index)) {
      const openIndex = searchFrom + capMatch.index + capMatch[0].length - 1;
      const group = extractBraceGroup(active, openIndex);
      return group ? group.title : fallback;
    }
    if (endMatch) break;
  }
  return fallback;
}

/** The part of a line before an unescaped `%`, which is the only part sectioning commands,
 * `\begin`, and `\label` are read from — a command written after a comment marker is not live
 * LaTeX, just text the author left themselves. */
function activeText(line: string): string {
  const idx = commentIndex(line);
  return idx === -1 ? line : line.slice(0, idx);
}

interface PositionedItem {
  index: number; // where in the line's active text this was found, for left-to-right ordering
  item: OutlineItem;
}

/** Walk the document once, line by line, collecting sections, figure/table captions, labels and
 * `% TODO`/`% FIXME` comments in the order they appear. See the module comment for what this
 * deliberately does not try to do (render LaTeX, understand verbatim bodies). */
export function scanOutline(text: string): OutlineItem[] {
  const lines = text.split(/\r\n|\n/);
  const items: OutlineItem[] = [];

  for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
    const line = lines[lineIndex]!;
    const lineNumber = lineIndex + 1;
    const active = activeText(line);
    const found: PositionedItem[] = [];

    SECTION_RE.lastIndex = 0;
    for (let m = SECTION_RE.exec(active); m; m = SECTION_RE.exec(active)) {
      const level = SECTION_LEVELS[m[1]!]!;
      const title = sectionTitle(active, m.index + m[0].length);
      found.push({ index: m.index, item: { kind: 'section', title, line: lineNumber, level } });
    }

    BEGIN_ENV_RE.lastIndex = 0;
    for (let m = BEGIN_ENV_RE.exec(active); m; m = BEGIN_ENV_RE.exec(active)) {
      const envName = m[1] as 'figure' | 'table';
      const title = environmentTitle(lines, lineIndex, m.index + m[0].length, envName);
      found.push({ index: m.index, item: { kind: envName, title, line: lineNumber, level: 0 } });
    }

    LABEL_RE.lastIndex = 0;
    for (let m = LABEL_RE.exec(active); m; m = LABEL_RE.exec(active)) {
      const group = extractBraceGroup(active, m.index + m[0].length - 1);
      found.push({ index: m.index, item: { kind: 'label', title: group ? group.title : '', line: lineNumber, level: 0 } });
    }

    found.sort((a, b) => a.index - b.index);
    for (const { item } of found) items.push(item);

    const commentAt = commentIndex(line);
    if (commentAt !== -1) {
      const commentText = line.slice(commentAt + 1).trim();
      if (/^(TODO|FIXME)\b/.test(commentText)) {
        items.push({ kind: 'todo', title: commentText, line: lineNumber, level: 0 });
      }
    }
  }

  return items;
}

/**
 * Combine the scan above with TexLab's `documentSymbol` answer. The server, when it answers,
 * knows the document's real sectioning structure better than a regex ever will (it has parsed
 * the whole include graph, macros and all); the scan is what supplies figures, tables and labels
 * the server has no opinion about, and TODOs, which no LSP server has a concept of at all.
 *
 * A scanned section is dropped when a symbol already claims its line — the server's answer wins
 * on a shared line rather than showing the same heading twice. With no symbols at all (server not
 * ready, or nothing found), the scan is returned unchanged: it is the entire outline.
 */
export function mergeOutline(scanned: OutlineItem[], symbols: FlatSymbol[]): OutlineItem[] {
  if (symbols.length === 0) return scanned;

  const claimedLines = new Set(symbols.map((s) => s.line));
  const fromServer: OutlineItem[] = symbols.map((s) => ({
    kind: 'section',
    title: s.name,
    line: s.line,
    level: s.depth,
  }));
  const fromScan = scanned.filter((item) => item.kind === 'todo' || !claimedLines.has(item.line));

  return [...fromServer, ...fromScan].sort((a, b) => a.line - b.line);
}
