// The bibliography index as the frontend sees it (S7.2): one reactive snapshot of what
// `src-tauri/src/bibliography.rs` built from disk, plus the two lookups every consumer of it
// wants — entries by key (S7.3's completion, S7.6's dedup) and citations by key (S8.3's "never
// cited" / "undefined citation" checks).
//
// Kept apart from `state.svelte.ts` on purpose: `app` is what the three panes render, and the
// index is data those panes consult rather than show. Only the controller writes `index`, the
// same rule `app` follows. The pure helpers below take the index as an argument rather than
// reading the store, so `bibliography.test.ts` exercises them with a literal.

import type { BibEntrySummary, BibliographyIndex, Citation, ZoteroStatus } from './ipc';

/** Entries keyed by citation key. A key defined twice keeps its *first* definition, which is
 * what BibTeX itself does; the index still lists both for S8.3 to complain about. */
export function entriesByKey(index: BibliographyIndex | null): Map<string, BibEntrySummary> {
  const map = new Map<string, BibEntrySummary>();
  if (!index) return map;
  for (const entry of index.entries) {
    if (!map.has(entry.key)) map.set(entry.key, entry);
  }
  return map;
}

/** Every place each key is cited, in document order. */
export function citationsByKey(index: BibliographyIndex | null): Map<string, Citation[]> {
  const map = new Map<string, Citation[]>();
  if (!index) return map;
  for (const citation of index.citations) {
    const list = map.get(citation.key);
    if (list) list.push(citation);
    else map.set(citation.key, [citation]);
  }
  return map;
}

class BibliographyState {
  /** `null` until the first index arrives after a folder opens. `$state.raw` because the index
   * is replaced whole on every change and never edited in place — deep reactivity over a few
   * thousand entries would be cost for nothing. */
  index = $state.raw<BibliographyIndex | null>(null);

  entries = $derived(entriesByKey(this.index));
  citations = $derived(citationsByKey(this.index));
  /** Files the document names that are not on disk — the first thing worth telling the author
   * about a bibliography, and the reason a missing file is still listed by path. */
  missingFiles = $derived((this.index?.files ?? []).filter((file) => !file.exists).map((file) => file.path));

  /** `null` until the author asks (S8.1): detection is a one-shot probe, not a background poll,
   * so there is nothing to show before that first ask. */
  zoteroStatus = $state<ZoteroStatus | null>(null);
}

export const bibliography = new BibliographyState();
