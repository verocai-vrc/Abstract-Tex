import { EditorState } from '@codemirror/state';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  decideExternalChange,
  lineEndingOf,
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

  it('save(false) writes but skips afterSave, for a caller about to act on its own', async () => {
    const { backend, writes, saved } = fakeBackend();
    const doc = new OpenDocument('main.tex', '', backend);
    doc.ytext.insert(0, 'x');
    expect(await doc.save(false)).toBe(true);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'x' }]);
    expect(saved).toEqual([]); // the whole point: no afterSave callback this time
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

  // Review finding R4.6. The diff is computed asynchronously; if the author types while it runs the
  // ops no longer fit, so `applyExternal` retries. When every retry is overtaken it used to fall
  // through, record the disk text as saved, and leave the author's typing in the buffer, so the next
  // autosave overwrote the external change without a question. It now says it did not apply.
  it('says so when the change landed, and when the author kept typing through every attempt', async () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'old text', backend);
    expect(await doc.applyExternal('new text')).toBe(true);
    expect(doc.text()).toBe('new text');

    const real = backend.diffOps;
    backend.diffOps = async (oldText, newText) => {
      doc.ytext.insert(doc.ytext.length, '!'); // a keystroke lands while Rust is diffing
      return real(oldText, newText);
    };
    expect(await doc.applyExternal('somebody else wrote this')).toBe(false);
    expect(doc.text()).toBe('new text!!!'); // one keystroke per attempt, nothing of the disk text merged
    doc.dispose();
  });

  // Review finding R4.5. `scheduleSave` ran `void this.save()`: when the write rejected (disk full, a
  // read-only file, the folder gone) the rejection was unhandled, nobody was told, and nothing tried
  // again until the next keystroke, so the author could close the window believing the text was saved.
  it('tells the author when an autosave fails, keeps the text dirty, and tries again until it lands', async () => {
    const { backend, writes } = fakeBackend();
    let failures = 2;
    const working = backend.writeFile;
    backend.writeFile = async (path, contents) => {
      if (failures-- > 0) throw new Error('No space left on device');
      return working(path, contents);
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    const errors: string[] = [];
    let recovered = 0;
    doc.onSaveError = (error) => errors.push(String(error));
    doc.onSaveRecovered = () => recovered++;

    doc.ytext.insert(3, 'd');
    await vi.advanceTimersByTimeAsync(700); // the debounce fires; the write fails
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('No space left on device');
    expect(doc.dirty).toBe(true);

    await vi.advanceTimersByTimeAsync(5000); // first retry fails too; the author is told once, not each time
    expect(errors).toHaveLength(1);
    expect(doc.dirty).toBe(true);

    await vi.advanceTimersByTimeAsync(5000); // the second retry lands
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abcd' }]);
    expect(doc.dirty).toBe(false);
    expect(recovered).toBe(1);
    doc.dispose();
  });

  // R6.23. `save()` cancels the pending timer before it writes, and only `autosave` re-armed one when
  // the write failed. So a save made by anything else (a build's save-all, a tab closing, the
  // New-project button) that failed took the retry with it: the buffer stayed dirty, the notice said
  // "will be saved as soon as it can be", and nothing tried again until the next keystroke. Seen in the
  // app: the folder made writable again, the tab still dirty two minutes later, Ctrl S saved it at once.
  it('keeps trying after another save fails while a retry is waiting', async () => {
    const { backend, writes } = fakeBackend();
    let failures = 2;
    const working = backend.writeFile;
    backend.writeFile = async (path, contents) => {
      if (failures-- > 0) throw new Error('Permission denied');
      return working(path, contents);
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.onSaveError = () => {};
    doc.ytext.insert(3, 'd');
    await vi.advanceTimersByTimeAsync(700); // the autosave fails; a retry is now waiting
    await expect(doc.save(false)).rejects.toThrow('Permission denied'); // a save-all fails too
    expect(doc.dirty).toBe(true);

    await vi.advanceTimersByTimeAsync(5000); // the retry must still come, and now the write works
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abcd' }]);
    expect(doc.dirty).toBe(false);
    doc.dispose();
  });

  it('keeps trying after a save made before the debounce fired fails', async () => {
    const { backend, writes } = fakeBackend();
    let failures = 1;
    const working = backend.writeFile;
    backend.writeFile = async (path, contents) => {
      if (failures-- > 0) throw new Error('Permission denied');
      return working(path, contents);
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.onSaveError = () => {};
    doc.ytext.insert(3, 'd');
    await expect(doc.save(false)).rejects.toThrow('Permission denied'); // the build's save-all, before 700 ms
    await vi.advanceTimersByTimeAsync(5000);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'abcd' }]);
    expect(doc.dirty).toBe(false);
    doc.dispose();
  });

  it('does not leave a retry behind when the document is disposed during a failing write', async () => {
    const { backend } = fakeBackend();
    let attempts = 0;
    let fail: (error: Error) => void = () => {};
    backend.writeFile = () => {
      attempts++;
      return new Promise<void>((_resolve, reject) => (fail = reject));
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.ytext.insert(3, 'd');
    const saving = doc.save(false).catch(() => {});
    doc.dispose(); // the tab closes while the write is out
    fail(new Error('Permission denied'));
    await saving;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(attempts).toBe(1);
  });

  it('leaves a debounce that typing set during the failing write where it is', async () => {
    const { backend } = fakeBackend();
    let attempts = 0;
    let fail: (error: Error) => void = () => {};
    backend.writeFile = () => {
      attempts++;
      if (attempts > 1) return Promise.resolve();
      return new Promise<void>((_resolve, reject) => (fail = reject));
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.onSaveError = () => {};
    doc.ytext.insert(3, 'd');
    const saving = doc.save(false).catch(() => {});
    doc.ytext.insert(4, 'e'); // typing goes on while the write is out: the 700 ms debounce is set
    fail(new Error('Permission denied'));
    await saving;
    await vi.advanceTimersByTimeAsync(700); // not five seconds: the newest text is tried at once
    expect(attempts).toBe(2);
    doc.dispose();
  });

  it('stops retrying a failed save once the document is disposed', async () => {
    const { backend } = fakeBackend();
    let attempts = 0;
    backend.writeFile = async () => {
      attempts++;
      throw new Error('read-only');
    };
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.onSaveError = () => {};
    doc.ytext.insert(3, 'd');
    await vi.advanceTimersByTimeAsync(700);
    doc.dispose();
    const before = attempts;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(attempts).toBe(before);
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

  it('saves only once every reason for holding is gone', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'abc', backend);
    doc.ytext.insert(3, 'd');
    doc.holdSaves('disk');
    doc.holdSaves('opening');
    doc.releaseSaves('opening');
    expect(await doc.save()).toBe(false);
    doc.releaseSaves('disk');
    expect(await doc.save()).toBe(true);
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

  it('applies a fix through the ordinary edit path: dirty, saved, and undoable (S6.2)', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'Salt & pepper.\n', backend);
    const applied = doc.applyFix(1, { description: 'Escape as \\&', find: '&', replace: '\\&' });
    expect(applied).toBe(true);
    expect(doc.text()).toBe('Salt \\& pepper.\n');
    expect(doc.dirty).toBe(true); // the same debounced save every keystroke gets, not a side channel
    await vi.advanceTimersByTimeAsync(700);
    expect(writes).toEqual([{ path: 'main.tex', contents: 'Salt \\& pepper.\n' }]);
    doc.undo.undo();
    expect(doc.text()).toBe('Salt & pepper.\n');
    doc.dispose();
  });

  it('declines a fix rather than editing the wrong text when the line does not hold it', () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'Hello.\n', backend);
    const applied = doc.applyFix(1, { description: 'Escape as \\&', find: '&', replace: '\\&' });
    expect(applied).toBe(false);
    expect(doc.text()).toBe('Hello.\n');
    expect(doc.dirty).toBe(false);
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

// R8.1. A file that starts with a byte-order mark (what older Windows editors leave, three bytes `EF BB BF`)
// came in as a `U+FEFF` character at the start of the buffer. The caret can sit in front of it, so a line
// typed at the top of the file went before the mark and the file was written with the mark in the middle,
// which the engine reads as a character in the text: "Missing \begin{document}" over a dot nobody can read.
describe('a file that starts with a byte-order mark (R8.1)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const mark = '\uFEFF';
  const withMark = `${mark}\\documentclass{article}\nbody\n`;

  it('is held without the mark, so nothing can be typed in front of it', () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('main.tex', withMark, backend);
    expect(doc.text()).toBe('\\documentclass{article}\nbody\n');
    doc.dispose();
  });

  it('is written back with the mark first, and a line typed at the top comes after it', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', withMark, backend);
    doc.ytext.insert(0, '% my paper\n'); // the caret at the very start of the file, as Ctrl Home puts it
    await vi.advanceTimersByTimeAsync(700);
    expect(writes).toHaveLength(1);
    expect(writes[0]!.contents).toBe(`${mark}% my paper\n\\documentclass{article}\nbody\n`);
    expect(writes[0]!.contents.indexOf(mark)).toBe(0);
    doc.dispose();
  });

  it('is not rewritten when nothing changed: the three bytes stay as they were', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', withMark, backend);
    expect(await doc.save()).toBe(false);
    expect(writes).toEqual([]);
    doc.dispose();
  });

  it('does not put a mark on a file that had none', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'a\nb\n', backend);
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).toBe('Xa\nb\n');
    doc.dispose();
  });

  it('keeps the mark and a CRLF ending together', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', `${mark}a\r\nb\r\n`, backend);
    expect(doc.text()).toBe('a\nb\n');
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).toBe(`${mark}Xa\r\nb\r\n`);
    doc.dispose();
  });

  it('recognises its own file on disk with or without the mark, and learns a change of it', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', withMark, backend);
    expect(doc.matchesDisk(withMark)).toBe(true);
    expect(doc.matchesDisk(withMark.slice(1))).toBe(true); // saved again by an editor that drops the mark
    expect(doc.matchesDisk(`${withMark}more`)).toBe(false);
    // The file has no mark on disk now, so the next save keeps it that way rather than putting it back.
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).toBe('X\\documentclass{article}\nbody\n');
    doc.dispose();
  });

  it('folds an external change in without calling the mark a change, and learns it from the new text', async () => {
    const { backend, writes } = fakeBackend();
    const diffs: Array<[string, string]> = [];
    const spying: DocumentBackend = {
      ...backend,
      diffOps: async (oldText, newText) => {
        diffs.push([oldText, newText]);
        return backend.diffOps(oldText, newText);
      },
    };
    const doc = new OpenDocument('main.tex', withMark, spying);
    expect(await doc.applyExternal(withMark)).toBe(true); // the same file again: nothing to diff
    expect(diffs).toEqual([]);

    expect(await doc.applyExternal('\\documentclass{article}\nBODY\n')) .toBe(true); // rewritten with no mark
    expect(diffs).toEqual([['\\documentclass{article}\nbody\n', '\\documentclass{article}\nBODY\n']]);
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents.startsWith(mark)).toBe(false);
    doc.dispose();
  });

  it('gives the text as it is on disk when a byte offset from Rust has to be turned into a line', () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('refs.bib', `${mark}@a{k1,}\n`, backend);
    expect(doc.textAsOnDisk()).toBe(`${mark}@a{k1,}\n`);
    doc.dispose();
  });
});

// R7.11. CodeMirror counts a line break as one position and splits on "\r\n"; a Y.Text that holds
// "\r\n" counts two. y-codemirror hands CodeMirror's positions to Yjs unchanged, so every edit after
// the first line break of a CRLF file landed one character early per line break before it.
describe('a file with CRLF line endings (R7.11)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const crlf = 'line one\r\nline two\r\nline three\r\nline four\r\nline five\r\n';

  it('is held as LF, so CodeMirror and the Y.Text agree on every position', () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('main.tex', crlf, backend);
    expect(doc.text()).toBe('line one\nline two\nline three\nline four\nline five\n');
    // The root cause, pinned: the editor's document is built from `doc.text()` and must be as long as the Y.Text.
    expect(EditorState.create({ doc: doc.text() }).doc.length).toBe(doc.ytext.length);
    // ...which it was not for the text as read.
    expect(EditorState.create({ doc: crlf }).doc.length).toBeLessThan(crlf.length);
    doc.dispose();
  });

  it('is written back with CRLF, and an edit in the middle lands where it was made', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', crlf, backend);
    // The start of line 5 in the editor's own counting: four line breaks of one position each.
    const startOfLineFive = 'line one\nline two\nline three\nline four\n'.length;
    doc.ytext.insert(startOfLineFive, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes).toHaveLength(1);
    expect(writes[0]!.contents).toBe('line one\r\nline two\r\nline three\r\nline four\r\nXline five\r\n');
    doc.dispose();
  });

  it('is not rewritten when nothing changed: the bytes on disk stay as they were', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', crlf, backend);
    expect(await doc.save()).toBe(false);
    expect(writes).toEqual([]);
    expect(doc.dirty).toBe(false);
    doc.dispose();
  });

  it('keeps LF files LF', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', 'a\nb\n', backend);
    doc.ytext.insert(2, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).toBe('a\nXb\n');
    doc.dispose();
  });

  it('recognises its own file on disk, whichever line ending that has, and learns a change of it', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', crlf, backend);
    expect(doc.matchesDisk(crlf)).toBe(true);
    expect(doc.matchesDisk(crlf.replace(/\r\n/g, '\n'))).toBe(true); // checked out again with LF
    expect(doc.matchesDisk(crlf + 'more')).toBe(false);
    // The file is LF on disk now, so the next save keeps it LF rather than turning it back.
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).not.toContain('\r');
    doc.dispose();
  });

  it('folds an external CRLF change in without calling the unchanged lines a change', async () => {
    const { backend } = fakeBackend();
    const diffs: Array<[string, string]> = [];
    const spying: DocumentBackend = {
      ...backend,
      diffOps: async (oldText, newText) => {
        diffs.push([oldText, newText]);
        return backend.diffOps(oldText, newText);
      },
    };
    const doc = new OpenDocument('main.tex', crlf, spying);
    expect(await doc.applyExternal(crlf)).toBe(true); // the same file again: nothing to diff
    expect(diffs).toEqual([]);

    const changed = crlf.replace('line three', 'line 3');
    expect(await doc.applyExternal(changed)).toBe(true);
    expect(diffs).toEqual([['line one\nline two\nline three\nline four\nline five\n', 'line one\nline two\nline 3\nline four\nline five\n']]);
    expect(doc.text()).toBe('line one\nline two\nline 3\nline four\nline five\n');
    expect(doc.dirty).toBe(false);
    doc.dispose();
  });

  it('learns a new line ending from an external change that also changes the text', async () => {
    const { backend, writes } = fakeBackend();
    const doc = new OpenDocument('main.tex', crlf, backend);
    await doc.applyExternal('one\ntwo\n'); // rewritten by something that uses LF
    doc.ytext.insert(0, 'X');
    await vi.advanceTimersByTimeAsync(700);
    expect(writes[0]!.contents).toBe('Xone\ntwo\n');
    doc.dispose();
  });

  it('gives the text as it is on disk when a byte offset from Rust has to be turned into a line', () => {
    const { backend } = fakeBackend();
    const doc = new OpenDocument('refs.bib', '@a{k1,}\r\n@b{k2,}\r\n', backend);
    expect(doc.textAsOnDisk()).toBe('@a{k1,}\r\n@b{k2,}\r\n');
    doc.dispose();
  });

  it('decides the line ending by the majority, and LF when there is no line break at all', () => {
    expect(lineEndingOf('a\r\nb\r\nc\n')).toBe('crlf');
    expect(lineEndingOf('a\nb\nc\r\n')).toBe('lf');
    expect(lineEndingOf('no line break')).toBe('lf');
    expect(lineEndingOf('')).toBe('lf');
  });
});
