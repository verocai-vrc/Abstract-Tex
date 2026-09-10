import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  decideExternalChange,
  OpenDocument,
  type DocumentBackend,
  type ExternalChangeFacts,
} from './document';
import type { TextOp } from './ipc';

/** A backend that records writes and computes a naive whole-text replacement diff. */
function fakeBackend() {
  const writes: Array<{ path: string; contents: string }> = [];
  const saved: string[] = [];
  const backend: DocumentBackend = {
    async writeFile(path, contents) {
      writes.push({ path, contents });
    },
    async diffOps(oldText, newText): Promise<TextOp[]> {
      // The same prefix/suffix trimming the Rust reconciler does, so untouched text is left
      // alone (a wholesale replace would delete the author's own insertions and make the
      // undo test meaningless). The real diff has its own property tests in Rust.
      let prefix = 0;
      while (prefix < oldText.length && prefix < newText.length && oldText[prefix] === newText[prefix]) prefix++;
      let suffix = 0;
      while (
        suffix < oldText.length - prefix &&
        suffix < newText.length - prefix &&
        oldText[oldText.length - 1 - suffix] === newText[newText.length - 1 - suffix]
      )
        suffix++;
      return [{ index: prefix, delete: oldText.length - prefix - suffix, insert: newText.slice(prefix, newText.length - suffix) }];
    },
    afterSave(path) {
      saved.push(path);
    },
  };
  return { backend, writes, saved };
}

describe('OpenDocument', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('starts clean with the loaded text', () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'hello', backend);
    expect(doc.text()).toBe('hello');
    expect(doc.dirty).toBe(false);
    doc.dispose();
  });

  it('a burst of edits produces one write, 700 ms after the last keystroke', async () => {
    const { backend, writes, saved } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'abc', backend);
    const dirtyStates: boolean[] = [];
    doc.onDirtyChange = (d) => dirtyStates.push(d);

    doc.ytext.insert(3, 'd');
    await vi.advanceTimersByTimeAsync(300);
    doc.ytext.insert(4, 'e');
    await vi.advanceTimersByTimeAsync(300);
    doc.ytext.insert(5, 'f');
    expect(writes).toHaveLength(0);
    expect(doc.dirty).toBe(true);

    await vi.advanceTimersByTimeAsync(700);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abcdef' }]);
    expect(doc.dirty).toBe(false);
    expect(saved).toEqual(['main.tex']);
    expect(dirtyStates).toEqual([true, false]);
    doc.dispose();
  });

  it('save() writes immediately and cancels the pending timer', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', '', backend);
    doc.ytext.insert(0, 'x');
    expect(await doc.save()).toBe(true);
    expect(writes).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(writes).toHaveLength(1);
    expect(await doc.save()).toBe(false);
    doc.dispose();
  });

  it('applying an external change does not mark the document dirty or trigger a write', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'old text', backend);
    await doc.applyExternal('new text');
    expect(doc.text()).toBe('new text');
    expect(doc.dirty).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(writes).toHaveLength(0);
    doc.dispose();
  });

  it('external changes are not undoable, but the author\'s own edits still are', async () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'one', backend);
    doc.ytext.insert(3, ' two');
    await vi.advanceTimersByTimeAsync(600); // past the undo capture timeout
    await doc.applyExternal('one two three');
    doc.undo.undo();
    // Undo removes the author's " two"; the external " three" stays.
    expect(doc.text()).toBe('one three');
    doc.dispose();
  });

  it('holding saves keeps the debounce from answering a conflict for the author', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'abc', backend);

    doc.ytext.insert(3, 'd'); // dirty, save armed for 700 ms from now
    doc.holdSaves();
    await vi.advanceTimersByTimeAsync(5000);
    expect(writes).toHaveLength(0);
    expect(doc.dirty).toBe(true);
    expect(await doc.save()).toBe(false); // even an explicit save is refused while held

    doc.releaseSaves();
    await vi.advanceTimersByTimeAsync(700);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abcd' }]);
    doc.dispose();
  });

  it('releasing a hold on a clean buffer does not write anything', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.holdSaves();
    doc.releaseSaves();
    await vi.advanceTimersByTimeAsync(5000);
    expect(writes).toHaveLength(0);
    doc.dispose();
  });

  it('after the file vanishes, an unchanged buffer still saves itself back', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'abc', backend);
    // Nothing about the buffer changed, so without forgetDiskState() save() would short-circuit
    // on `snapshot === lastSavedText` and Ctrl S would silently do nothing.
    expect(await doc.save()).toBe(false);
    doc.forgetDiskState();
    expect(doc.dirty).toBe(true);
    expect(await doc.save()).toBe(true);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abc' }]);
    doc.dispose();
  });
});

describe('decideExternalChange', () => {
  const facts = (over: Partial<ExternalChangeFacts> = {}): ExternalChangeFacts => ({
    isOpenFile: true,
    existsOnDisk: true,
    sameAsBuffer: false,
    bufferDirty: false,
    ...over,
  });

  it('ignores files that are not the one in the editor', () => {
    expect(decideExternalChange(facts({ isOpenFile: false }))).toBe('ignore');
    // Even when every other fact would otherwise be interesting.
    expect(decideExternalChange(facts({ isOpenFile: false, existsOnDisk: false, bufferDirty: true }))).toBe('ignore');
  });

  it('reports a deleted file rather than treating it as an empty one', () => {
    expect(decideExternalChange(facts({ existsOnDisk: false }))).toBe('vanished');
    expect(decideExternalChange(facts({ existsOnDisk: false, bufferDirty: true }))).toBe('vanished');
  });

  it('ignores the echo of our own write, dirty or not', () => {
    expect(decideExternalChange(facts({ sameAsBuffer: true }))).toBe('ignore');
    expect(decideExternalChange(facts({ sameAsBuffer: true, bufferDirty: true }))).toBe('ignore');
  });

  it('applies a real change silently when the buffer has nothing to lose', () => {
    expect(decideExternalChange(facts())).toBe('apply');
  });

  it('asks, and never merges, when the buffer is dirty and the disk moved on', () => {
    expect(decideExternalChange(facts({ bufferDirty: true }))).toBe('conflict');
  });
});
