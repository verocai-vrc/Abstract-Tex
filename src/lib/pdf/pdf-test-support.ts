// Test-only helpers for the PDF pane's tests: a real PDF to search, and a way to import pdf.js's web
// viewer without a browser. Nothing in the app imports this file.

import { vi } from 'vitest';

/** pdf.js's two halves, as a test needs them: the parser (its Node-friendly build) and the web viewer. */
export interface PdfJs {
  getDocument: typeof import('pdfjs-dist/legacy/build/pdf.mjs').getDocument;
  web: typeof import('pdfjs-dist/web/pdf_viewer.mjs');
}

/**
 * Import pdf.js. The web viewer is written for a browser: merely importing it reads `window` and
 * `document` (an animation-frame promise, the root element's style, and one throwaway element it makes to
 * ask the browser how it rounds). Its find controller touches neither, so the import is given just those
 * three things, and the controller is then driven exactly as the pane drives it.
 */
export async function loadPdfJs(): Promise<PdfJs> {
  vi.stubGlobal('window', { requestAnimationFrame: (callback: () => void) => setTimeout(callback, 0) });
  vi.stubGlobal('document', { documentElement: { style: {} }, createElement: () => ({ style: {} }) });
  // pdf.js first: the web viewer finds it on `globalThis.pdfjsLib`, as it does in a browser.
  const { getDocument } = await import('pdfjs-dist/legacy/build/pdf.mjs');
  const web = await import('pdfjs-dist/web/pdf_viewer.mjs');
  return { getDocument, web };
}

/** A PDF with one page per entry of `pages`, each a list of text lines. Plain ASCII, one built-in font. */
export function makePdf(pages: string[][]): Uint8Array {
  const objects: string[] = [];
  const pageObjectNumbers = pages.map((_, index) => 4 + index * 2);
  objects.push('<< /Type /Catalog /Pages 2 0 R >>');
  objects.push(`<< /Type /Pages /Kids [${pageObjectNumbers.map((n) => `${n} 0 R`).join(' ')}] /Count ${pages.length} >>`);
  objects.push('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  for (const lines of pages) {
    const body = lines.map((line, index) => `${index === 0 ? '' : '0 -16 Td '}(${line}) Tj`).join(' ');
    const stream = `BT /F1 12 Tf 72 720 Td ${body} ET`;
    const contentNumber = objects.length + 2;
    objects.push(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents ${contentNumber} 0 R >>`,
    );
    objects.push(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`);
  }
  let text = '%PDF-1.4\n';
  const offsets: number[] = [];
  objects.forEach((object, index) => {
    offsets.push(text.length);
    text += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xrefAt = text.length;
  text += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets) text += `${String(offset).padStart(10, '0')} 00000 n \n`;
  text += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefAt}\n%%EOF\n`;
  return new TextEncoder().encode(text);
}

