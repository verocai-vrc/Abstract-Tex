// The set of files open in tabs (S2.3). Each open file gets its own `OpenDocument` — its own
// Y.Doc, its own undo history, its own save debounce — that lives for as long as the tab does,
// not just while it happens to be the active one. Switching tabs must never re-read the file
// from disk or lose an edit that has not been saved yet; only closing a tab does.
//
// Like `document.ts`, this module knows nothing about Svelte or Tauri: `controller.svelte.ts`
// is the only thing that calls it, and copies what changed into the reactive `app` state.

import { OpenDocument, type DocumentBackend } from './document';

export class DocumentManager {
  private readonly docs = new Map<string, OpenDocument>();
  /** Tab order, left to right. A `Map` has no order guarantee worth relying on for that. */
  private order: string[] = [];

  constructor(private readonly backend: DocumentBackend) {}

  isOpen(path: string): boolean {
    return this.docs.has(path);
  }

  get(path: string): OpenDocument | undefined {
    return this.docs.get(path);
  }

  /** Tab order, left to right. */
  tabs(): readonly string[] {
    return this.order;
  }

  /**
   * Open a new tab. The caller must check `isOpen` first: calling this for a path that is
   * already open would create a second `Y.Doc` for the same file and silently orphan the live
   * one — discarding whatever the author had not yet saved in it, with nothing to undo it.
   */
  open(path: string, initialText: string): OpenDocument {
    const doc = new OpenDocument(path, initialText, this.backend);
    this.docs.set(path, doc);
    this.order.push(path);
    return doc;
  }

  /**
   * Close one tab, disposing its `Y.Doc`. Returns the path that should become active next —
   * the tab sliding into the closed one's place, biased left at the end of the row — or `null`
   * if none remain. Does nothing, and returns `null`, if `path` was not open.
   */
  close(path: string): string | null {
    const index = this.order.indexOf(path);
    if (index === -1) return null;
    this.docs.get(path)?.dispose();
    this.docs.delete(path);
    this.order.splice(index, 1);
    if (this.order.length === 0) return null;
    // `Math.min` keeps the index in bounds after the splice above, so this is always a real
    // path; `noUncheckedIndexedAccess` cannot see that, hence the assertion.
    return this.order[Math.min(index, this.order.length - 1)]!;
  }

  /** Close every open tab. Used when a different folder is opened. */
  closeAll(): void {
    for (const doc of this.docs.values()) doc.dispose();
    this.docs.clear();
    this.order = [];
  }

  /** Paths with edits not yet written to disk, in tab order. */
  dirtyPaths(): string[] {
    return this.order.filter((path) => this.docs.get(path)!.dirty);
  }

  /**
   * Save every open tab that has something to save. A compile reads whatever is on disk, so a
   * background tab's unsaved edits have to land there too, not only the active tab's — this is
   * what "save-all on compile" means. One tab failing to save must not stop the others: each
   * gets its own `.catch`, and the first error, if any, is what the caller sees.
   *
   * Saves with `notify = false`: the caller (`triggerCompile`) is about to compile once every
   * tab is flushed, so a per-tab `afterSave` here would only queue up redundant recompiles.
   */
  async saveAll(): Promise<void> {
    let firstError: unknown;
    await Promise.all(
      this.order.map(async (path) => {
        try {
          await this.docs.get(path)!.save(false);
        } catch (error) {
          firstError ??= error;
        }
      }),
    );
    if (firstError !== undefined) throw firstError;
  }
}
