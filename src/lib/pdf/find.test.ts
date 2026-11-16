// Searching the text of a PDF, with pdf.js's real find controller on a real (tiny) PDF.
//
// Nothing is mocked: the PDF is real bytes, the text is extracted by the real parser, and the matches are
// the real controller's. `pdf-test-support.ts` explains how pdf.js's browser-only viewer is made importable.

import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { PDFDocumentProxy } from 'pdfjs-dist';
import type { EventBus, PDFFindController } from 'pdfjs-dist/web/pdf_viewer.mjs';
import { describeFind, FindSession, searchSeed, type FindOptions, type FindStatus } from './find';
import { loadPdfJs, makePdf, type PdfJs } from './pdf-test-support';

let pdfJs: PdfJs;
let PaneFindController: typeof import('./find-controller').PaneFindController;

beforeAll(async () => {
  pdfJs = await loadPdfJs();
  // The controller the pane really uses, imported after pdf.js is loaded (it extends a pdf.js class).
  ({ PaneFindController } = await import('./find-controller'));
});

/** pdf.js's find controller on `pdf`, and a `FindSession` talking to it over a shared event bus. */
async function openSearch(pdf: Uint8Array) {
  const document: PDFDocumentProxy = await pdfJs.getDocument({ data: pdf, verbosity: 0 }).promise;
  const eventBus: EventBus = new pdfJs.web.EventBus();
  const controller: PDFFindController = new PaneFindController(
    { eventBus, linkService: { pagesCount: document.numPages, page: 1 } as never },
    () => {},
  );
  controller.setDocument(document);
  const statuses: FindStatus[] = [];
  const session = new FindSession(eventBus, (status) => statuses.push(status));
  return { document, eventBus, controller, session, statuses };
}

const INSENSITIVE: FindOptions = { caseSensitive: false, wholeWord: false };

const FOX_PAGES = [
  ['The quick brown fox jumps over', 'the lazy dog and a second fox.'],
  ['Foxes are not foxglove, and a Fox is not a fox.'],
];

describe('searching a PDF', () => {
  it('counts every match on every page', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', INSENSITIVE);
    // Page 1: two. Page 2: "Foxes", "foxglove", "Fox", "fox" — four. Six in all.
    await vi.waitFor(() => expect(session.status).toEqual({ kind: 'found', current: 1, total: 6, wrapped: false }));
  });

  it('can be told to care about capitals', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('Fox', { caseSensitive: true, wholeWord: false });
    // "Foxes" and "Fox" only.
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', total: 2 }));
  });

  it('can be told to match whole words only', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', { caseSensitive: false, wholeWord: true });
    // Not "Foxes" and not "foxglove": the two on page 1, and "Fox" and "fox" on page 2.
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', total: 4 }));
  });

  it('moves to the next match, and starts again from the first after the last', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', { caseSensitive: false, wholeWord: true });
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', current: 1, total: 4 }));

    session.next();
    await vi.waitFor(() => expect(session.status).toMatchObject({ current: 2, total: 4 }));
    session.next();
    session.next();
    await vi.waitFor(() => expect(session.status).toMatchObject({ current: 4, total: 4 }));
    session.next();
    await vi.waitFor(() => expect(session.status).toEqual({ kind: 'found', current: 1, total: 4, wrapped: true }));
  });

  it('moves to the previous match, and to the last from the first', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', { caseSensitive: false, wholeWord: true });
    await vi.waitFor(() => expect(session.status).toMatchObject({ current: 1, total: 4 }));

    session.previous();
    await vi.waitFor(() => expect(session.status).toEqual({ kind: 'found', current: 4, total: 4, wrapped: true }));
    session.previous();
    await vi.waitFor(() => expect(session.status).toMatchObject({ current: 3, total: 4 }));
  });

  it('finds words with punctuation in them: hyphens, dots, brackets, plus signs', async () => {
    // pdf.js builds its pattern with RegExp.escape for these, which a webview older than 2025 lacks.
    const pages = [['A non-trivial e-mail from U.S.A. about C++ and O(n) at [12] for $5.', 'Also well-known.']];
    for (const query of ['non-trivial', 'e-mail', 'U.S.A.', 'C++', 'O(n)', '[12]', '$5', 'well-known']) {
      const { session } = await openSearch(makePdf(pages));
      session.search(query, INSENSITIVE);
      await vi.waitFor(() => expect(session.status, query).toMatchObject({ kind: 'found', total: 1 }));
    }
  });

  it('says "no matches" for a query with punctuation that matches nothing, rather than searching for ever', async () => {
    const { session } = await openSearch(makePdf([['Nothing of the kind here.']]));
    session.search('non-trivial', INSENSITIVE);
    await vi.waitFor(() => expect(session.status).toEqual({ kind: 'none' }));
  });

  it('says so when nothing matches', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('wolf', INSENSITIVE);
    await vi.waitFor(() => expect(session.status).toEqual({ kind: 'none' }));
  });

  it('reports "searching" at first rather than a count it does not have yet', async () => {
    const { session, statuses } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', INSENSITIVE);
    expect(session.status).toEqual({ kind: 'searching' });
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));
    expect(statuses[0]).toEqual({ kind: 'searching' });
  });

  it('treats an empty query as no search at all', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', INSENSITIVE);
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));
    session.search('', INSENSITIVE);
    expect(session.status).toEqual({ kind: 'idle' });
  });

  it('stops highlighting when closed', async () => {
    const { session, controller } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', INSENSITIVE);
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));
    expect(controller.highlightMatches).toBe(true);

    session.close();
    expect(session.status).toEqual({ kind: 'idle' });
    await vi.waitFor(() => expect(controller.highlightMatches).toBe(false));
  });

  it('remembers what was searched for, so a rebuilt PDF can be searched the same way', async () => {
    const first = await openSearch(makePdf(FOX_PAGES));
    expect(first.session.request).toBeNull();
    first.session.search('fox', { caseSensitive: false, wholeWord: true });
    expect(first.session.request).toEqual({ query: 'fox', options: { caseSensitive: false, wholeWord: true } });

    // A build produced a new PDF with one more match. The pane opens it with a new controller and a new
    // session, and asks the new session to search for what the old one was searching for.
    const rebuilt = await openSearch(makePdf([...FOX_PAGES, ['One more fox.']]));
    const { query, options } = first.session.request!;
    rebuilt.session.search(query, options);
    await vi.waitFor(() => expect(rebuilt.session.status).toMatchObject({ kind: 'found', total: 5 }));
  });

  it('forgets the search once it is closed or emptied', async () => {
    const { session } = await openSearch(makePdf(FOX_PAGES));
    session.search('fox', INSENSITIVE);
    session.close();
    expect(session.request).toBeNull();
    session.search('fox', INSENSITIVE);
    session.search('', INSENSITIVE);
    expect(session.request).toBeNull();
  });

  it('does nothing when asked to move on without a search', async () => {
    const { session, statuses } = await openSearch(makePdf(FOX_PAGES));
    session.next();
    session.previous();
    expect(session.status).toEqual({ kind: 'idle' });
    expect(statuses).toEqual([]);
  });
});

describe('the words under the search box', () => {
  it('says nothing when there is no search', () => {
    expect(describeFind({ kind: 'idle' })).toBe('');
  });

  it('says it is searching', () => {
    expect(describeFind({ kind: 'searching' })).toBe('Searching…');
  });

  it('says there are no matches', () => {
    expect(describeFind({ kind: 'none' })).toBe('No matches');
  });

  it('says which match is shown out of how many', () => {
    expect(describeFind({ kind: 'found', current: 3, total: 12, wrapped: false })).toBe('3 of 12');
  });

  it('says when a search came round to the start again', () => {
    expect(describeFind({ kind: 'found', current: 1, total: 12, wrapped: true })).toBe('1 of 12 · from the start');
    expect(describeFind({ kind: 'found', current: 12, total: 12, wrapped: true })).toBe('12 of 12 · from the end');
  });

  it('says "1 match" in the singular', () => {
    expect(describeFind({ kind: 'found', current: 1, total: 1, wrapped: false })).toBe('1 match');
  });
});

describe('searchSeed', () => {
  it('starts the search from words the reader had selected', () => {
    expect(searchSeed('Hausdorff')).toBe('Hausdorff');
  });

  it('turns the line break a wrapped selection has into a space', () => {
    expect(searchSeed('the quick\nbrown   fox ')).toBe('the quick brown fox');
  });

  it('starts from nothing when nothing, or only space, was selected', () => {
    expect(searchSeed('')).toBe('');
    expect(searchSeed(' \n\t ')).toBe('');
  });

  it('does not start a search from a whole paragraph', () => {
    // Someone who selected a paragraph and pressed Ctrl F wants the box, not a search for the paragraph.
    expect(searchSeed('word '.repeat(60))).toBe('');
  });
});
