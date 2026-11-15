// The arithmetic and the decisions of the PDF pane that need no DOM, so they can be tested without
// one: where a click on a page is in PDF points, what a link in the PDF should do, how far to zoom.
// `viewer.ts` owns pdf.js and the elements; this file owns none of them.

/** The zoom range, as a multiple of the fit-to-width size. */
export const ZOOM_MIN = 0.25;
export const ZOOM_MAX = 4;

export function clampZoom(zoom: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
}

/**
 * A click on a page, in PDF points from the page's top-left corner — the unit and the origin SyncTeX
 * answers in. `cssPerPoint` is how many CSS pixels one PDF point is at the current zoom (pdf.js calls
 * it the viewport's `scale`). `page` is the page box's position on screen; its border, if it has one,
 * is not part of the page.
 */
export function pointsFromClick(
  click: { clientX: number; clientY: number },
  page: { left: number; top: number; borderLeft?: number; borderTop?: number },
  cssPerPoint: number,
): { x: number; y: number } {
  const x = (click.clientX - page.left - (page.borderLeft ?? 0)) / cssPerPoint;
  const y = (click.clientY - page.top - (page.borderTop ?? 0)) / cssPerPoint;
  return { x, y };
}

/** What clicking a link inside the PDF does. */
export type LinkAction = { kind: 'browser'; url: string } | { kind: 'internal' } | { kind: 'refused' };

/**
 * The only addresses a PDF may send the person to are web pages, and only in their own browser: never
 * in this window (a PDF that navigated the window would replace the editor with a page of its
 * author's choosing), and never `file:`, `javascript:`, `mailto:` and the rest. A link to another
 * place in the same PDF (`#page=3`, a named destination) is the PDF viewer's own business.
 */
export function linkAction(href: string): LinkAction {
  if (href.startsWith('#')) return { kind: 'internal' };
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return { kind: 'refused' };
  }
  if (url.protocol === 'https:' || url.protocol === 'http:') return { kind: 'browser', url: url.href };
  return { kind: 'refused' };
}

/** A rectangle on screen, in CSS pixels. */
export interface ScreenBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** How close to an edge of the pane a match may be and still count as in view: a line that near the
 * edge is half hidden by it. */
const REVEAL_MARGIN = 24;

/**
 * Where to scroll the pane so the reader can see `match` (a search result), or null when they already can.
 * Both boxes are on screen; `scroll` is where the pane is scrolled now. A match out of view goes a third
 * of the way down the pane, which leaves room above it for the line before — pdf.js's own choice, to the
 * top edge, hides the context. One that is merely out to the side (a zoomed page) is centred across.
 */
export function scrollToReveal(
  match: ScreenBox,
  pane: ScreenBox,
  scroll: { left: number; top: number },
): { left: number; top: number } | null {
  const outUpOrDown =
    match.top < pane.top + REVEAL_MARGIN || match.top + match.height > pane.top + pane.height - REVEAL_MARGIN;
  const outToTheSide =
    match.left < pane.left + REVEAL_MARGIN || match.left + match.width > pane.left + pane.width - REVEAL_MARGIN;
  if (!outUpOrDown && !outToTheSide) return null;

  const top = outUpOrDown ? scroll.top + (match.top - pane.top) - pane.height / 3 : scroll.top;
  const left = outToTheSide
    ? scroll.left + (match.left + match.width / 2) - (pane.left + pane.width / 2)
    : scroll.left;
  return { left, top };
}
