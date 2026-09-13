// Turning `textDocument/hover` into a CodeMirror `hoverTooltip`.
//
// Same seam as `completion.ts`: this file knows nothing about JSON-RPC or Tauri, and `lsp.ts`
// knows nothing about CodeMirror. It only calls an injected requester and maps the answer onto
// `@codemirror/view`'s tooltip types.

import { hoverTooltip, type EditorView, type Tooltip } from '@codemirror/view';
import { isHover, markupToPlainText, type Hover } from '../lsp-protocol';
import { offsetToPosition, positionToOffset } from './positions';

/** What `hover.ts` needs from the rest of the app: ask for a hover at a position, get back
 * whatever the server said (or `null`). `controller.svelte.ts`'s `lspHover` satisfies this. */
export type HoverRequester = (line: number, character: number) => Promise<Hover | null>;

/** The range a `Hover` covers, as CodeMirror offsets — the server's own `range` when it sent
 * one, or a single point at the requested offset when it did not (the spec allows omitting it,
 * and TexLab does for some replies). */
function rangeOf(doc: Parameters<typeof positionToOffset>[0], hover: Hover, atOffset: number): { from: number; to: number } {
  if (!hover.range) return { from: atOffset, to: atOffset };
  return {
    from: positionToOffset(doc, hover.range.start),
    to: positionToOffset(doc, hover.range.end),
  };
}

/**
 * The `HoverTooltipSource` function itself, exported separately from the extension it builds
 * (`lspHoverSource` below) so a test can call it directly rather than reaching into
 * `hoverTooltip`'s returned extension, which exposes no way to invoke its source function from
 * the outside (only an `active` state field, for reading what is currently shown).
 *
 * Returns `null` for every case that must degrade to "no tooltip," never an error popup — a dead
 * or missing server is a degraded mode, not a notice (DESIGN.md §2 commitment 6).
 */
export async function hoverSource(view: EditorView, pos: number, request: HoverRequester): Promise<Tooltip | null> {
  const position = offsetToPosition(view.state.doc, pos);
  let response: Hover | null;
  try {
    response = await request(position.line, position.character);
  } catch {
    return null;
  }
  if (!response || !isHover(response)) return null;

  const text = markupToPlainText(response.contents);
  if (!text) return null;

  const { from, to } = rangeOf(view.state.doc, response, pos);
  return {
    pos: from,
    end: to,
    above: true,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'cm-lsp-hover';
      dom.textContent = text;
      return { dom };
    },
  };
}

/**
 * Build a CodeMirror `hoverTooltip` extension backed by `request`.
 *
 * `hoverTooltip` itself owns the idle timer and the "is the pointer still over the same word"
 * bookkeeping (CodeMirror's default `hoverTime` is 300 ms); `hoverSource` above only has to
 * answer "what goes in the tooltip for this offset," the same division `lspCompletionSource`
 * draws for the popup list.
 */
export function lspHoverSource(request: HoverRequester) {
  return hoverTooltip((view, pos) => hoverSource(view, pos, request));
}
