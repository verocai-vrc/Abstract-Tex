//! Typewriter mode (S4.5): keep the cursor's line vertically centred in the viewport rather than
//! letting it drift to the bottom before CodeMirror's default "scroll just enough to stay in
//! view" kicks in. Purely a scroll-position effect — never touches the Yjs text, the same rule
//! `focus.ts` follows (DESIGN.md §1.3: not WYSIWYG, this is editor behaviour, not a document
//! transform).

import { EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

/** The numbers `centeredScrollTarget` needs, all already in pixels and already relative to the
 * scrollable content (not the viewport) — see `centerCursor` for how a real `EditorView`
 * supplies them. */
export interface ScrollMetrics {
  /** Height of the visible scroller. */
  viewportHeight: number;
  /** Top edge of the cursor's line, relative to the top of the scrollable content — i.e. it
   * already accounts for however far the view is currently scrolled. */
  cursorTop: number;
  /** Height of the cursor's line. */
  cursorHeight: number;
  /** The largest legal `scrollTop` for this content (content height minus viewport height); 0
   * for a document that fits on screen without scrolling at all. */
  maxScrollTop: number;
}

/**
 * Where the scroller's `scrollTop` should land so the cursor's line sits at the vertical centre
 * of the viewport, clamped to a range that actually exists. This is the testable core the card
 * asks for: plain numbers in and out, no `EditorView` in the signature, so it needs no real DOM
 * or webview to verify.
 */
export function centeredScrollTarget(metrics: ScrollMetrics): number {
  const centered = metrics.cursorTop + metrics.cursorHeight / 2 - metrics.viewportHeight / 2;
  const clampedMax = Math.max(0, metrics.maxScrollTop);
  return Math.max(0, Math.min(centered, clampedMax));
}

/** Read the four numbers `centeredScrollTarget` needs from a live view, then apply the result.
 * Kept separate from the math above on purpose: this is the one part that needs a real DOM and
 * so cannot be unit-tested in this sandbox (no webview here — see the loop's report). */
function centerCursor(view: EditorView): void {
  const scroller = view.scrollDOM;
  const coords = view.coordsAtPos(view.state.selection.main.head);
  if (!coords) return; // the cursor's position is not currently measured (e.g. mid-layout)
  const scrollerTop = scroller.getBoundingClientRect().top;
  const target = centeredScrollTarget({
    viewportHeight: scroller.clientHeight,
    cursorTop: coords.top - scrollerTop + scroller.scrollTop,
    cursorHeight: coords.bottom - coords.top,
    maxScrollTop: scroller.scrollHeight - scroller.clientHeight,
  });
  scroller.scrollTop = target;
}

/**
 * A `ViewPlugin` that re-centres the cursor's line after every change to the document or the
 * selection — covers both typing past the middle of the screen and moving the cursor with the
 * keyboard or a click.
 */
export function typewriterModePlugin() {
  return ViewPlugin.fromClass(
    class {
      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet) centerCursor(update.view);
      }
    },
  );
}

/** Typewriter mode as a CodeMirror extension, on or off. `setup.ts` holds this behind a
 * `Compartment` so a toggle mid-session does not need to rebuild the editor. */
export function typewriterModeExtension(enabled: boolean) {
  return enabled ? [typewriterModePlugin()] : [];
}
