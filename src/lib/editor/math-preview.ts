//! Maths preview on hover (S4.6): find the `$…$` / `\[…\]` / `\begin{align}…` span under the
//! pointer, render it with KaTeX, show the result in a CodeMirror `hoverTooltip`. Purely a view
//! popover. This module must never read or write the Yjs text and never touches disk — it reads
//! the editor's current string on hover and hands back HTML for a tooltip, nothing more.
//! DESIGN.md §1.3 rules out anything WYSIWYG-shaped; a popover that appears 300 ms after the
//! pointer stops is a preview, not a rendering of the document, and nothing here runs on a
//! keystroke (§2 commitment 2).

import type { Text } from '@codemirror/state';
import { hoverTooltip, type EditorView, type Tooltip } from '@codemirror/view';
import katex from 'katex';
import { currentParagraphRange } from './focus';

/** A maths span found by `mathAtOffset`: `[from, to]` covers the delimiters too (so hovering a
 * `$` counts as inside), `tex` is what KaTeX gets, `display` picks KaTeX's display mode. */
export interface MathSpan {
  from: number;
  to: number;
  tex: string;
  display: boolean;
}

/** Above this, the popover would be a wall of glyphs and KaTeX's own time starts to show. */
const MAX_TEX_LENGTH = 2000;

/** The environments KaTeX can render whole, `\begin`/`\end` wrapper included, so `tex` keeps the
 * wrapper for these rather than the bare body. Order does not matter; the name is matched exactly
 * against what follows `\begin{`. */
const MATH_ENVIRONMENTS = ['equation', 'equation*', 'align', 'align*', 'gather', 'gather*', 'multline', 'multline*'];

/** One recognised opening delimiter: what closes it, and whether the result is display maths. */
interface Delimiter {
  open: string;
  close: string;
  display: boolean;
  /** `true` keeps the delimiters in `tex` — only the environments want that. */
  keepWrapper: boolean;
}

/** `true` when the character at `index` is preceded by an odd number of backslashes, i.e. it is
 * escaped (`\$` is a dollar sign in prose, `\\$` is a line break followed by a real `$`). */
function isEscaped(text: string, index: number): boolean {
  let backslashes = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) backslashes += 1;
  return backslashes % 2 === 1;
}

/** Index of the next `needle` at or after `start` that is not escaped, or -1. */
function indexOfUnescaped(text: string, needle: string, start: number): number {
  let candidate = text.indexOf(needle, start);
  while (candidate !== -1 && isEscaped(text, candidate)) {
    candidate = text.indexOf(needle, candidate + 1);
  }
  return candidate;
}

/**
 * Which delimiter, if any, opens at `index` in `text`. Tried in the card's order — `\[` before
 * `\(`, `$$` before `$` — because `$$` starts with `$` and a scan that tried `$` first would
 * pair the two halves of an empty `$$` with each other.
 *
 * The caller has already ruled out an escaped character at `index`, so `\` here really does
 * start a command: `\[`, `\(`, `\begin{…}`, or some other command we step over.
 */
function delimiterAt(text: string, index: number): Delimiter | null {
  if (text.startsWith('\\[', index)) return { open: '\\[', close: '\\]', display: true, keepWrapper: false };
  if (text.startsWith('$$', index)) return { open: '$$', close: '$$', display: true, keepWrapper: false };
  if (text.startsWith('\\(', index)) return { open: '\\(', close: '\\)', display: false, keepWrapper: false };
  if (text[index] === '$') return { open: '$', close: '$', display: false, keepWrapper: false };
  if (text.startsWith('\\begin{', index)) {
    const nameEnd = text.indexOf('}', index);
    if (nameEnd === -1) return null;
    const name = text.slice(index + '\\begin{'.length, nameEnd);
    if (!MATH_ENVIRONMENTS.includes(name)) return null;
    return { open: `\\begin{${name}}`, close: `\\end{${name}}`, display: true, keepWrapper: true };
  }
  return null;
}

/**
 * `text` with every comment replaced by spaces of the same length, so offsets are unchanged.
 *
 * A comment starts at a `%` that is not escaped and runs to the end of the line. Stepping over
 * the character after each `\` is what keeps `\%` (a percent sign) from starting one, and lets
 * `\\%` (a line break, then a comment) start one. Without this, `% price is $5` leaves a stray
 * `$` that pairs with the next real opener and shifts every span after it in the paragraph.
 */
export function blankComments(text: string): string {
  let result = '';
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === '\\') {
      result += text.slice(index, index + 2);
      index += 2;
    } else if (character === '%') {
      const lineEnd = text.indexOf('\n', index);
      const end = lineEnd === -1 ? text.length : lineEnd;
      result += ' '.repeat(end - index);
      index = end;
    } else {
      result += character;
      index += 1;
    }
  }
  return result;
}

/**
 * The maths span containing `offset` in `doc`, or `null` when the offset is in prose, the
 * span is never closed within its paragraph, or the expression is too long to preview.
 *
 * Plain strings and numbers in, a span out, no CodeMirror types anywhere in the signature — the
 * same rule `focus.ts` follows, and for the same reason: this is the part a Node-only test can
 * exercise. The scan is bounded to the paragraph around `offset` (via `currentParagraphRange`),
 * so a hover on a long chapter costs one paragraph, not the whole file, and a `$` left open two
 * pages up cannot swallow the rest of the document.
 *
 * `%` comments are blanked before the scan (see [`blankComments`]), so a `$` in one cannot pair
 * with a real opener. Not attempted: `verbatim`.
 */
export function mathAtOffset(doc: Text, offset: number): MathSpan | null {
  const paragraph = currentParagraphRange(doc, offset);
  // Only the paragraph is copied out of the document, never the whole of it (S9.6).
  const text = doc.sliceString(paragraph.from, paragraph.to);
  // The scan looks at `scannable`, the same text with comments blanked; the span's TeX is still
  // cut from `text`, because a comment inside a formula is the author's and KaTeX copes with it.
  const scannable = blankComments(text);
  const localOffset = offset - paragraph.from;

  let scanFrom = 0;
  while (scanFrom < scannable.length) {
    const character = scannable[scanFrom];
    if (character !== '$' && character !== '\\') {
      scanFrom += 1;
      continue;
    }

    const delimiter = delimiterAt(scannable, scanFrom);
    if (delimiter === null) {
      // `\` followed by something that is not a maths opener (`\alpha`, `\\`, `\$`): step over
      // the backslash *and* the character it escapes, so `\$` never reads as a `$`.
      scanFrom += character === '\\' ? 2 : 1;
      continue;
    }

    const spanStart = scanFrom;
    const bodyStart = spanStart + delimiter.open.length;
    const closeAt = indexOfUnescaped(scannable, delimiter.close, bodyStart);
    if (closeAt === -1) {
      // Unclosed within the paragraph: everything from here on is one broken span, and the
      // offset is either in it (null) or before it (already checked, so also prose).
      return null;
    }
    const spanEnd = closeAt + delimiter.close.length;

    if (localOffset < spanStart) return null; // prose between two spans
    if (localOffset <= spanEnd) {
      const tex = delimiter.keepWrapper ? text.slice(spanStart, spanEnd) : text.slice(bodyStart, closeAt);
      if (tex.length > MAX_TEX_LENGTH) return null;
      return {
        from: paragraph.from + spanStart,
        to: paragraph.from + spanEnd,
        tex,
        display: delimiter.display,
      };
    }
    scanFrom = spanEnd;
  }
  return null;
}

/** What `renderMath` hands back: KaTeX's HTML, or the one-line reason it could not render. */
export type RenderResult = { html: string } | { error: string };

/**
 * KaTeX's error message is `KaTeX parse error: <sentence> at position <n>: <source with
 * combining underlines>` (or `… at end of input: <source>`). The popover sits on the expression
 * already, so the position and the echo of the source add nothing; the sentence alone is what
 * §5.3 promises ("catches bracket errors before a compile does").
 */
function shortMessage(raw: string): string {
  return raw.replace(/^KaTeX parse error: /, '').replace(/ at (?:position \d+|end of input):.*$/s, '');
}

/**
 * KaTeX has no `multline` environment (it has `gather`, which is the same one-equation-per-line
 * shape minus the left/right alignment of the first and last lines). A preview that reads "No
 * such environment" for a perfectly good `multline` would be the tool's gap presented as the
 * author's error, so the wrapper is swapped for KaTeX's nearest equivalent here — in the KaTeX
 * adapter, not in `mathAtOffset`, which reports the span as written.
 */
function substituteUnsupportedEnvironments(tex: string): string {
  return tex.replace(/\\(begin|end)\{multline(\*?)\}/g, '\\$1{gather$2}');
}

/**
 * Render `tex` to HTML with KaTeX, or explain in one sentence why it could not be. Never throws:
 * a malformed expression is the *expected* input while someone is mid-edit, and a preview that
 * threw would take the hover machinery down with it.
 *
 * No macro table is passed, so a `\newcommand` from the preamble renders as KaTeX's "Undefined
 * control sequence" — the honest answer until a later loop walks the include graph to the
 * preamble (SPRINTS.md §4 decision row for S4.6).
 */
export function renderMath(tex: string, display: boolean): RenderResult {
  try {
    const html = katex.renderToString(substituteUnsupportedEnvironments(tex), {
      displayMode: display,
      throwOnError: true,
      output: 'html',
      // Do not `console.warn` about every non-ASCII glyph in an expression: authors type accents.
      strict: 'ignore',
    });
    return { html };
  } catch (thrown) {
    // `instanceof` narrows the caught `unknown` to something with a `.message`; anything else
    // KaTeX might throw (it should not) still becomes a string rather than escaping.
    const message = thrown instanceof Error ? thrown.message : String(thrown);
    return { error: shortMessage(message) };
  }
}

/**
 * The `HoverTooltipSource` itself, exported separately from the extension (`mathPreview` below)
 * for the same reason `hover.ts` exports `hoverSource`: `hoverTooltip`'s returned extension gives
 * a test no way to invoke its source function.
 *
 * Synchronous, unlike the LSP one — KaTeX is a pure function of the string, so there is no
 * request to await. Rendering happens here, once per hover, not in `create`, so a render failure
 * still produces a tooltip (the error line) rather than a blank box.
 */
export function mathPreviewSource(view: EditorView, pos: number): Tooltip | null {
  const span = mathAtOffset(view.state.doc, pos);
  if (span === null) return null;

  const rendered = renderMath(span.tex, span.display);
  return {
    pos: span.from,
    end: span.to,
    above: true,
    create: () => {
      const dom = document.createElement('div');
      if ('html' in rendered) {
        dom.className = 'cm-math-preview';
        // KaTeX's HTML is built from the author's own expression with `trust` off (the
        // default), so it contains no scripts, URLs or raw markup from the source.
        dom.innerHTML = rendered.html;
      } else {
        dom.className = 'cm-math-preview-error';
        dom.textContent = rendered.error;
      }
      return { dom };
    },
  };
}

/**
 * The maths preview as a CodeMirror extension. A second, independent `hoverTooltip` next to the
 * LSP one in `hover.ts`; TexLab does not answer hover inside `$…$`, so the two do not fight in
 * practice. `hoverTooltip`'s default 300 ms idle time is kept — nothing in this module runs
 * until the pointer has rested.
 */
export function mathPreview() {
  return hoverTooltip(mathPreviewSource);
}
