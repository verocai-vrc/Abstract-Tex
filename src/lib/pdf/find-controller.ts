// The PDF pane's find controller: pdf.js's `PDFFindController`, with the two things it does to the view
// taken over. Everything else (extracting the text, matching, counting, highlighting) is pdf.js's own.
//
// What pdf.js does by itself when a match is selected, and why neither is good enough here:
//  - the match's page is brought into view with `linkService.page = n`, a jump to the top of that page;
//  - once the page is drawn and the match highlighted, the text layer calls `scrollMatchIntoView`, and
//    the default is `element.scrollIntoView({ block: 'start' })`: the matching line flush against the top
//    edge of the pane, with the sentence around it out of sight.
// So the second is replaced by a callback that decides where the view goes (`scrollToReveal`, which keeps
// a third of the pane above the match). And both can be switched off, because a search is also re-run
// when a build replaces the PDF, and the reader who is on page 40 must stay on page 40.

import { PDFFindController } from 'pdfjs-dist/web/pdf_viewer.mjs';
import type { EventBus, PDFLinkService } from 'pdfjs-dist/web/pdf_viewer.mjs';

export class PaneFindController extends PDFFindController {
  /** Shared with the link service handed to pdf.js, which is made before `this` exists. */
  private readonly stillness: { holdStill: boolean };

  /**
   * `reveal` is told which highlighted element to show: the pane scrolls itself to it.
   */
  constructor(
    options: { eventBus: EventBus; linkService: PDFLinkService },
    private readonly reveal: (match: HTMLElement) => void,
  ) {
    const stillness = { holdStill: false };
    // pdf.js moves to a match's page by assigning the link service's `page`. This stand-in passes every
    // read through, and drops that one write while the controller is holding still.
    const guardedLinkService = {
      get pagesCount() {
        return options.linkService.pagesCount;
      },
      get page() {
        return options.linkService.page;
      },
      set page(pageNumber: number) {
        if (!stillness.holdStill) options.linkService.page = pageNumber;
      },
    } as PDFLinkService;
    super({ eventBus: options.eventBus, linkService: guardedLinkService });
    this.stillness = stillness;
  }

  /** While true, finding a match moves nothing: the highlights appear and the count is kept, and the
   * view stays where the reader left it. Turn it off before the reader's own next/previous. */
  holdStill(still: boolean): void {
    this.stillness.holdStill = still;
  }

  override scrollMatchIntoView({
    element,
    pageIndex,
    matchIndex,
  }: {
    element: HTMLElement;
    pageIndex: number;
    matchIndex: number;
  }): void {
    if (!element) return;
    // pdf.js's own method decides whether this is the match to scroll to (the selected one, once per
    // search) and then calls `scrollIntoView` on the element. Handing it a stand-in element whose
    // `scrollIntoView` does our scrolling keeps that decision in pdf.js and changes only what happens.
    const standIn = {
      scrollIntoView: () => {
        if (!this.stillness.holdStill) this.reveal(element);
      },
    } as unknown as HTMLElement;
    super.scrollMatchIntoView({ element: standIn, pageIndex, matchIndex });
  }
}
