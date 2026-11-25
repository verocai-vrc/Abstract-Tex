import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PagePictures } from './page-picture';

const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('the picture of page one (S17.15)', () => {
  let drawn = 0;
  let canDraw = true;
  let pictures: PagePictures;
  beforeEach(() => {
    vi.useFakeTimers();
    drawn = 0;
    canDraw = true;
    pictures = new PagePictures(
      async () => {
        drawn += 1;
      },
      () => canDraw,
    );
  });
  afterEach(() => vi.useRealTimers());

  it('is drawn five seconds after a good build, and the wait starts again at every key', async () => {
    pictures.buildFinished();
    await advance(3000);
    pictures.typed();
    await advance(4999);
    expect(drawn).toBe(0);
    await advance(1);
    expect(drawn).toBe(1);
    await advance(60_000);
    expect(drawn).toBe(1); // once for a build
  });

  it('is not drawn for a key alone', async () => {
    pictures.typed();
    await advance(10_000);
    expect(drawn).toBe(0);
  });

  it('is drawn once for two builds in a row', async () => {
    pictures.buildFinished();
    await advance(4000);
    pictures.buildFinished();
    await advance(4999);
    expect(drawn).toBe(0);
    await advance(10_000);
    expect(drawn).toBe(1);
  });

  it('is not drawn once another document is open', async () => {
    pictures.buildFinished();
    pictures.cancel();
    await advance(10_000);
    expect(drawn).toBe(0);
  });

  it('is not drawn while a draft or a comparison is what is shown', async () => {
    canDraw = false;
    pictures.buildFinished();
    await advance(10_000);
    expect(drawn).toBe(0);
  });

  it('is a picture less, and nothing else, when it cannot be drawn', async () => {
    const failing = new PagePictures(
      () => Promise.reject(new Error('no canvas')),
      () => true,
    );
    failing.buildFinished();
    await advance(5000); // an unhandled rejection would fail the run
  });
});
