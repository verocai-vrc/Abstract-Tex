// Where the pane goes to show a search match, with pdf.js's real find controller on a real PDF.
//
// pdf.js moves the view for a match in two ways: it sets the link service's `page` (a jump to the top of
// the match's page) and the text layer calls `scrollMatchIntoView` with the highlighted element. The
// pane's controller takes over both: the second to put the match where the reader can see it with its
// context (`scrollToReveal`), and both to stand still when a rebuild re-runs a search, because a reader
// who is reading page 40 must not be taken to page 3 each time the document is rebuilt.

import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { EventBus } from 'pdfjs-dist/web/pdf_viewer.mjs';
import { FindSession } from './find';
import { loadPdfJs, makePdf, type PdfJs } from './pdf-test-support';

let pdfJs: PdfJs;
let PaneFindController: typeof import('./find-controller').PaneFindController;

beforeAll(async () => {
  pdfJs = await loadPdfJs();
  // After the stubs: this module extends a pdf.js class, so it needs pdf.js loaded when it is imported.
  ({ PaneFindController } = await import('./find-controller'));
});

const CASE_SENSITIVE = { caseSensitive: true, wholeWord: false };

/** Page 1 has one "fox", page 2 has two "Fox": searching for "Fox" with capitals first selects page 2. */
const PAGES = [['The quick brown fox.'], ['Fox one and Fox two.']];

async function openSearch() {
  const document = await pdfJs.getDocument({ data: makePdf(PAGES), verbosity: 0 }).promise;
  const eventBus: EventBus = new pdfJs.web.EventBus();
  // A link service that writes down every page it is told to go to; the reader is on page 1.
  const pagesGoneTo: number[] = [];
  const linkService = {
    pagesCount: document.numPages,
    get page() {
      return 1;
    },
    set page(pageNumber: number) {
      pagesGoneTo.push(pageNumber);
    },
  };
  const reveal = vi.fn();
  const controller = new PaneFindController({ eventBus, linkService: linkService as never }, reveal);
  controller.setDocument(document);
  const session = new FindSession(eventBus, () => {});
  /** What the text layer does for the highlighted match once its page is drawn. */
  const textLayerAsksToScroll = (matchOffset = 0) => {
    const element = { scrollIntoView: vi.fn() };
    const { pageIdx, matchIdx } = controller.selected!;
    controller.scrollMatchIntoView({
      element: element as unknown as HTMLElement,
      pageIndex: pageIdx,
      matchIndex: matchIdx + matchOffset,
    });
    return element;
  };
  return { controller, session, reveal, pagesGoneTo, textLayerAsksToScroll };
}

describe('the pane\'s find controller', () => {
  it('shows the selected match with the pane\'s own scrolling, not the browser\'s', async () => {
    const { session, reveal, textLayerAsksToScroll } = await openSearch();
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', total: 2 }));

    const element = textLayerAsksToScroll();
    expect(reveal).toHaveBeenCalledExactlyOnceWith(element);
    expect(element.scrollIntoView).not.toHaveBeenCalled();
  });

  it('moves the view once for a search, however many times the page is redrawn', async () => {
    const { session, reveal, textLayerAsksToScroll } = await openSearch();
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));

    textLayerAsksToScroll();
    textLayerAsksToScroll();
    expect(reveal).toHaveBeenCalledTimes(1);
  });

  it('ignores a match that is not the selected one', async () => {
    const { session, reveal, textLayerAsksToScroll } = await openSearch();
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));

    textLayerAsksToScroll(1);
    expect(reveal).not.toHaveBeenCalled();
  });

  it('takes the reader to the page the match is on', async () => {
    const { session, pagesGoneTo } = await openSearch();
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status.kind).toBe('found'));
    expect(pagesGoneTo).toContain(2);
  });

  it('stands still, moving the view nowhere, while it is told to hold still', async () => {
    const { controller, session, reveal, pagesGoneTo, textLayerAsksToScroll } = await openSearch();
    controller.holdStill(true);
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', total: 2 }));

    const element = textLayerAsksToScroll();
    expect(reveal).not.toHaveBeenCalled();
    expect(element.scrollIntoView).not.toHaveBeenCalled();
    expect(pagesGoneTo).toEqual([]);
  });

  it('moves the view again once the reader asks for another match', async () => {
    const { controller, session, reveal, textLayerAsksToScroll } = await openSearch();
    controller.holdStill(true);
    session.search('Fox', CASE_SENSITIVE);
    await vi.waitFor(() => expect(session.status).toMatchObject({ kind: 'found', current: 1 }));
    textLayerAsksToScroll();

    controller.holdStill(false);
    session.next();
    await vi.waitFor(() => expect(session.status).toMatchObject({ current: 2 }));
    const element = textLayerAsksToScroll();
    expect(reveal).toHaveBeenCalledExactlyOnceWith(element);
  });
});
