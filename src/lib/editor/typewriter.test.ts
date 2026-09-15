import { describe, expect, it } from 'vitest';
import { centeredScrollTarget, type ScrollMetrics } from './typewriter';

function metrics(overrides: Partial<ScrollMetrics>): ScrollMetrics {
  return {
    viewportHeight: 600,
    cursorTop: 0,
    cursorHeight: 20,
    maxScrollTop: 10000,
    ...overrides,
  };
}

describe('centeredScrollTarget', () => {
  it('centres a cursor line partway down a tall document', () => {
    // Cursor line's top is at 1000px into the content; a 600px viewport centred on it wants its
    // midpoint (1010) at the viewport's midpoint (300), i.e. scrollTop = 1010 - 300 = 710.
    const target = centeredScrollTarget(metrics({ cursorTop: 1000, cursorHeight: 20, viewportHeight: 600 }));
    expect(target).toBe(710);
  });

  it('clamps to 0 when centring would scroll past the top of the document', () => {
    // A cursor near the very start cannot be centred without a negative scrollTop.
    const target = centeredScrollTarget(metrics({ cursorTop: 10, cursorHeight: 20, viewportHeight: 600 }));
    expect(target).toBe(0);
  });

  it('clamps to maxScrollTop when centring would scroll past the end of the document', () => {
    // A cursor near the very end cannot be centred without scrolling further than the content
    // allows; it should stop at the last legal scrollTop instead of overshooting.
    const target = centeredScrollTarget(
      metrics({ cursorTop: 9990, cursorHeight: 20, viewportHeight: 600, maxScrollTop: 9400 }),
    );
    expect(target).toBe(9400);
  });

  it('produces scrollTop 0 for a document that fits entirely on screen', () => {
    const target = centeredScrollTarget(
      metrics({ cursorTop: 100, cursorHeight: 20, viewportHeight: 600, maxScrollTop: 0 }),
    );
    expect(target).toBe(0);
  });

  it('scales with a taller cursor line (e.g. a wrapped long line)', () => {
    const shortLine = centeredScrollTarget(metrics({ cursorTop: 1000, cursorHeight: 20, viewportHeight: 600 }));
    const tallLine = centeredScrollTarget(metrics({ cursorTop: 1000, cursorHeight: 60, viewportHeight: 600 }));
    expect(tallLine).toBeGreaterThan(shortLine);
  });

  it('scales with viewport height, centring lower content in a taller viewport at the same scrollTop', () => {
    const shortViewport = centeredScrollTarget(metrics({ cursorTop: 1000, cursorHeight: 20, viewportHeight: 400 }));
    const tallViewport = centeredScrollTarget(metrics({ cursorTop: 1000, cursorHeight: 20, viewportHeight: 800 }));
    expect(tallViewport).toBeLessThan(shortViewport);
  });
});
