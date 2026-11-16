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

/** The one key every centring request is filed under, so CodeMirror runs the latest of several
 * requests made in the same frame (typing and a cursor move) once, not once each. */
const centeringKey = {};

/** The layout CodeMirror lets nobody read while it is applying an update. `null` when the cursor's
 * position is not currently measured (e.g. mid-layout), which leaves the scroll where it is. */
function readMetrics(view: EditorView): ScrollMetrics | null {
  const scroller = view.scrollDOM;
  const coords = view.coordsAtPos(view.state.selection.main.head);
  if (!coords) return null;
  const scrollerTop = scroller.getBoundingClientRect().top;
  return {
    viewportHeight: scroller.clientHeight,
    cursorTop: coords.top - scrollerTop + scroller.scrollTop,
    cursorHeight: coords.bottom - coords.top,
    maxScrollTop: scroller.scrollHeight - scroller.clientHeight,
  };
}

/**
 * Centre the cursor's line, in CodeMirror's measure phase and not now. A plugin's `update` runs
 * while the view is applying a change, and CodeMirror throws if layout is read there ("Reading the
 * editor layout isn't allowed during an update"): it logs the throw, switches the plugin off for
 * that view, and the author sees only that the mode never does anything (R6.21). `requestMeasure`
 * is the sanctioned way round it: `read` runs once the DOM is in step with the new state, and
 * `write` runs after every read, before CodeMirror scrolls the cursor into view, so by the time it
 * checks, the cursor already is.
 *
 * This is the one part that needs a real DOM, so it is tested only through a fake view that
 * enforces the same rule (see `typewriter.test.ts`) and by running the app.
 */
function centerCursor(view: EditorView): void {
  view.requestMeasure({
    key: centeringKey,
    read: readMetrics,
    write: (metrics, writingView) => {
      if (metrics) writingView.scrollDOM.scrollTop = centeredScrollTarget(metrics);
    },
  });
}

/** What the plugin does on each update; a function of its own so a test can call it without a
 * `ViewPlugin` instance. */
export function centerOnUpdate(update: Pick<ViewUpdate, 'docChanged' | 'selectionSet' | 'view'>): void {
  if (update.docChanged || update.selectionSet) centerCursor(update.view);
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
        centerOnUpdate(update);
      }
    },
  );
}

/** Typewriter mode as a CodeMirror extension, on or off. `setup.ts` holds this behind a
 * `Compartment` so a toggle mid-session does not need to rebuild the editor. */
export function typewriterModeExtension(enabled: boolean) {
  return enabled ? [typewriterModePlugin()] : [];
}
