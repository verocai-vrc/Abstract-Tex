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
const { applyRememberedLook, look, resolveLook, setColours, setTextSize, zoomMissingFromPixelRatio } = await import('./look.svelte');
const { allCommands } = await import('./commands');
await import('./start-controller.svelte');

/** What the system wants, and who listens for it to change. */
const system = { dark: false, moreContrast: false };
let listeners: Array<() => void> = [];
const root = { dataset: {} as Record<string, string | undefined> };
vi.stubGlobal('document', { documentElement: root });
vi.stubGlobal('matchMedia', (query: string) => ({
  get matches() {
    return query.includes('contrast') ? system.moreContrast : system.dark;
  },
  addEventListener: (_kind: string, listener: () => void) => listeners.push(listener),
}));

const kept: Preferences = { place: { path: '/d', parts: ['d'] }, author: '', textSize: 100, colours: 'match' };

beforeEach(() => {
  vi.useFakeTimers();
  ipcCalls.length = 0;
  answers = {
    preferences: () => kept,
    setPreferences: (change: Partial<Preferences>) => ({ ...kept, ...change }),
  };
  look.textSize = 100;
  look.colours = 'match';
  root.dataset = {};
  system.dark = system.moreContrast = false;
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

describe('colours (S17.12)', () => {
  it('are the ones chosen, or the ones the system wants', () => {
    expect(resolveLook('match', false, false)).toBe('light');
    expect(resolveLook('match', true, false)).toBe('dark');
    expect(resolveLook('match', false, true)).toBe('contrast-light');
    expect(resolveLook('match', true, true)).toBe('contrast-dark');
    expect(resolveLook('contrast-light', true, false)).toBe('contrast-light');
    expect(resolveLook('light', true, true)).toBe('light');
  });

  it('are applied at once and remembered', async () => {
    const remembering = setColours('contrast-dark');
    expect([look.colours, root.dataset.look]).toEqual(['contrast-dark', 'contrast-dark']);
    await remembering;
    expect(ipcCalls).toEqual([['setPreferences', { colours: 'contrast-dark' }]]);
    expect(startScreen.preferences?.colours).toBe('contrast-dark');
  });

  it('that cannot be remembered are applied, and that is said', async () => {
    answers.setPreferences = () => {
      throw 'the disk is full';
    };
    await setColours('dark');
    expect([root.dataset.look, startScreen.message?.text]).toEqual(['dark', 'the disk is full']);
  });

  it('that were remembered are applied before the window is drawn', async () => {
    answers.preferences = () => ({ ...kept, colours: 'contrast-light' });
    await applyRememberedLook();
    expect([look.colours, root.dataset.look]).toEqual(['contrast-light', 'contrast-light']);
  });

  it('match the system when nothing answers', async () => {
    system.dark = true;
    answers.preferences = () => new Promise(() => {});
    void applyRememberedLook();
    await vi.advanceTimersByTimeAsync(1000);
    expect(root.dataset.look).toBe('dark');
  });

  it('are not put back by a wait that is over', async () => {
    let answer!: (preferences: Preferences) => void;
    answers.preferences = () => new Promise((resolve) => (answer = resolve));
    void applyRememberedLook();
    await vi.advanceTimersByTimeAsync(999);
    answer({ ...kept, colours: 'dark' });
    await vi.advanceTimersByTimeAsync(1000);
    expect(root.dataset.look).toBe('dark');
  });

  it('follow the system while they match it, and only then', async () => {
    listeners = [];
    await applyRememberedLook();
    expect(root.dataset.look).toBe('light');
    system.dark = true;
    for (const listener of listeners) listener();
    expect(root.dataset.look).toBe('dark');
    system.moreContrast = true;
    for (const listener of listeners) listener();
    expect(root.dataset.look).toBe('contrast-dark');
    expect(listeners.length).toBe(2);
    await setColours('light');
    for (const listener of listeners) listener();
    expect(root.dataset.look).toBe('light');
  });
});
