// pdf.js rendering for the preview pane.
//
// The pane must never flash or lose its place when a new PDF arrives (DESIGN.md §6): we
// render the new document into fresh canvases off-screen, then swap the whole set in and
// restore the scroll position. If a newer PDF arrives mid-render, the older render is dropped.
//
// Sprint-1 simplification: every page is rendered eagerly. Fine for papers; a sixty-page
// thesis will want virtualised rendering (a later loop, noted in SPRINTS.md).

import * as pdfjs from 'pdfjs-dist';
import type { PDFDocumentProxy } from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?worker&url';

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

export class PdfViewer {
  private document: PDFDocumentProxy | null = null;
  private pagesHost: HTMLElement;
  private renderToken = 0;
  private zoom = 1;

  constructor(private readonly container: HTMLElement) {
    this.pagesHost = window.document.createElement('div');
    this.pagesHost.className = 'pages';
    container.appendChild(this.pagesHost);
  }

  /**
   * Call `handler` with `(page, x, y)` in PDF points whenever the reader double-clicks a page —
   * the inverse-search gesture (S3.5). A plain callback rather than an event the caller
   * subscribes to elsewhere, matching how `definition.ts`'s click handler hands CodeMirror a
   * function rather than knowing about tabs itself: this class stays ignorant of IPC, projects,
   * and tab-opening, and only turns a DOM event into the coordinates SyncTeX needs.
   *
   * Double-click, not single-click: a single click inside the text layer is how pdf.js's own
   * selection and search already work, and stealing it would break "select text in the PDF",
   * which the exit demo for v0.2 does not ask this loop to give up.
   */
  onInverseSearch(handler: (page: number, x: number, y: number) => void): void {
    this.pagesHost.addEventListener('dblclick', (event) => {
      const canvas = event.target;
      if (!(canvas instanceof HTMLCanvasElement)) return;
      const page = Number(canvas.dataset.page);
      const pointWidth = Number(canvas.dataset.pointWidth ?? canvas.clientWidth);
      const cssPerPoint = canvas.clientWidth / pointWidth;
      const rect = canvas.getBoundingClientRect();
      const x = (event.clientX - rect.left) / cssPerPoint;
      const y = (event.clientY - rect.top) / cssPerPoint;
      handler(page, x, y);
    });
  }

  get pageCount(): number {
    return this.document?.numPages ?? 0;
  }

  /** Load and render a PDF from a URL the webview may fetch. Safe to call repeatedly. */
  async load(url: string): Promise<void> {
    const token = ++this.renderToken;

    // Fetch the bytes ourselves rather than letting pdf.js stream them: one request, no
    // range-request negotiation with the asset protocol, and an ordinary error if it fails.
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Could not load PDF (${response.status})`);
    const data = await response.arrayBuffer();
    if (token !== this.renderToken) return;

    const next = await pdfjs.getDocument({ data }).promise;
    if (token !== this.renderToken) {
      await next.destroy();
      return;
    }

    const fragment = await this.renderAllPages(next, token);
    if (!fragment || token !== this.renderToken) {
      await next.destroy();
      return;
    }

    const { scrollTop, scrollLeft } = this.container;
    this.pagesHost.replaceChildren(fragment);
    this.container.scrollTop = scrollTop;
    this.container.scrollLeft = scrollLeft;

    const previous = this.document;
    this.document = next;
    await previous?.destroy();
  }

  private async renderAllPages(doc: PDFDocumentProxy, token: number): Promise<DocumentFragment | null> {
    const fragment = window.document.createDocumentFragment();
    const availableWidth = Math.max(200, this.container.clientWidth - 32);
    const dpr = window.devicePixelRatio || 1;

    for (let pageNumber = 1; pageNumber <= doc.numPages; pageNumber++) {
      if (token !== this.renderToken) return null;
      const page = await doc.getPage(pageNumber);
      const natural = page.getViewport({ scale: 1 });
      const cssScale = (availableWidth / natural.width) * this.zoom;
      const viewport = page.getViewport({ scale: cssScale * dpr });

      const canvas = window.document.createElement('canvas');
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      canvas.style.width = `${Math.floor(viewport.width / dpr)}px`;
      canvas.style.height = `${Math.floor(viewport.height / dpr)}px`;
      canvas.dataset.page = String(pageNumber);
      // PDF points (72 dpi), the unit SyncTeX answers in — recorded here rather than recomputed
      // by whoever needs it later, so a click handler or `scrollToPosition` never has to redo
      // this division against a differently-scaled read of the same canvas.
      canvas.dataset.pointWidth = String(natural.width);

      const context = canvas.getContext('2d');
      if (!context) return null;
      await page.render({ canvasContext: context, viewport, canvas }).promise;
      fragment.appendChild(canvas);
    }
    return fragment;
  }

  /**
   * Scroll to and briefly highlight a spot from a SyncTeX forward search (S3.4): `page` is
   * 1-based, `x`/`y` are PDF points from the page's top-left corner — the same convention
   * `renderAllPages`'s own `page.getViewport({ scale: 1 })` uses, which is why the only
   * conversion needed here is CSS-pixels-per-point, read back from `data-point-width`
   * (`renderAllPages` records it at render time so this method never has to re-derive a scale
   * from the canvas's device-pixel-ratio-scaled backing size).
   *
   * A highlight is a short-lived absolutely positioned `<div>` over the target canvas rather
   * than anything drawn into the canvas itself: the canvas is replaced wholesale on every
   * reload (this file's own header comment), so anything baked into pixels would vanish on the
   * next build and there would be nothing to fade back out.
   */
  scrollToPosition(page: number, x: number, y: number): void {
    const canvas = this.pagesHost.querySelector<HTMLCanvasElement>(`canvas[data-page="${page}"]`);
    if (!canvas) return;

    const pointWidth = Number(canvas.dataset.pointWidth ?? canvas.clientWidth);
    const cssPerPoint = canvas.clientWidth / pointWidth;

    const highlightTop = canvas.offsetTop + y * cssPerPoint;
    const highlightLeft = canvas.offsetLeft + x * cssPerPoint;

    this.container.scrollTo({
      top: Math.max(0, highlightTop - this.container.clientHeight / 3),
      left: 0,
      behavior: 'smooth',
    });

    this.flashHighlight(highlightLeft, highlightTop);
  }

  private flashHighlight(left: number, top: number): void {
    const mark = window.document.createElement('div');
    mark.className = 'synctex-highlight';
    mark.style.left = `${left}px`;
    mark.style.top = `${top}px`;
    this.pagesHost.appendChild(mark);
    // A CSS transition, not a `setTimeout` removal: if a new PDF loads mid-flash,
    // `replaceChildren` in `load()` already removes this node, so nothing needs to race it.
    requestAnimationFrame(() => mark.classList.add('fade'));
    mark.addEventListener('transitionend', () => mark.remove());
  }

  setZoom(zoom: number): void {
    this.zoom = Math.min(4, Math.max(0.25, zoom));
  }

  getZoom(): number {
    return this.zoom;
  }

  async destroy(): Promise<void> {
    this.renderToken++;
    await this.document?.destroy();
    this.document = null;
    this.pagesHost.replaceChildren();
  }
}
