// The session document (DESIGN.md §5.6): one open file as a Yjs text, with a debounced save
// back to disk and a way to fold external changes in as CRDT edits.
//
// Disk is authoritative. This object is a session-lifetime view of the file, never storage.
// It knows nothing about CodeMirror; the editor binds to `ytext` through y-codemirror.next.
// It also knows nothing about Tauri: it talks to a `DocumentBackend` so tests can use a fake.

import * as Y from 'yjs';
import { locateFix } from './fix';
import type { Fix, TextOp } from './ipc';

/** Transaction origins that are *not* the author typing. */
export const ORIGIN_LOAD = 'abstract-tex:load';
export const ORIGIN_EXTERNAL = 'abstract-tex:external';

/** How long the keyboard must be idle before we write to disk (DESIGN.md §3). */
export const SAVE_DELAY_MS = 700;

export interface DocumentBackend {
  writeFile(path: string, contents: string): Promise<void>;
  diffOps(oldText: string, newText: string): Promise<TextOp[]>;
  /** Called after a successful write. The controller uses it to trigger a compile. */
  afterSave?(path: string): void;
}

/** The outcome of the decision table below. */
export type ExternalChange = 'ignore' | 'apply' | 'conflict' | 'vanished';

/** Everything we know about a file the watcher reported. */
export interface ExternalChangeFacts {
  /** Is this the file currently in the editor? Other files only refresh the tree. */
  isOpenFile: boolean;
  existsOnDisk: boolean;
  /** Does the disk text already equal the buffer? (Our own write, echoed back.) */
  sameAsBuffer: boolean;
  bufferDirty: boolean;
}

/**
 * What to do when a file changes underneath us (DESIGN.md §5.6). The whole policy is this
 * table, so that the one rule that must never bend — a dirty buffer plus a changed file is a
 * question, not a merge — is visible in four lines instead of spread through the controller.
 *
 * | open file? | on disk? | same as buffer? | dirty? | decision |
 * |---|---|---|---|---|
 * | no  | –   | –   | –   | `ignore`   |
 * | yes | no  | –   | –   | `vanished` |
 * | yes | yes | yes | –   | `ignore`   |
 * | yes | yes | no  | no  | `apply`    |
 * | yes | yes | no  | yes | `conflict` |
 */
export function decideExternalChange(facts: ExternalChangeFacts): ExternalChange {
  if (!facts.isOpenFile) return 'ignore';
  if (!facts.existsOnDisk) return 'vanished';
  if (facts.sameAsBuffer) return 'ignore';
  return facts.bufferDirty ? 'conflict' : 'apply';
}

export class OpenDocument {
  readonly ydoc = new Y.Doc();
  readonly ytext: Y.Text;
  readonly undo: Y.UndoManager;

  /** True while the buffer holds edits not yet written to disk. */
  dirty = false;
  onDirtyChange?: (dirty: boolean) => void;

  /** Fired on *every* observed change to `ytext`, including `ORIGIN_LOAD` and `ORIGIN_EXTERNAL` —
   * the two origins `onChange` below deliberately does not dirty the buffer for. The Document
   * map (S4.2) needs to notice a `git checkout` that changes the outline under the author's feet
   * even though it is not a dirty-buffer event; the controller is what debounces this before it
   * touches anything expensive, keeping it off the <16 ms keystroke path (DESIGN.md §2). */
  onTextChange?: () => void;

  /**
   * The text we believe is on disk, or `null` for "nothing on disk matches this buffer" — which
   * is the state after the file is deleted underneath us. `null` is not the same as the empty
   * string: an empty file on disk is a real state whose save we should skip, a missing file is
   * one whose save we must not.
   */
  private lastSavedText: string | null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;
  private savesHeld = false;

  constructor(
    readonly path: string,
    initialText: string,
    private readonly backend: DocumentBackend,
    private readonly saveDelayMs = SAVE_DELAY_MS,
  ) {
    this.ytext = this.ydoc.getText('content');
    this.ydoc.transact(() => this.ytext.insert(0, initialText), ORIGIN_LOAD);
    this.lastSavedText = initialText;

    // The undo manager tracks the author's own edits. y-codemirror adds its own origin to it;
    // loads and external changes use origins it never tracks, so Ctrl-Z cannot undo a
    // `git checkout` that happened while the author was away.
    this.undo = new Y.UndoManager(this.ytext, { captureTimeout: 500 });

    this.ytext.observe((_event, transaction) => this.onChange(transaction.origin));
  }

  text(): string {
    return this.ytext.toString();
  }

  private onChange(origin: unknown) {
    if (this.disposed) return;
    // Every observed change reaches the outline, load and external changes included; only the
    // dirty/save path below cares which origin this was.
    this.onTextChange?.();
    if (origin === ORIGIN_LOAD || origin === ORIGIN_EXTERNAL) return;
    this.setDirty(true);
    this.scheduleSave();
  }

  private setDirty(dirty: boolean) {
    if (this.dirty === dirty) return;
    this.dirty = dirty;
    this.onDirtyChange?.(dirty);
  }

  private scheduleSave() {
    if (this.savesHeld) return;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      void this.save();
    }, this.saveDelayMs);
  }

  /**
   * Stop writing to disk until `releaseSaves`, cancelling any pending debounce. The controller
   * holds saves while it decides what an external change means, and keeps them held for as long
   * as a conflict bar is on screen: otherwise the 700 ms timer would fire behind the author's
   * back and answer the question for them, which is exactly the automatic merge DESIGN.md §5.6
   * forbids.
   */
  holdSaves(): void {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    this.savesHeld = true;
  }

  /** Allow writes again, rescheduling the debounce if the buffer is still dirty. */
  releaseSaves(): void {
    if (!this.savesHeld) return;
    this.savesHeld = false;
    if (this.dirty) this.scheduleSave();
  }

  /**
   * Forget what is on disk without touching the buffer. Called when the file was deleted or
   * renamed away underneath us: the text is still the author's, but no file now holds it, so
   * the next save has to write even though the buffer has not changed since the last one.
   */
  forgetDiskState(): void {
    this.lastSavedText = null;
    this.setDirty(true);
  }

  /**
   * Write now if anything changed. Returns whether a write happened. Safe to call any time.
   *
   * `notify = false` skips the `afterSave` callback — the hook the controller uses to trigger a
   * recompile after a save. `DocumentManager.saveAll` passes it when a caller (`triggerCompile`,
   * S2.3's save-all-on-compile) is about to compile anyway once every tab is flushed: without
   * it, each tab's own save would *also* call `afterSave` and kick off another `triggerCompile`,
   * which calls `saveAll` again while the first one is still in flight — redundant at best, and
   * a real race at worst, since a second `save()` starting on a tab whose write has not yet
   * landed can send the same edit to disk twice.
   */
  async save(notify = true): Promise<boolean> {
    if (this.savesHeld) return false;
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    const snapshot = this.text();
    if (snapshot === this.lastSavedText) {
      this.setDirty(false);
      return false;
    }
    await this.backend.writeFile(this.path, snapshot);
    this.lastSavedText = snapshot;
    // Typing may have continued during the write. If so, stay dirty and let the debounce
    // timer (already restarted by onChange) take care of it.
    if (this.text() === snapshot) this.setDirty(false);
    if (notify) this.backend.afterSave?.(this.path);
    return true;
  }

  /**
   * Fold a new on-disk version into the document as CRDT edits, preserving undo history and
   * any future remote peers' positions. The caller must have decided this is safe
   * (see `decideExternalChange`); this method never checks `dirty`.
   */
  async applyExternal(diskText: string): Promise<void> {
    // The diff is computed asynchronously (in Rust). If the author types in between, the ops
    // would land on the wrong indices, so we re-check and retry a couple of times.
    for (let attempt = 0; attempt < 3; attempt++) {
      const current = this.text();
      if (current === diskText) break;
      const ops = await this.backend.diffOps(current, diskText);
      if (this.text() !== current) continue;
      this.ydoc.transact(() => {
        for (const op of ops) {
          if (op.delete > 0) this.ytext.delete(op.index, op.delete);
          if (op.insert.length > 0) this.ytext.insert(op.index, op.insert);
        }
      }, ORIGIN_EXTERNAL);
      break;
    }
    this.lastSavedText = diskText;
    this.setDirty(this.text() !== diskText);
  }

  /**
   * Apply a rule catalog fix (S5.5's `Fix`, S6.2 makes it real): find `fix.find` on `line` and
   * replace it, through an ordinary `ydoc.transact` with no explicit origin — the same origin
   * every keystroke uses, so `Y.UndoManager`'s default `trackedOrigins` (which is exactly `{
   * null }`) picks it up like any other edit. That is what makes this undoable with a plain
   * Ctrl-Z, and what makes it dirty the buffer and schedule a save through the ordinary
   * `onChange` path below, rather than needing a second, parallel "apply an edit" pipeline.
   * Returns whether anything actually changed: `locateFix` returning `null` means the line the
   * diagnostic named does not hold what the fix expects, and this declines rather than editing
   * the wrong text (see `fix.ts`'s own module doc for why that can happen).
   */
  applyFix(line: number, fix: Fix): boolean {
    const edit = locateFix(this.text(), line, fix);
    if (!edit) return false;
    this.ydoc.transact(() => {
      this.ytext.delete(edit.from, edit.to - edit.from);
      this.ytext.insert(edit.from, edit.insert);
    });
    return true;
  }

  /**
   * Replace `from`–`to` with `insert` in one transaction, so Ctrl Z undoes it in one step. The
   * assistant's accepted changes arrive this way (S12.3b): in place, through the normal undo stack.
   * Returns `false` and changes nothing if the text there is no longer `expected` — the author kept
   * typing while the model thought — because an edit to text that has moved on is an edit to the
   * wrong text.
   */
  replaceRange(from: number, to: number, insert: string, expected: string): boolean {
    if (to < from || to > this.ytext.length || this.ytext.toString().slice(from, to) !== expected) return false;
    // The undo manager folds edits made within `captureTimeout` into one step. Cutting the history
    // on both sides makes the assistant's edit exactly one step: Ctrl Z takes back the suggestion
    // and not the sentence typed just before it.
    this.undo.stopCapturing();
    this.ydoc.transact(() => {
      this.ytext.delete(from, to - from);
      this.ytext.insert(from, insert);
    });
    this.undo.stopCapturing();
    return true;
  }

  dispose() {
    this.disposed = true;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.undo.destroy();
    this.ydoc.destroy();
  }
}
