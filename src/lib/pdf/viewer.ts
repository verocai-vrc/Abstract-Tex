// The preview pane's PDF viewer: pdf.js's own `PDFViewer`, in a container Svelte mounts and then leaves
// alone (CLAUDE.md: pdf.js owns its DOM subtree).
//
// Why pdf.js's viewer and not canvases of our own (review findings R5.4 and R5.5). The first version drew
// every page eagerly onto a canvas of its own: a 300-page thesis was 300 canvases in memory, the new PDF
// appeared only when the last page was done, and the PDF was a picture — no text to select, copy or
// search, and no working links, although DESIGN.md §4 chose pdf.js for exactly its text layer.
// `PDFViewer` draws only the pages near the window, keeps a text layer and a link layer on each, and
// does its own zoom.
//
// The pane must never flash or lose its place when a new PDF arrives (DESIGN.md §6). So a new PDF is
// opened in a second, hidden viewer in the same place; once the pages the reader is looking at are drawn
// there, it takes over and the old one is thrown away. If a newer PDF arrives meanwhile, the older one is
// dropped. A search the reader has open carries over: the new PDF is searched for the same words, with
// the highlights back and the view where it was.

import * as pdfjs from 'pdfjs-dist';
import type { PDFDocumentProxy } from 'pdfjs-dist';
import { AnnotationMode } from 'pdfjs-dist';
import { EventBus, PDFLinkService, PDFViewer, RenderingStates } from 'pdfjs-dist/web/pdf_viewer.mjs';
import 'pdfjs-dist/web/pdf_viewer.css';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?worker&url';
import { FindSession, type FindOptions, type FindRequest, type FindStatus } from './find';
import { PaneFindController } from './find-controller';
import { clampZoom, linkAction, pointsFromClick, scrollToReveal } from './geometry';

// The worker is handed to pdf.js as a live Worker object rather than as a URL, and which
// spelling we use to build it matters more than it looks.
//
// The bug this replaces: `?url` yields a root-absolute `/assets/…` path. Under `tauri dev`
// the page's origin is tauri://localhost while Vite serves from http://localhost:1420, so
// that path resolves against the wrong origin and the browser refuses to start a
// cross-origin worker — the PDF pane stays blank with an empty window.error.
//
// `new URL(…, import.meta.url)` is *not* a fix here either: Vite compiles it to
// `new URL('/assets/…', import.meta.url)`, and a root-absolute first argument discards the
// base, so it lands back on the page origin.
//
// `?worker&url` is the spelling that works: Vite bundles the file as a dedicated worker
// entry and hands back a URL it guarantees is loadable from the page, in dev and in a
// packaged build alike. `type: 'module'` is required — pdf.js 5 ships an ESM worker — and
// we set `workerPort` rather than `workerSrc` because we already hold the instance, and
// pdf.js would otherwise fetch a second copy of it.
const pdfWorker = new Worker(workerUrl, { type: 'module' });

pdfjs.GlobalWorkerOptions.workerPort = pdfWorker;

/** Have pdf.js draw its pages at the ratio `current` answers, in device pixels to a CSS pixel, and not at
 * the window's `devicePixelRatio`, which in WebKit does not have the zoom in it (S17.11). pdf.js asks its
 * own `OutputScale` each time, and has no option for this. What is already drawn needs a `redraw`. */
export function drawAtPixelRatio(current: () => number): void {
  Object.defineProperty(pdfjs.OutputScale, 'pixelRatio', { get: current, configurable: true });
}

/** The share of the pane's width a page fills at the "fit" size: a page flush with both edges of the
 * pane looks cut off. */
const FIT_SHARE = 0.94;

/** How long a new PDF may take to draw the pages in view before it takes over anyway. */
const FIRST_PAGES_TIMEOUT_MS = 4000;

/** What the viewer tells whoever owns it. Plain callbacks: this file knows nothing of IPC, projects or
 * tabs (the same split `definition.ts` makes). */
export interface PdfViewerCallbacks {
  /** A double-click on a page: `(page, x, y)` in PDF points from the page's top-left corner, the unit
   * SyncTeX answers in (S3.5). */
  onInverseSearch?: (page: number, x: number, y: number) => void;
  /** A click on a link in the PDF that goes to a web page. The viewer never navigates the window. */
  onExternalLink?: (url: string) => void;
}

/** One pdf.js viewer with its own scroll container: the pane holds one that is showing and, for a
 * moment while a new PDF draws, a second that is hidden. */
class Surface {
  readonly container: HTMLDivElement;
  readonly pdfViewer: PDFViewer;
  private readonly eventBus = new EventBus();
  private readonly linkService: PDFLinkService;
  private document: PDFDocumentProxy | null = null;
  /** The size that fits the pane's width, as a pdf.js scale; what a zoom is a multiple of. */
  private fitScale = 1;
  /** Searches the text of this surface's document; the controller does the finding, the session talks to it. */
  readonly findController: PaneFindController;
  readonly find: FindSession;
  /** Who hears how this surface's search is going. Set while this is the surface on show: a hidden one
   * that is still drawing must not report to the find bar. */
  findStatusListener: ((status: FindStatus) => void) | null = null;

  constructor(host: HTMLElement, callbacks: PdfViewerCallbacks, hidden: boolean) {
    this.container = window.document.createElement('div');
    this.container.className = 'pdf-scroll';
    if (hidden) this.setHidden(true);
    const viewerElement = window.document.createElement('div');
    viewerElement.className = 'pdfViewer';
    this.container.appendChild(viewerElement);
    host.appendChild(this.container);

    this.linkService = new PDFLinkService({ eventBus: this.eventBus });
    this.findController = new PaneFindController(
      { eventBus: this.eventBus, linkService: this.linkService },
      (match) => this.revealMatch(match),
    );
    this.find = new FindSession(this.eventBus, (status) => this.findStatusListener?.(status));
    this.pdfViewer = new PDFViewer({
      container: this.container,
      viewer: viewerElement,
      eventBus: this.eventBus,
      linkService: this.linkService,
      findController: this.findController,
      // Links drawn and clickable, forms not: a LaTeX document has links and no forms.
      annotationMode: AnnotationMode.ENABLE,
      // The pane has its own Ctrl+wheel zoom (`PdfPane.svelte`).
      supportsPinchToZoom: false,
      removePageBorders: true,
    });
    this.linkService.setViewer(this.pdfViewer);

    // Capture phase, so this sees the click before pdf.js's own link handling and before the browser
    // acts on the address. A link in a PDF is its author's text, not ours: a web address goes to the
    // person's browser, a link inside the PDF is left to pdf.js, and nothing is ever allowed to
    // navigate this window (`geometry.linkAction`).
    this.container.addEventListener(
      'click',
      (event) => {
        const anchor = event.target instanceof Element ? event.target.closest('a') : null;
        if (!anchor) return;
        const action = linkAction(anchor.getAttribute('href') ?? '');
        if (action.kind === 'internal') return;
        event.preventDefault();
        event.stopPropagation();
        if (action.kind === 'browser') callbacks.onExternalLink?.(action.url);
      },
      true,
    );

    this.container.addEventListener('dblclick', (event) => {
      const pageElement = event.target instanceof Element ? event.target.closest<HTMLElement>('.page') : null;
      const pageNumber = Number(pageElement?.dataset.pageNumber);
      if (!pageElement || !Number.isInteger(pageNumber)) return;
      const pageView = this.pdfViewer.getPageView(pageNumber - 1);
      if (!pageView) return;
      const box = pageElement.getBoundingClientRect();
      const { x, y } = pointsFromClick(
        event,
        { left: box.left, top: box.top, borderLeft: pageElement.clientLeft, borderTop: pageElement.clientTop },
        pageView.viewport.scale,
      );
      callbacks.onInverseSearch?.(pageNumber, x, y);
    });
  }

  /** Scroll so a search match can be seen, with the line before it above it (see `scrollToReveal`). */
  private revealMatch(match: HTMLElement): void {
    const box = match.getBoundingClientRect();
    const pane = this.container.getBoundingClientRect();
    const target = scrollToReveal(
      { left: box.left, top: box.top, width: box.width, height: box.height },
      { left: pane.left, top: pane.top, width: this.container.clientWidth, height: this.container.clientHeight },
      { left: this.container.scrollLeft, top: this.container.scrollTop },
    );
    // At once, not smoothly: a match can be a hundred pages away, and a smooth scroll across them would
    // draw every page on the way.
    if (target) this.container.scrollTo({ ...target, behavior: 'instant' });
  }

  /** Hidden but still laid out, so pdf.js measures and draws it as it will look once it is shown. */
  setHidden(hidden: boolean): void {
    this.container.style.visibility = hidden ? 'hidden' : '';
    this.container.style.pointerEvents = hidden ? 'none' : '';
  }

  /** Show `document` and resolve when pdf.js has laid out the pages (`pagesinit`). */
  open(document: PDFDocumentProxy): Promise<void> {
    this.document = document;
    const initialised = new Promise<void>((resolve) => this.eventBus.on('pagesinit', () => resolve(), { once: true }));
    this.pdfViewer.setDocument(document);
    this.linkService.setDocument(document, null);
    return initialised;
  }

  /** The first page, `width` pixels wide, as a PNG. */
  async firstPagePng(width: number): Promise<Blob | null> {
    const page = await this.document?.getPage(1);
    if (!page) return null;
    const viewport = page.getViewport({ scale: width / page.getViewport({ scale: 1 }).width });
    const canvas = window.document.createElement('canvas');
    canvas.width = width;
    // Of a poster a kilometre high, the top.
    canvas.height = Math.min(Math.round(viewport.height), width * 2);
    await page.render({ canvas, viewport }).promise;
    return new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
  }

  /** Draw the pages again at the same size. */
  redraw(): void {
    if (this.document) this.pdfViewer.refresh();
  }

  /** The size that fits the pane's width ([`FIT_SHARE`] of it), times `zoom`. */
  applyZoom(zoom: number): void {
    // pdf.js works out what fits the width; the fit size is then a little smaller than that.
    this.pdfViewer.currentScaleValue = 'page-width';
    this.fitScale = this.pdfViewer.currentScale * FIT_SHARE;
    this.pdfViewer.currentScale = this.fitScale * zoom;
  }

  /** The pane was resized: a document at the fit size is fitted again; a zoomed one stays as it is. */
  refit(zoom: number): void {
    if (zoom === 1 && this.document) this.applyZoom(1);
  }

  get pageCount(): number {
    return this.pdfViewer.pagesCount;
  }

  /** Resolves once every page that is in view has been drawn, or after `timeoutMs`: a document that
   * is slow to draw must not keep the old one on screen for ever. */
  async inViewPagesDrawn(timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (this.inViewPages().every((pageView) => pageView.renderingState === RenderingStates.FINISHED)) return;
      await new Promise((resolve) => setTimeout(resolve, 30));
    }
  }

  private inViewPages() {
    const top = this.container.scrollTop;
    const bottom = top + this.container.clientHeight;
    const inView = [];
    for (let index = 0; index < this.pdfViewer.pagesCount; index++) {
      const pageView = this.pdfViewer.getPageView(index);
      if (!pageView) continue;
      const pageTop = pageView.div.offsetTop;
      const pageBottom = pageTop + pageView.div.offsetHeight;
      if (pageBottom >= top && pageTop <= bottom) inView.push(pageView);
      else if (pageTop > bottom) break;
    }
    return inView;
  }

  /** Throw the viewer away: its pages, its document, its element. */
  async destroy(): Promise<void> {
    this.pdfViewer.setDocument(null as unknown as PDFDocumentProxy);
    this.container.remove();
    const document = this.document;
    this.document = null;
    await document?.destroy();
  }
}

export class PdfViewer {
  private active: Surface | null = null;
  private renderToken = 0;
  /** What is on show, as `load` was given it. */
  private shown: string | null = null;
  private zoom = 1;
  /** What the reader is searching for, kept here so a new PDF can be searched for the same thing. */
  private findRequest: FindRequest | null = null;
  private findStatusHandler: ((status: FindStatus) => void) | null = null;
  private readonly resizeObserver: ResizeObserver;

  constructor(
    private readonly host: HTMLElement,
    private readonly callbacks: PdfViewerCallbacks = {},
  ) {
    this.resizeObserver = new ResizeObserver(() => this.active?.refit(this.zoom));
    this.resizeObserver.observe(host);
  }

  /**
   * Call `handler` with `(page, x, y)` in PDF points whenever the reader double-clicks a page —
   * the inverse-search gesture (S3.5). Double-click, not single-click: a single click inside the
   * text layer is how pdf.js's own selection already works, and stealing it would break "select text in
   * the PDF".
   */
  onInverseSearch(handler: (page: number, x: number, y: number) => void): void {
    this.callbacks.onInverseSearch = handler;
  }

  get pageCount(): number {
    return this.active?.pageCount ?? 0;
  }

  /** Call `handler` whenever what the find bar should say about the search changes. */
  onFindStatus(handler: (status: FindStatus) => void): void {
    this.findStatusHandler = handler;
  }

  /** Search the PDF for `query`: every match highlighted, the first one shown. Called as the reader types. */
  find(query: string, options: FindOptions): void {
    this.findRequest = query === '' ? null : { query, options };
    // The reader asked: show them, even if a rebuild had been holding the view still.
    this.active?.findController.holdStill(false);
    this.active?.find.search(query, options);
    if (query === '') this.findStatusHandler?.({ kind: 'idle' });
  }

  findNext(): void {
    this.active?.findController.holdStill(false);
    this.active?.find.next();
  }

  findPrevious(): void {
    this.active?.findController.holdStill(false);
    this.active?.find.previous();
  }

  /** End the search: highlights off, and a new PDF is no longer searched. */
  closeFind(): void {
    this.findRequest = null;
    this.active?.find.close();
    this.findStatusHandler?.({ kind: 'idle' });
  }

  /** Load and show a PDF from a URL the webview may fetch. Safe to call repeatedly: a newer call
   * makes an older one that is still drawing give up. */
  async load(url: string): Promise<void> {
    const token = ++this.renderToken;

    // Fetch the bytes ourselves rather than letting pdf.js stream them: one request, no
    // range-request negotiation with the asset protocol, and an ordinary error if it fails.
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Could not load PDF (${response.status})`);
    const data = await response.arrayBuffer();
    if (token !== this.renderToken) return;

    const document = await pdfjs.getDocument({ data }).promise;
    if (token !== this.renderToken) {
      await document.destroy();
      return;
    }

    const previous = this.active;
    const next = new Surface(this.host, this.callbacks, previous !== null);
    try {
      await next.open(document);
      next.applyZoom(this.zoom);
      // Where the reader was: the same scroll position in the new document (the pages are laid out the
      // same unless the document changed shape, and then it is close).
      if (previous) {
        next.container.scrollTop = previous.container.scrollTop;
        next.container.scrollLeft = previous.container.scrollLeft;
      }
      await next.inViewPagesDrawn(FIRST_PAGES_TIMEOUT_MS);
    } catch (error) {
      await next.destroy();
      throw error;
    }
    if (token !== this.renderToken) {
      await next.destroy();
      return;
    }

    next.setHidden(false);
    this.active = next;
    this.shown = url;
    if (previous) previous.findStatusListener = null;
    next.findStatusListener = (status) => this.findStatusHandler?.(status);
    // The reader's search carries over to the new PDF. The view is held still: they are reading wherever
    // they are, and a rebuild must not take them to the first match.
    if (this.findRequest) {
      next.findController.holdStill(true);
      next.find.search(this.findRequest.query, this.findRequest.options);
    }
    await previous?.destroy();
  }

  /**
   * Scroll to and briefly highlight a spot from a SyncTeX forward search (S3.4): `page` is 1-based,
   * `x`/`y` are PDF points from the page's top-left corner. The highlight is a short-lived absolutely
   * positioned `<div>` in the page's box rather than anything drawn into the page: pages are redrawn
   * whenever they scroll out of and back into view, so anything baked into pixels would vanish.
   */
  scrollToPosition(page: number, x: number, y: number): void {
    const surface = this.active;
    const pageView = surface?.pdfViewer.getPageView(page - 1);
    if (!surface || !pageView) return;
    const cssPerPoint = pageView.viewport.scale;
    const pageElement = pageView.div;
    const top = pageElement.offsetTop + pageElement.clientTop + y * cssPerPoint;

    surface.container.scrollTo({
      top: Math.max(0, top - surface.container.clientHeight / 3),
      left: 0,
      behavior: 'smooth',
    });

    const mark = window.document.createElement('div');
    mark.className = 'synctex-highlight';
    mark.style.left = `${x * cssPerPoint}px`;
    mark.style.top = `${y * cssPerPoint}px`;
    pageElement.appendChild(mark);
    // A CSS transition, not a `setTimeout` removal: if the page is redrawn meanwhile its box is
    // emptied, and nothing needs to race that.
    requestAnimationFrame(() => mark.classList.add('fade'));
    mark.addEventListener('transitionend', () => mark.remove());
  }

  /** Zoom as a multiple of the size that fits the pane's width; applied at once. */
  setZoom(zoom: number): void {
    this.zoom = clampZoom(zoom);
    this.active?.applyZoom(this.zoom);
  }

  /** The first page of the PDF at `url`, `width` pixels wide; `null` when another one is on show. */
  async firstPagePng(url: string, width: number): Promise<Blob | null> {
    return this.shown === url ? (this.active?.firstPagePng(width) ?? null) : null;
  }

  /** Draw the pages again: the pixel ratio has changed (see `drawAtPixelRatio`). */
  redraw(): void {
    this.active?.redraw();
  }

  getZoom(): number {
    return this.zoom;
  }

  /** Show no pages, and stop any load still in flight from showing some later. Used when there is
   * no PDF to show — a different project just opened — so the previous one's pages do not sit under
   * the "Compiling…" note. */
  async clear(): Promise<void> {
    this.renderToken++;
    this.shown = null;
    const previous = this.active;
    this.active = null;
    // The search stays asked for, and runs again when a PDF arrives; until then there is nothing to count.
    if (this.findRequest) this.findStatusHandler?.({ kind: 'idle' });
    await previous?.destroy();
  }

  async destroy(): Promise<void> {
    this.renderToken++;
    this.resizeObserver.disconnect();
    const previous = this.active;
    this.active = null;
    await previous?.destroy();
  }
}
