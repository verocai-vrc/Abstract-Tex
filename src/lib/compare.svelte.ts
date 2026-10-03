// Comparing two commits (S11.4d, DESIGN.md §5.7): which Graph rows are marked, and what the PDF
// pane and the drawer show while a comparison is on screen.
//
// Two things live here and they are kept apart on purpose. The *marking* rules and the *event
// folding* are plain functions over plain objects, so `compare.test.ts` drives them with literals
// and no Svelte runtime. The `compare` store at the bottom only holds their results; as with
// `git` and `app`, only the controller writes to it.

import type { CompileEvent, Diagnostic } from './ipc';

/** The two rows the author has clicked. Which is *from* and which is *to* here is click order and
 * nothing more: Rust puts the pair older → newer whatever order it arrives in (A4), and the
 * toolbar shows the pair as Rust answered, never as clicked. */
export interface Marks {
  from: string | null;
  to: string | null;
}

export const NO_MARKS: Marks = { from: null, to: null };

/** What one click or `Enter` did: the new marks, and the pair to compare if that click completed
 * one. */
export interface MarkStep {
  marks: Marks;
  compare: [from: string, to: string] | null;
}

/**
 * The click-to-mark state machine (design interview A10):
 *
 * - nothing marked → the row becomes *from*;
 * - *from* marked, another row clicked → it becomes *to*, and that is a comparison to start;
 * - *from* marked, the same row clicked → unmarked;
 * - both marked, a marked row clicked → that mark is removed and the other stays, as *from*;
 * - both marked, any other row clicked → a new *from*; the old pair is forgotten.
 *
 * The first two give "second click renders immediately"; the last gives "a third click starts
 * over". Pure, so the test does not need a DOM.
 */
export function markRow(marks: Marks, id: string): MarkStep {
  if (marks.from === null) return { marks: { from: id, to: null }, compare: null };

  if (marks.to === null) {
    if (marks.from === id) return { marks: NO_MARKS, compare: null };
    return { marks: { from: marks.from, to: id }, compare: [marks.from, id] };
  }

  if (marks.from === id) return { marks: { from: marks.to, to: null }, compare: null };
  if (marks.to === id) return { marks: { from: marks.from, to: null }, compare: null };
  return { marks: { from: id, to: null }, compare: null };
}

/** Which mark, if any, a row carries — what the row's style and its screen-reader label read. */
export function markOf(marks: Marks, id: string): 'from' | 'to' | null {
  if (marks.from === id) return 'from';
  if (marks.to === id) return 'to';
  return null;
}

/**
 * The comparison on screen, in the one place both the PDF pane and the drawer read it.
 *
 * - `idle`: no comparison; the live PDF and the live diagnostics are showing.
 * - `building`: asked for, not finished. `generation` is `null` until Rust's answer arrives — the
 *   answer names the generation whose `compile-diff` events belong to this comparison.
 * - `ready`: `pdfUrl` is the marked-up PDF.
 * - `failed`: it could not be built; `message` is the sentence, `diagnostics` the diff build's own.
 *
 * Everything except `idle` is "diff mode": the pane shows the banner and the PDF pane stops
 * answering SyncTeX clicks, because no line of the marked-up document is one the author can edit.
 */
export interface ComparisonView {
  phase: 'idle' | 'building' | 'ready' | 'failed';
  /** Full commit ids, older → newer, as Rust ordered them. Empty strings while `idle`, and while
   * `building` before the answer says which is which. */
  older: string;
  newer: string;
  generation: number | null;
  pdfUrl: string | null;
  message: string | null;
  diagnostics: Diagnostic[];
  stderr: string;
  /** The engine's latest progress line, shown in the banner while building. */
  progress: string | null;
}

export const NO_COMPARISON: ComparisonView = {
  phase: 'idle',
  older: '',
  newer: '',
  generation: null,
  pdfUrl: null,
  message: null,
  diagnostics: [],
  stderr: '',
  progress: null,
};

/** The state right after asking: nothing known yet but that something is being built. */
export function startedComparison(): ComparisonView {
  return { ...NO_COMPARISON, phase: 'building' };
}

/** Whether the comparison is the thing on screen, rather than the live build. */
export function inDiffMode(view: ComparisonView): boolean {
  return view.phase !== 'idle';
}

/**
 * Fold one `compile-diff` event into the view.
 *
 * Only events of the comparison's own generation count (S11.4c's note for this loop): the diff
 * lane cancels an older comparison, and an event from it can still arrive after a newer one was
 * asked for. `toUrl` turns a PDF path into the webview's asset URL — a parameter, because that is
 * `ipc.assetUrl` and this module imports nothing from Tauri.
 *
 * `draft` is ignored: a comparison never asks for one (S9.9's drafts belong to the live lane).
 */
export function foldDiffEvent(
  view: ComparisonView,
  event: CompileEvent,
  toUrl: (absolutePath: string) => string,
): ComparisonView {
  if (view.phase !== 'building' || view.generation === null || event.generation !== view.generation) return view;

  switch (event.status) {
    case 'progress':
      return { ...view, progress: event.message };
    case 'finished':
      if (event.success && event.pdfPath) {
        // The query string defeats the webview's cache, as the live PDF's does.
        return { ...view, phase: 'ready', pdfUrl: `${toUrl(event.pdfPath)}?v=${event.generation}`, progress: null };
      }
      return {
        ...view,
        phase: 'failed',
        message:
          event.diagnostics.length > 0
            ? 'The comparison could not be built. latexdiff’s markup does not suit every document, and these problems are in its marked-up copy, not in yours.'
            : 'The comparison could not be built, and TeX did not say why.',
        diagnostics: event.diagnostics,
        stderr: event.stderr,
        progress: null,
      };
    case 'failed':
      return { ...view, phase: 'failed', message: event.message, progress: null };
    default:
      return view;
  }
}

/** `a1b2c3d (3 days ago)` — the banner's wording for one end of the pair. `relative` is the
 * caller's `relativeTime`, passed in so this stays a function of its arguments. */
export function describeEnd(shortId: string, relative: string | null): string {
  return relative ? `${shortId} (${relative})` : shortId;
}

class CompareState {
  /** The marks on the Graph rows. Replaced whole on every click, never edited in place. */
  marks = $state.raw<Marks>(NO_MARKS);
  /** The comparison, as above. */
  view = $state.raw<ComparisonView>(NO_COMPARISON);
  /** Why a comparison was refused before anything was written — no `latexdiff`, or a commit
   * without the root file — drawn under the Graph toolbar (A12). */
  refusal = $state<string | null>(null);
  /** Why *Save as…* did not write a file, shown beside the button. */
  saveError = $state<string | null>(null);

  mode = $derived(inDiffMode(this.view));
}

export const compare = new CompareState();
