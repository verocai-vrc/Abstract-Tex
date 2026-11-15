import { describe, expect, it } from 'vitest';
import { clampZoom, linkAction, pointsFromClick, ZOOM_MAX, ZOOM_MIN } from './geometry';

describe('pointsFromClick', () => {
  it('turns a click on a page into PDF points from the page\'s top-left corner', () => {
    // The page's box starts at (100, 50) on screen; one PDF point is 1.5 CSS pixels at this zoom.
    const point = pointsFromClick({ clientX: 250, clientY: 200 }, { left: 100, top: 50 }, 1.5);
    expect(point).toEqual({ x: 100, y: 100 });
  });

  it('allows for the page box\'s border, which is not part of the page', () => {
    const point = pointsFromClick({ clientX: 110, clientY: 60 }, { left: 100, top: 50, borderLeft: 9, borderTop: 9 }, 1);
    expect(point).toEqual({ x: 1, y: 1 });
  });
});

describe('linkAction', () => {
  it('opens an https or http address in the browser', () => {
    expect(linkAction('https://example.org/a?b=c#d')).toEqual({ kind: 'browser', url: 'https://example.org/a?b=c#d' });
    expect(linkAction('http://example.org/')).toEqual({ kind: 'browser', url: 'http://example.org/' });
  });

  it('leaves a link inside the PDF to the PDF viewer', () => {
    expect(linkAction('#page=3')).toEqual({ kind: 'internal' });
    expect(linkAction('#section.2')).toEqual({ kind: 'internal' });
  });

  it('refuses every other kind of address, so a PDF cannot send the window anywhere', () => {
    for (const href of [
      'javascript:alert(1)',
      'file:///etc/passwd',
      'mailto:someone@example.org',
      'ftp://example.org/x',
      'data:text/html,<script>1</script>',
      'asset://localhost/x',
      '',
      'not a url',
    ]) {
      expect(linkAction(href), href).toEqual({ kind: 'refused' });
    }
  });
});

describe('clampZoom', () => {
  it('keeps the zoom inside the limits', () => {
    expect(clampZoom(0.01)).toBe(ZOOM_MIN);
    expect(clampZoom(99)).toBe(ZOOM_MAX);
    expect(clampZoom(1.25)).toBe(1.25);
  });
});
