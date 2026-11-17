// Paste-to-cite (S7.6, DESIGN.md §5.4): pasting a DOI, arXiv id, or ISBN offers "Cite" instead of
// inserting the pasted text. The `paste` DOM event must be answered synchronously — calling
// `preventDefault()` after an `await` is too late, CodeMirror's own paste handling has already
// run — so `identify` here is a plain TypeScript port of `texbib::acquire::identify`'s shape
// matching, used only to decide *whether* to intercept. The actual fetch, dedup, and file write
// stay server-side (`ipc.pasteCite`), which re-runs the real `identify` anyway; this copy can
// afford to be slightly permissive; texbib's own answer is what's authoritative once a request
// is actually made.

import { StateEffect, StateField, type ChangeSet } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';

export type PasteKind = 'doi' | 'arxiv' | 'isbn';

export interface Identified {
  kind: PasteKind;
  normalized: string;
}

/** `texbib::acquire::doi::normalize_doi`, ported: strip a known `doi.org`/`dx.doi.org` URL or a
 * `doi:` scheme, case-insensitively; anything else passes through unchanged. */
function normalizeDoi(pasted: string): string {
  const prefixes = ['https://doi.org/', 'http://doi.org/', 'https://dx.doi.org/', 'http://dx.doi.org/', 'doi:'];
  const trimmed = pasted.trim();
  for (const prefix of prefixes) {
    if (trimmed.slice(0, prefix.length).toLowerCase() === prefix) {
      return trimmed.slice(prefix.length).trim();
    }
  }
  return trimmed;
}

/** `texbib::acquire::identify::is_doi_shape`, ported: one word shaped `10.<registrant>/<suffix>`, the
 * registrant being digits with optional `.`-separated sub-divisions and a first group of at least
 * four. A paste that only *begins* `10.` ("10. Conclusion", "10.5 mg was…") is text, and swallowing
 * it would send all of it to doi.org and lose it (review finding R8.18). Kept identical to the Rust
 * rule: `crates/texbib/tests/identify-cases.json` is checked against both. */
function isDoiShape(doi: string): boolean {
  return !/\s/.test(doi) && /^10\.\d{4,}(?:\.\d+)*\/./.test(doi);
}

/** `texbib::acquire::arxiv::normalize_arxiv_id`, ported. */
function normalizeArxivId(pasted: string): string {
  const prefixes = ['https://arxiv.org/abs/', 'http://arxiv.org/abs/', 'https://arxiv.org/pdf/', 'http://arxiv.org/pdf/', 'arxiv:'];
  const trimmed = pasted.trim();
  for (const prefix of prefixes) {
    if (trimmed.slice(0, prefix.length).toLowerCase() === prefix) {
      const rest = trimmed.slice(prefix.length).trim();
      return rest.endsWith('.pdf') ? rest.slice(0, -4) : rest;
    }
  }
  return trimmed;
}

/** `texbib::acquire::isbn::normalize_isbn`, ported: digits and a possible trailing `X` only. */
function normalizeIsbn(pasted: string): string {
  return pasted
    .trim()
    .split('')
    .filter((c) => !/\s/.test(c) && c !== '-')
    .map((c) => c.toUpperCase())
    .join('');
}

/** `texbib::acquire::arxiv::strip_version_suffix`, ported. */
function stripVersionSuffix(id: string): string {
  const match = id.match(/^(.*)v(\d+)$/);
  return match ? match[1]! : id;
}

/** `texbib::acquire::identify::is_arxiv_id_shape`, ported: `YYMM.NNNNN[vN]` or
 * `archive/YYMMNNN[vN]`. */
function isArxivIdShape(id: string): boolean {
  const withoutVersion = stripVersionSuffix(id);
  const dot = withoutVersion.indexOf('.');
  if (dot !== -1) {
    const yymm = withoutVersion.slice(0, dot);
    const rest = withoutVersion.slice(dot + 1);
    return yymm.length === 4 && /^\d{4}$/.test(yymm) && rest.length > 0 && rest.length <= 5 && /^\d+$/.test(rest);
  }
  const slash = withoutVersion.indexOf('/');
  if (slash !== -1) {
    const archive = withoutVersion.slice(0, slash);
    const digits = withoutVersion.slice(slash + 1);
    return archive.length > 0 && /^[A-Za-z.-]+$/.test(archive) && digits.length === 7 && /^\d+$/.test(digits);
  }
  return false;
}

/** `texbib::acquire::identify::looks_like_isbn`, ported: 10 or 13 digits, the 10-digit form
 * allowing a trailing `X` check digit. */
function looksLikeIsbn(normalized: string): boolean {
  if (normalized.length === 10) return /^\d{9}[\dX]$/.test(normalized);
  if (normalized.length === 13) return /^\d{13}$/.test(normalized);
  return false;
}

/** `texbib::acquire::identify::identify`, ported: which of the three a pasted string looks like,
 * or `null` for plain text (a sentence, a title, an empty paste). Order matches the Rust side —
 * DOI, then arXiv, then ISBN — though the three shapes never actually collide. */
export function identify(pasted: string): Identified | null {
  const trimmed = pasted.trim();
  if (trimmed.length === 0) return null;

  const doi = normalizeDoi(trimmed);
  if (isDoiShape(doi)) return { kind: 'doi', normalized: doi };

  const arxiv = normalizeArxivId(trimmed);
  if (isArxivIdShape(arxiv)) return { kind: 'arxiv', normalized: arxiv };

  const isbn = normalizeIsbn(trimmed);
  if (looksLikeIsbn(isbn)) return { kind: 'isbn', normalized: isbn };

  return null;
}

/** What the editor asks of the rest of the app once a paste is recognised: resolve it (fetch,
 * dedup, maybe write a new `.bib` entry) and insert `\cite{key}` in place of the pasted text.
 * Shaped like `DefinitionRequester` — the whole job, not just an answer — because talking to IPC
 * and telling the author what happened both need more context (the document, a status message)
 * than this CodeMirror-facing file has any business holding. `view`, `from` and `to` are the
 * live editor and the selection *at paste time*, captured synchronously by the handler below so
 * the eventual insertion lands where the paste happened even if the cursor has since moved during
 * the network round trip. */
export type PasteCiteRequester = (pasted: string, view: EditorView, from: number, to: number) => Promise<void>;

/** Intercept a paste that looks like a DOI/arXiv id/ISBN and hand it to `request` instead of
 * letting CodeMirror insert the raw text. Returns `false` (event not handled) for everything
 * else, which is the vast majority of pastes and must cost nothing beyond the synchronous
 * `identify` check above — no network, no delay, ordinary paste behaviour untouched. */
export function pasteCiteHandler(request: PasteCiteRequester) {
  return (event: ClipboardEvent, view: EditorView): boolean => {
    const text = event.clipboardData?.getData('text/plain') ?? '';
    if (!identify(text)) return false;
    event.preventDefault();
    const { from, to } = view.state.selection.main;
    void request(text, view, from, to);
    return true;
  };
}

// ---- Keeping the place of a paste while its answer is awaited (review finding R6.19) -----------
//
// A DOI is fetched over the network, so the cite is inserted seconds after the paste. A position
// captured at paste time is a position in the document *as it was*: anything typed before it since
// then moves the spot, and CodeMirror does not map a position that a caller holds across
// transactions (only its own state does). So the place is kept in the editor's state, where every
// transaction maps it, and read back when the answer arrives.

/** Where a waiting paste is: the text it will replace (empty for a plain cursor). */
export interface PastePlace {
  from: number;
  to: number;
}

const startTracking = StateEffect.define<PastePlace & { id: number }>();
const stopTracking = StateEffect.define<number>();

/** Maps one place through a change. A cursor ends up after text typed at its own spot; a selection
 * is not widened by text typed at either edge; a range whose text was deleted collapses to a point. */
function mapPlace(place: PastePlace, changes: ChangeSet): PastePlace {
  const empty = place.from === place.to;
  const from = changes.mapPos(place.from, 1);
  const to = changes.mapPos(place.to, empty ? 1 : -1);
  return { from, to: Math.max(from, to) };
}

/** The editor extension that holds the places. `setup.ts` installs it together with the paste
 * handler; without it `trackPaste` still works, but keeps the place it was given. */
export const pasteTracking = StateField.define<ReadonlyMap<number, PastePlace>>({
  create: () => new Map(),
  update(places, transaction) {
    const mapped = transaction.docChanged && places.size > 0;
    const effects = transaction.effects.filter((effect) => effect.is(startTracking) || effect.is(stopTracking));
    if (!mapped && effects.length === 0) return places; // almost every transaction: nothing waiting
    const next = new Map<number, PastePlace>();
    for (const [id, place] of places) next.set(id, mapped ? mapPlace(place, transaction.changes) : place);
    for (const effect of effects) {
      if (effect.is(startTracking)) next.set(effect.value.id, { from: effect.value.from, to: effect.value.to });
      else if (effect.is(stopTracking)) next.delete(effect.value);
    }
    return next;
  },
});

export interface PasteTracker {
  /** Where the paste is now, in the document as it is now. */
  range(): PastePlace;
  /** Stop keeping it. Call when the answer has been used or has failed. */
  release(): void;
}

let nextTrackedPaste = 0;

/** Start keeping the place `from`..`to` of `view`, called synchronously from the paste event so no
 * edit can come between the paste and the first mapping. */
export function trackPaste(view: EditorView, from: number, to: number): PasteTracker {
  const id = ++nextTrackedPaste;
  view.dispatch({ effects: startTracking.of({ id, from, to }) });
  return {
    range() {
      const place = view.state.field(pasteTracking, false)?.get(id) ?? { from, to };
      const end = view.state.doc.length;
      return { from: Math.min(place.from, end), to: Math.min(place.to, end) };
    },
    release() {
      try {
        view.dispatch({ effects: stopTracking.of(id) });
      } catch {
        // The editor is gone (its tab was closed); there is nothing left to keep a place in.
      }
    },
  };
}
