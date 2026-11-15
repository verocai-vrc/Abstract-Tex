// Searching the text of the PDF: what the reader asked for, and what pdf.js's find controller has
// answered so far, in a shape the find bar can show. No DOM here, so it is tested against pdf.js's real
// controller on a real PDF (`find.test.ts`).
//
// The split of work. pdf.js's `PDFFindController` does the searching: it pulls the text of every page,
// finds the matches, numbers them, highlights the ones on pages that are drawn, and moves between them.
// It is driven entirely through the viewer's event bus — a "find" event in, "update…" events out — and
// this class is the thin layer that speaks that protocol, so the rest of the app never has to.

import type { EventBus } from 'pdfjs-dist/web/pdf_viewer.mjs';

/** The two switches beside the search box. Accents are always treated as the same letter: nobody
 * searching a LaTeX paper for "Erdos" wants to miss "Erdős". */
export interface FindOptions {
  caseSensitive: boolean;
  wholeWord: boolean;
}

/** What the reader is told about their search. */
export type FindStatus =
  /** Nothing is being searched for. */
  | { kind: 'idle' }
  /** Asked, not answered yet; a long document answers a page at a time. */
  | { kind: 'searching' }
  | { kind: 'none' }
  /** `current` is the match shown (1-based) out of `total`; `wrapped` means that moving on or back came
   * round the end of the document. */
  | { kind: 'found'; current: number; total: number; wrapped: boolean };

/** What the find bar shows next to the search box. */
export function describeFind(status: FindStatus): string {
  switch (status.kind) {
    case 'idle':
      return '';
    case 'searching':
      return 'Searching…';
    case 'none':
      return 'No matches';
    case 'found': {
      if (status.total === 1) return '1 match';
      const position = `${status.current} of ${status.total}`;
      if (!status.wrapped) return position;
      // Coming round to the first match means going forward off the end; to the last, backward off the start.
      return status.current === 1 ? `${position} · from the start` : `${position} · from the end`;
    }
  }
}

/** pdf.js's own numbering of how a search is going (`FindState` in `pdf_find_controller`). Spelt out
 * here, not imported, so this file loads without pdf.js's browser-only viewer module. */
const PDFJS_FOUND = 0;
const PDFJS_NOT_FOUND = 1;
const PDFJS_WRAPPED = 2;
const PDFJS_PENDING = 3;

/** What pdf.js reports about the matches. `current` is 1-based, and 0 while there is no current one. */
interface MatchesCount {
  current: number;
  total: number;
}

/** The part of pdf.js's event bus this file uses. */
type FindEventBus = Pick<EventBus, 'dispatch' | 'on'>;

/** One search, as asked. */
export interface FindRequest {
  query: string;
  options: FindOptions;
}

/**
 * The search of one open PDF. Create one beside the `PDFFindController` that shares its event bus; it
 * calls `onStatus` whenever what the reader should be told changes.
 */
export class FindSession {
  private latest: FindStatus = { kind: 'idle' };
  private asked: FindRequest | null = null;

  constructor(
    private readonly eventBus: FindEventBus,
    private readonly onStatus: (status: FindStatus) => void,
  ) {
    // The controller announces how a search is going in two events: the state (found, wrapped, none,
    // pending) and, as it works through the pages, the running count of matches.
    eventBus.on('updatefindcontrolstate', (event: { state: number; matchesCount: MatchesCount }) => {
      this.onControllerState(event.state, event.matchesCount);
    });
    eventBus.on('updatefindmatchescount', (event: { matchesCount: MatchesCount }) => {
      this.onMatchesCount(event.matchesCount);
    });
  }

  get status(): FindStatus {
    return this.latest;
  }

  /** The search as last asked, or null when there is none. A rebuild opens a new PDF with a new session;
   * this is how it repeats the search there. */
  get request(): FindRequest | null {
    return this.asked;
  }

  /** Search for `query`, highlighting every match. Called on each keystroke: pdf.js waits a moment for
   * the typing to stop before it starts. An empty query is no search. */
  search(query: string, options: FindOptions): void {
    if (query === '') {
      this.close();
      return;
    }
    this.asked = { query, options };
    this.setStatus({ kind: 'searching' });
    this.dispatchFind('', false);
  }

  /** Move to the next match; after the last, the first. */
  next(): void {
    if (this.asked) this.dispatchFind('again', false);
  }

  /** Move to the previous match; before the first, the last. */
  previous(): void {
    if (this.asked) this.dispatchFind('again', true);
  }

  /** End the search: the highlights go and the bar has nothing to say. */
  close(): void {
    this.asked = null;
    this.setStatus({ kind: 'idle' });
    this.eventBus.dispatch('findbarclose', { source: this });
  }

  private dispatchFind(type: '' | 'again', backwards: boolean): void {
    if (!this.asked) return;
    this.eventBus.dispatch('find', {
      source: this,
      // '' is "the query changed" (pdf.js debounces it); 'again' is "go to the next one".
      type,
      query: this.asked.query,
      caseSensitive: this.asked.options.caseSensitive,
      entireWord: this.asked.options.wholeWord,
      highlightAll: true,
      findPrevious: backwards,
      matchDiacritics: false,
    });
  }

  private onControllerState(state: number, matches: MatchesCount): void {
    // Events that arrive after the search was closed are the controller tidying up, not news.
    if (!this.asked) return;
    if (state === PDFJS_PENDING) {
      this.setStatus({ kind: 'searching' });
    } else if (state === PDFJS_NOT_FOUND) {
      this.setStatus({ kind: 'none' });
    } else if (state === PDFJS_FOUND || state === PDFJS_WRAPPED) {
      this.showCount(matches, state === PDFJS_WRAPPED);
    }
  }

  private onMatchesCount(matches: MatchesCount): void {
    if (!this.asked) return;
    // A count with no match in it says nothing yet; leave "searching" or "no matches" as they are.
    if (this.latest.kind !== 'found' && this.latest.kind !== 'searching') return;
    this.showCount(matches, this.latest.kind === 'found' && this.latest.wrapped);
  }

  private showCount(matches: MatchesCount, wrapped: boolean): void {
    // pdf.js reports 0 of 0 while it has not got as far as a match; that is still "searching".
    if (matches.total === 0 || matches.current === 0) {
      this.setStatus({ kind: 'searching' });
      return;
    }
    this.setStatus({ kind: 'found', current: matches.current, total: matches.total, wrapped });
  }

  private setStatus(status: FindStatus): void {
    const unchanged = JSON.stringify(status) === JSON.stringify(this.latest);
    this.latest = status;
    if (!unchanged) this.onStatus(status);
  }
}

/** The longest selection that is a search term: past this it is a passage, not a word or a phrase. */
const LONGEST_SEED = 100;

/**
 * What to put in the search box when it opens over a selection (the way a browser does): the selected
 * words on one line, or nothing when there is no selection or it is too long to be something searched
 * for.
 */
export function searchSeed(selected: string): string {
  const oneLine = selected.replace(/\s+/g, ' ').trim();
  return oneLine.length > LONGEST_SEED ? '' : oneLine;
}
