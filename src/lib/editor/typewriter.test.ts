import type { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';
import { centeredScrollTarget, centerOnUpdate, type ScrollMetrics } from './typewriter';

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

/**
 * A stand-in for the one rule of `EditorView` that matters here: layout may be read while the view
 * is idle or measuring, never while it is applying an update (CodeMirror's `readMeasured` throws
 * "Reading the editor layout isn't allowed during an update"). Real CodeMirror needs a DOM, which
 * Vitest in Node does not have; this reproduces the rule and nothing else. R6.21 was a plugin that
 * read layout from its `update`, which this fake refuses the way the real view does.
 */
function fakeView() {
  let phase: 'idle' | 'updating' | 'measuring' = 'idle';
  const requests: { key?: unknown; read: (view: EditorView) => unknown; write?: (value: unknown, view: EditorView) => void }[] = [];
  const scroller = {
    scrollTop: 0,
    clientHeight: 600,
    scrollHeight: 5000,
    getBoundingClientRect: () => ({ top: 100 }),
  };
  const view = {
    scrollDOM: scroller,
    state: { selection: { main: { head: 5 } } },
    coordsAtPos(): { top: number; bottom: number } {
      if (phase === 'updating') throw new Error("Reading the editor layout isn't allowed during an update");
      // The cursor's line sits 1000px into the content once the scroller's own offset is removed.
      return { top: 100 + 1000 - scroller.scrollTop, bottom: 100 + 1020 - scroller.scrollTop };
    },
    requestMeasure(request: (typeof requests)[number]): void {
      requests.push(request);
    },
  };
  return {
    view: view as unknown as EditorView,
    scroller,
    requests,
    /** Run `work` the way `EditorView.update` runs a plugin: with layout reads forbidden. */
    duringUpdate(work: () => void): void {
      phase = 'updating';
      try {
        work();
      } finally {
        phase = 'idle';
      }
    },
    /** Run the queued measure requests the way CodeMirror does: all reads, then all writes. */
    measure(): void {
      phase = 'measuring';
      try {
        const measured = requests.map((request) => request.read(view as unknown as EditorView));
        requests.forEach((request, index) => request.write?.(measured[index], view as unknown as EditorView));
      } finally {
        phase = 'idle';
      }
    },
  };
}

describe('centerOnUpdate (R6.21)', () => {
  it('does not read the layout while the update is running', () => {
    const fake = fakeView();
    expect(() => fake.duringUpdate(() => centerOnUpdate({ docChanged: false, selectionSet: true, view: fake.view }))).not.toThrow();
  });

  it('centres the cursor line once the view measures', () => {
    const fake = fakeView();
    fake.duringUpdate(() => centerOnUpdate({ docChanged: false, selectionSet: true, view: fake.view }));
    fake.measure();
    // The line's top is 1000px into the content, 20px tall, in a 600px scroller: 1010 - 300.
    expect(fake.scroller.scrollTop).toBe(710);
  });

  it('does nothing for an update that changed neither the text nor the selection', () => {
    const fake = fakeView();
    fake.duringUpdate(() => centerOnUpdate({ docChanged: false, selectionSet: false, view: fake.view }));
    fake.measure();
    expect(fake.requests).toHaveLength(0);
    expect(fake.scroller.scrollTop).toBe(0);
  });

  it('asks under one key, so several updates in a frame are measured once', () => {
    const fake = fakeView();
    fake.duringUpdate(() => {
      centerOnUpdate({ docChanged: true, selectionSet: true, view: fake.view });
      centerOnUpdate({ docChanged: true, selectionSet: true, view: fake.view });
    });
    expect(fake.requests).toHaveLength(2);
    expect(fake.requests[0]!.key).toBeDefined();
    expect(fake.requests[0]!.key).toBe(fake.requests[1]!.key);
  });
});
