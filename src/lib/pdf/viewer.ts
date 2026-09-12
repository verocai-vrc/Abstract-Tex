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

      const context = canvas.getContext('2d');
      if (!context) return null;
      await page.render({ canvasContext: context, viewport, canvas }).promise;
      fragment.appendChild(canvas);
    }
    return fragment;
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
