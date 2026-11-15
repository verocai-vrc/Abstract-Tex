import { describe, expect, it } from 'vitest';
import { clampZoom, linkAction, pointsFromClick, scrollToReveal, ZOOM_MAX, ZOOM_MIN } from './geometry';

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

describe('scrollToReveal', () => {
  // The pane on screen: 600 wide, 900 tall, its top-left corner at (200, 100); scrolled 1000 down, 0 across.
  const pane = { left: 200, top: 100, width: 600, height: 900 };
  const scrolled = { left: 0, top: 1000 };

  it('leaves the view alone when the match is already well inside it', () => {
    const match = { left: 300, top: 500, width: 80, height: 14 };
    expect(scrollToReveal(match, pane, scrolled)).toBeNull();
  });

  it('brings a match from below the window up to a third of the way down', () => {
    // 1200 px below the pane's top edge: 600 further down than the window shows.
    const match = { left: 300, top: 100 + 1200, width: 80, height: 14 };
    // After scrolling, the match sits at pane top + 300 (a third of 900).
    expect(scrollToReveal(match, pane, scrolled)).toEqual({ left: 0, top: 1000 + 1200 - 300 });
  });

  it('brings a match from above the window down to a third of the way down', () => {
    const match = { left: 300, top: 100 - 400, width: 80, height: 14 };
    expect(scrollToReveal(match, pane, scrolled)).toEqual({ left: 0, top: 1000 - 400 - 300 });
  });

  it('does not leave a match half cut off by the edge', () => {
    // 5 px from the bottom edge: inside the window, but a line that close to the edge is hard to see.
    const match = { left: 300, top: 100 + 900 - 19, width: 80, height: 14 };
    expect(scrollToReveal(match, pane, scrolled)).not.toBeNull();
  });

  it('also centres the match across when a zoomed page has put it out to the side', () => {
    // Off the right edge of the pane by 300 px: a zoomed page is wider than the pane. It is in view
    // up and down, so the page is not scrolled that way.
    const match = { left: 200 + 600 + 300, top: 500, width: 100, height: 14 };
    // Its middle is at 1150 and the pane's middle at 500, so the view moves 650 to the right.
    expect(scrollToReveal(match, pane, scrolled)).toEqual({ left: 650, top: 1000 });
  });
});
