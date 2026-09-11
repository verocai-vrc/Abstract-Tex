import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DocumentManager } from './documents';
import type { DocumentBackend } from './document';
import type { TextOp } from './ipc';

/** A backend that records writes. `diffOps` is never exercised by this module directly. */
function fakeBackend() {
  const writes: Array<{ path: string; contents: string }> = [];
  const backend: DocumentBackend = {
    async writeFile(path, contents) {
      writes.push({ path, contents });
    },
    async diffOps(): Promise<TextOp[]> {
      return [];
    },
  };
  return { backend, writes };
}

describe('DocumentManager', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('opening two files keeps two independent documents, in the order they were opened', () => {
    const { backend } = fakeBackend();
    const manager = new DocumentManager(backend);

    const intro = manager.open('sections/intro.tex', 'Intro.');
    const results = manager.open('sections/results.tex', 'Results.');

    expect(manager.isOpen('sections/intro.tex')).toBe(true);
    expect(manager.get('sections/intro.tex')).toBe(intro);
    expect(manager.get('sections/results.tex')).toBe(results);
    expect(manager.tabs()).toEqual(['sections/intro.tex', 'sections/results.tex']);

    // Each has its own Y.Doc: editing one leaves the other's text untouched.
    intro.ytext.insert(intro.ytext.length, '..');
    expect(intro.text()).toBe('Intro...');
    expect(results.text()).toBe('Results.');
  });

  it('closing a tab disposes its document and hands back a neighbour to activate', () => {
    const { backend } = fakeBackend();
    const manager = new DocumentManager(backend);
    manager.open('a.tex', 'a');
    const b = manager.open('b.tex', 'b');
    manager.open('c.tex', 'c');

    // Closing the middle tab leaves the one that slides into its place.
    expect(manager.close('b.tex')).toBe('c.tex');
    expect(manager.isOpen('b.tex')).toBe(false);
    expect(manager.tabs()).toEqual(['a.tex', 'c.tex']);
    expect(b.dirty).toBe(false); // disposal did not throw; nothing left listening either

    // Closing the last tab has no neighbour to offer.
    manager.close('c.tex');
    expect(manager.close('a.tex')).toBeNull();
    expect(manager.tabs()).toEqual([]);
  });

  it('closing an unopened path is a no-op, not an error', () => {
    const { backend } = fakeBackend();
    const manager = new DocumentManager(backend);
    manager.open('a.tex', 'a');
    expect(manager.close('nowhere.tex')).toBeNull();
    expect(manager.tabs()).toEqual(['a.tex']);
  });

  it('closeAll disposes every open document and clears the tab list', () => {
    const { backend } = fakeBackend();
    const manager = new DocumentManager(backend);
    manager.open('a.tex', 'a');
    manager.open('b.tex', 'b');
    manager.closeAll();
    expect(manager.tabs()).toEqual([]);
    expect(manager.isOpen('a.tex')).toBe(false);
  });

  it('dirtyPaths lists only the tabs with unsaved edits, in tab order', () => {
    const { backend } = fakeBackend();
    const manager = new DocumentManager(backend);
    const a = manager.open('a.tex', 'a');
    manager.open('b.tex', 'b');
    a.ytext.insert(1, '!');
    expect(manager.dirtyPaths()).toEqual(['a.tex']);
  });

  it('saveAll writes every dirty tab and leaves clean ones untouched', async () => {
    const { backend, writes } = fakeBackend();
    const manager = new DocumentManager(backend);
    const a = manager.open('a.tex', 'a');
    const b = manager.open('b.tex', 'b');
    a.ytext.insert(1, '!');
    // Neither tab's own 700 ms debounce has fired; saveAll must not wait for it.
    await manager.saveAll();
    expect(writes).toEqual([{ path: 'a.tex', contents: 'a!' }]);
    expect(a.dirty).toBe(false);
    expect(b.dirty).toBe(false);
  });

  it('saveAll surfaces a failure without stopping the other tabs from saving', async () => {
    const writes: string[] = [];
    const backend: DocumentBackend = {
      async writeFile(path, contents) {
        if (path === 'broken.tex') throw new Error('disk full');
        writes.push(path + ':' + contents);
      },
      async diffOps() {
        return [];
      },
    };
    const manager = new DocumentManager(backend);
    const broken = manager.open('broken.tex', 'x');
    const fine = manager.open('fine.tex', 'y');
    broken.ytext.insert(1, '1');
    fine.ytext.insert(1, '2');

    await expect(manager.saveAll()).rejects.toThrow('disk full');
    expect(writes).toEqual(['fine.tex:y2']);
    expect(fine.dirty).toBe(false);
    // The failed tab's edit is still there, still marked dirty, ready to retry.
    expect(broken.dirty).toBe(true);
  });
});
