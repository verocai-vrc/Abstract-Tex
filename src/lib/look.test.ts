// S17.11: how the window looks, with a fake Rust that records every call.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Preferences } from './ipc';

const ipcCalls: unknown[][] = [];
let answers: Record<string, (...args: any[]) => unknown> = {};

vi.mock('./ipc', () => ({
  ipc: new Proxy({}, {
    get: (_target, name: string) => (...args: unknown[]) => {
      ipcCalls.push([name, ...args]);
      if (name.startsWith('on')) return Promise.resolve(() => {});
      return new Promise((resolve) => resolve(answers[name]?.(...args) ?? null));
    },
  }),
}));

const { app } = await import('./state.svelte');
const { startScreen } = await import('./start.svelte');
const { applyRememberedLook, look, setTextSize, zoomMissingFromPixelRatio } = await import('./look.svelte');
const { allCommands } = await import('./commands');
await import('./start-controller.svelte');

const kept: Preferences = { place: { path: '/d', parts: ['d'] }, author: '', textSize: 100, colours: 'match' };

beforeEach(() => {
  vi.useFakeTimers();
  ipcCalls.length = 0;
  answers = {
    preferences: () => kept,
    setPreferences: (change: Partial<Preferences>) => ({ ...kept, ...change }),
  };
  look.textSize = 100;
  startScreen.preferences = null;
  startScreen.message = null;
  startScreen.visible = true;
  app.launching = false;
  app.opening = null;
});
afterEach(() => vi.useRealTimers());

describe('text size (S17.11)', () => {
  it('counts the zoom only where the pixel ratio has not', () => {
    // Measured under WebKitGTK: 1 at 100%, 1.5 at 150%, 1.149999976158142 at 115%.
    expect(zoomMissingFromPixelRatio(1, 1.5, 1.5)).toBe(1);
    expect(zoomMissingFromPixelRatio(1, 1.149999976158142, 1.15)).toBe(1);
    expect(zoomMissingFromPixelRatio(2, 3, 1.5)).toBe(1);
    // One that keeps the screen's own ratio whatever the zoom.
    expect(zoomMissingFromPixelRatio(1, 1, 1.5)).toBe(1.5);
    expect(zoomMissingFromPixelRatio(2, 2, 1.15)).toBe(1.15);
    // At 100% there is nothing to miss, on whatever screen the window has been moved to.
    expect(zoomMissingFromPixelRatio(2, 1, 1)).toBe(1);
  });

  it('remembers the size and applies it', async () => {
    await setTextSize(150);
    expect(ipcCalls).toEqual([['setPreferences', { textSize: 150 }], ['applyTextSize', 150]]);
    expect([look.textSize, startScreen.preferences?.textSize]).toEqual([150, 150]);
  });

  it('is applied even when it cannot be remembered, and that is said', async () => {
    answers.setPreferences = () => {
      throw 'the disk is full';
    };
    await setTextSize(150);
    expect([look.textSize, startScreen.message?.text]).toEqual([150, 'the disk is full']);
  });

  it('a size that cannot be applied is said, and is not the one in use', async () => {
    answers.applyTextSize = () => {
      throw 'no';
    };
    await setTextSize(150);
    expect([look.textSize, startScreen.message?.text]).toEqual([100, 'no']);
  });

  it('of two sizes asked for quickly the last one wins, however slow the first', async () => {
    const first = new Promise((resolve) => setTimeout(() => resolve({ ...kept, textSize: 150 }), 500));
    answers.setPreferences = (change: Partial<Preferences>) => (change.textSize === 150 ? first : { ...kept, ...change });
    const both = [setTextSize(150), setTextSize(200)];
    await vi.advanceTimersByTimeAsync(500);
    await Promise.all(both);
    expect(ipcCalls.filter(([name]) => name === 'applyTextSize')).toEqual([['applyTextSize', 150], ['applyTextSize', 200]]);
    expect(look.textSize).toBe(200);
  });

  it('what was remembered is applied before the window is drawn', async () => {
    answers.preferences = () => ({ ...kept, textSize: 130 });
    await applyRememberedLook();
    expect(ipcCalls).toEqual([['preferences'], ['applyTextSize', 130]]);
    expect([look.textSize, startScreen.preferences?.textSize]).toEqual([130, 130]);
  });

  it('waits a second at most for preferences that never answer', async () => {
    answers.preferences = () => new Promise(() => {});
    let ready = false;
    void applyRememberedLook().then(() => (ready = true));
    await vi.advanceTimersByTimeAsync(999);
    expect(ready).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(ready).toBe(true);
  });

  it('preferences that cannot be read keep nobody out', async () => {
    answers.preferences = () => {
      throw 'no';
    };
    await applyRememberedLook();
    expect([look.textSize, startScreen.message]).toEqual([100, null]);
  });

  it('the page is in the palette', () => {
    const command = allCommands().find((c) => c.id === 'look');
    expect([command?.title, command?.withoutDocument]).toEqual(['Text and colours…', true]);
    command?.run();
    expect(startScreen.page).toBe('look');
  });
});
