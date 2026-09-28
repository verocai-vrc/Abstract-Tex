// The bibliography index as the frontend sees it (S7.2): one reactive snapshot of what
// `src-tauri/src/bibliography.rs` built from disk, plus the two lookups every consumer of it
// wants — entries by key (S7.3's completion, S7.6's dedup) and citations by key (S8.3's "never
// cited" / "undefined citation" checks).
//
// Kept apart from `state.svelte.ts` on purpose: `app` is what the three panes render, and the
// index is data those panes consult rather than show. Only the controller writes `index`, the
// same rule `app` follows. The pure helpers below take the index as an argument rather than
// reading the store, so `bibliography.test.ts` exercises them with a literal.

import type { BibEntrySummary, BibliographyIndex, Citation, Finding, ZoteroLibrary, ZoteroStatus } from './ipc';

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

/** The 1-based line a `Finding.jump`'s `BibEntry` byte offset falls on, within `text` (the
 * `.bib` file's own contents). `Span.start`/`end` (`crates/texbib/src/parse.rs`) are *byte*
 * offsets — BibTeX files are commonly UTF-8 with non-ASCII author names, so a JS string index
 * (UTF-16 code units) cannot be compared to one directly. `TextEncoder` re-encodes the text once
 * and counts `\n` bytes (0x0A, which never appears as a continuation byte in UTF-8) up to the
 * offset — the same reasoning `crates/abstract-tex-reconcile` uses on the Rust side of this same
 * byte/unit boundary, just going the other way. */
export function lineAtByteOffset(text: string, byteOffset: number): number {
  const bytes = new TextEncoder().encode(text);
  let line = 1;
  const end = Math.min(byteOffset, bytes.length);
  for (let i = 0; i < end; i++) {
    if (bytes[i] === 0x0a) line++;
  }
  return line;
}

/** One file's worth of findings, for `BibliographyHealth.svelte`'s per-file sections. */
export interface FindingGroup {
  file: string;
  findings: Finding[];
}

/** Every finding grouped by file, in the order they first appear — `Drawer.svelte`'s
 * `groupDiagnostics` returns the same shape (an array, not a `Map`) for the same reason: a
 * Svelte `{#each}` over groups reads better than one over `Map` entries. */
export function findingsByFile(findings: Finding[]): FindingGroup[] {
  const groups: FindingGroup[] = [];
  for (const finding of findings) {
    const file = finding.jump.file;
    const existing = groups.find((group) => group.file === file);
    if (existing) existing.findings.push(finding);
    else groups.push({ file, findings: [finding] });
  }
  return groups;
}

/** The linked `.bib` files (S8.7: what "Unlink" can offer) — those listed because
 * `abstract-tex.toml` links them, not because the document names them. A file the document also names
 * is `named`, and unlinking it would change nothing the author could see, so it is not offered. */
export function linkedFiles(index: BibliographyIndex | null): string[] {
  return (index?.files ?? []).filter((file) => file.origin.kind === 'linked').map((file) => file.path);
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
  linkedFiles = $derived(linkedFiles(this.index));

  /** The five health checks (S8.3), refreshed alongside `index` (same trigger, separate backend
   * call — `bibliography_health` reparses rather than deriving from the index the frontend
   * already has, see `ipc.ts`). `[]` before the first project opens or when nothing is wrong —
   * `findingsGroups` tells those two apart by checking `index` instead. */
  findings = $state.raw<Finding[]>([]);

  findingsGroups = $derived(findingsByFile(this.findings));

  /** `null` until the author asks (S8.1): detection is a one-shot probe, not a background poll,
   * so there is nothing to show before that first ask. */
  zoteroStatus = $state<ZoteroStatus | null>(null);

  /** The libraries listed for the "link a collection" picker (S8.2). `null` before the author
   * opens it; cleared again once they pick a collection or close it, so a stale list is never
   * shown the next time. */
  zoteroLibraries = $state<ZoteroLibrary[] | null>(null);
}

export const bibliography = new BibliographyState();
