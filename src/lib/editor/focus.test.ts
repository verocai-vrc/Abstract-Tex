import { describe, expect, it } from 'vitest';
import { currentParagraphRange } from './focus';

describe('currentParagraphRange', () => {
  it('spans a single-line paragraph surrounded by blank lines', () => {
    const doc = 'Intro line.\n\nOne paragraph.\n\nOutro line.';
    const cursor = doc.indexOf('One paragraph.') + 3;
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(doc.slice(from, to)).toBe('One paragraph.');
  });

  it('spans every contiguous non-blank line around the cursor', () => {
    const doc = ['First para line one.', 'First para line two.', '', 'Second para.'].join('\n');
    const cursor = doc.indexOf('line two') + 4;
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(doc.slice(from, to)).toBe('First para line one.\nFirst para line two.');
  });

  it('treats a blank cursor line as its own paragraph', () => {
    const doc = 'Para one.\n\nPara two.';
    const cursor = doc.indexOf('\n\n') + 1; // the blank line itself
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(doc.slice(from, to)).toBe('');
  });

  it('stops at the start of the document', () => {
    const doc = 'Line one.\nLine two.\n\nLine four.';
    const cursor = 2; // inside "Line one."
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(from).toBe(0);
    expect(doc.slice(from, to)).toBe('Line one.\nLine two.');
  });

  it('stops at the end of the document with no trailing blank line', () => {
    const doc = 'Para one.\n\nLast line one.\nLast line two.';
    const cursor = doc.length - 2;
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(to).toBe(doc.length);
    expect(doc.slice(from, to)).toBe('Last line one.\nLast line two.');
  });

  it('handles a cursor at offset 0 in a single-line document', () => {
    const doc = 'Just one line.';
    const { from, to } = currentParagraphRange(doc, 0);
    expect(doc.slice(from, to)).toBe('Just one line.');
  });

  it('treats a whitespace-only line as blank', () => {
    const doc = 'Para one.\n   \nPara two.';
    const cursor = doc.indexOf('Para two.') + 2;
    const { from, to } = currentParagraphRange(doc, cursor);
    expect(doc.slice(from, to)).toBe('Para two.');
  });
});
