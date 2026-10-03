// What is left to fill in (S11.12): the `% FILL IN:` lines of every `.tex` file in the project, and
// where the cursor is. Only the controller writes to `placeholders`, as with the other stores; the
// finding and ordering logic is in `editor/placeholders.ts` so it tests without a Svelte runtime.

class PlaceholderState {
  /** Marker lines by project-relative path. Replaced whole, never edited, like the other lists; a
   * file with none has no entry. */
  byFile = $state.raw<ReadonlyMap<string, readonly number[]>>(new Map());
  /** The 1-based line the active tab's cursor is on. */
  cursorLine = $state(1);

  total = $derived.by(() => {
    let count = 0;
    for (const lines of this.byFile.values()) count += lines.length;
    return count;
  });
}

export const placeholders = new PlaceholderState();
