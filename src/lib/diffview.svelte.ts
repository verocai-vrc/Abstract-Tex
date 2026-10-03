// The side-by-side view's state (S11.7, design interview B8): which Changes row is open, and the
// two texts Rust answered with. Only the controller writes to `diffView`.

import type { DiffSides } from './ipc';

/** Whether a row gets the side-by-side view. `.tex` and `.bib` are what an author writes; anything
 * else opens as it always has (design interview B8). */
export function isDiffable(path: string): boolean {
  return /\.(tex|bib)$/i.test(path);
}

/** What the two panes are called, in the words of what *Stage* and *Commit* do — not "index". */
export function sideLabels(staged: boolean): { before: string; after: string } {
  return staged ? { before: 'Last commit', after: 'Staged' } : { before: 'Before staging', after: 'Your file' };
}

class DiffViewState {
  /** The row open in the view, or `null`. */
  open = $state.raw<{ path: string; staged: boolean; deleted: boolean } | null>(null);
  /** The two texts; `null` while Rust is reading them. */
  sides = $state.raw<DiffSides | null>(null);
  error = $state<string | null>(null);
}

export const diffView = new DiffViewState();
