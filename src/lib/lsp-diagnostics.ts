// The language server's diagnostics, per file, between the wire and the editor's gutter (S3.3b).
//
// TexLab pushes `textDocument/publishDiagnostics` unasked, whenever it has reparsed a file. This
// module owns the resulting map — URI → rows the gutter can draw — and the mapping from LSP's
// vocabulary (0-based lines, numeric severities, `file://` URIs) into the editor's (1-based
// lines, two named severities, absolute paths).
//
// What it must never do:
//
//   * Reach into `app`, `$state`, or Tauri. It is a plain class so `lsp-diagnostics.test.ts` can
//     exercise every case with no Svelte runtime and no server; the controller is the only thing
//     that knows this store exists, and the only thing that mirrors its `version` into a rune.
//   * Feed `app.compile.diagnostics`, the drawer, or `errorCount`/`warningCount`. That is a
//     decision, not an omission — see the note on `EditorDiagnostic` below.
//   * Spell a URI itself. `pathToUri` from `lsp.ts` is the one URI producer in the app; a second
//     spelling would differ on Windows drive-letter case and every lookup here would miss.

import { pathToUri } from './lsp';
import type { LspDiagnostic, Position, PublishDiagnosticsParams } from './lsp-protocol';

/**
 * One row the gutter can draw, in the editor's own units.
 *
 * Deliberately *not* the `Diagnostic` from `ipc.ts` that the drawer and the status bar consume.
 * Those come from the log parser and carry an explanation written by the rule catalog
 * (DESIGN.md §5.2); these are TexLab's own strings. Mixing them would put a raw server message
 * into a list whose contract is an explained sentence — commitment 3 — and would make
 * `errorCount` flicker per keystroke against a compile counter that updates per build
 * (commitment 2). So LSP diagnostics reach the gutter and nothing else. Squiggles under the
 * range are S3.3d; hover text over a marker rides S3.3c's hover plumbing.
 */
export interface EditorDiagnostic {
  severity: 'error' | 'warning';
  /** The message's first sentence, for the gutter dot's tooltip. */
  title: string;
  /** 1-based, the way CodeMirror counts lines. `range.start.line + 1`. */
  startLine: number;
  /** The original 0-based LSP range, kept whole for S3.3d's squiggles. */
  from: Position;
  to: Position;
  /** `Diagnostic.source` off the wire, or `null`. Nothing renders it yet. */
  source: string | null;
}

/** Longest tooltip worth showing. Past this a gutter tooltip is a paragraph hovering over the
 * text, which is the thing §5.2 exists to avoid. */
const TITLE_LIMIT = 120;

/**
 * LSP's numeric severity → the two the gutter knows how to colour, or `null` to drop the row.
 *
 * 3 (Information) and 4 (Hint) are dropped rather than folded into 'warning': TexLab emits very
 * few, and neither has a visual in `diagnostics.ts` yet. Inventing a third dot colour is a
 * §5.x interface decision, not something to slip in under a merge loop.
 *
 * An *absent* severity is a warning, not an error. The spec leaves the choice to the client, and
 * a problem nobody was willing to name must not arrive wearing the loudest colour we have.
 */
export function severityOf(severity: number | undefined): EditorDiagnostic['severity'] | null {
  if (severity === 1) return 'error';
  if (severity === 2) return 'warning';
  if (severity === undefined) return 'warning';
  return null;
}

/**
 * The message's first sentence, trimmed and capped.
 *
 * No rule catalog runs over this: LSP messages are already sentences about the document, which
 * is exactly what §5.2 asks a diagnostic to be, so there is nothing to translate. The only work
 * is keeping a multi-paragraph message from becoming a multi-paragraph tooltip.
 *
 * "First sentence" means up to the first `. ` or a `.` that ends the string — not any `.`, or
 * `\ref{fig.1} is undefined` would be cut mid-command.
 */
export function titleOf(message: unknown): string {
  const text = typeof message === 'string' ? message.trim() : '';
  const firstLine = text.split(/\r?\n/, 1)[0]?.trim() ?? '';
  const sentenceEnd = firstLine.search(/\.(\s|$)/);
  const sentence = sentenceEnd === -1 ? firstLine : firstLine.slice(0, sentenceEnd + 1);
  if (sentence.length <= TITLE_LIMIT) return sentence;
  return `${sentence.slice(0, TITLE_LIMIT - 1).trimEnd()}…`;
}

/** One wire diagnostic → one gutter row, or `null` for a severity we do not draw. Exported for
 * its test; `publish` is the only caller in the app. */
export function toEditorDiagnostic(diagnostic: LspDiagnostic): EditorDiagnostic | null {
  const severity = severityOf(diagnostic.severity);
  if (severity === null) return null;
  const start = diagnostic.range?.start;
  const end = diagnostic.range?.end ?? start;
  if (!start || typeof start.line !== 'number') return null;
  return {
    severity,
    title: titleOf(diagnostic.message),
    // The single most likely bug in this loop: LSP counts lines from 0, CodeMirror from 1.
    startLine: start.line + 1,
    from: start,
    to: end,
    source: typeof diagnostic.source === 'string' ? diagnostic.source : null,
  };
}

/**
 * Every file's current diagnostics, as last published.
 *
 * A plain class with a `version` counter rather than a rune: the counter is what the controller
 * mirrors into `app`, so `Editor.svelte`'s effect has something reactive to depend on without
 * this module importing Svelte and without the effect reading the map it would then write
 * (MEMORY: `effect_update_depth_exceeded`).
 */
export class LspDiagnosticStore {
  /** Keyed by URI, because that is what the server addresses files by and what `pathToUri`
   * produces. Never keyed by path: two spellings of the same file is the bug this avoids. */
  private readonly byUri = new Map<string, EditorDiagnostic[]>();

  /** Bumped on every `publish` and every non-empty `clear`. Only ever read for equality. */
  version = 0;

  /**
   * Replace this URI's diagnostics wholesale — `publishDiagnostics` is a full state for one
   * file, never a delta.
   *
   * An empty `diagnostics` array is the server saying "this file is clean now" and is the case
   * that rots silently: skipping the call would leave yesterday's dots on a fixed file forever.
   * So the key is deleted, and the version still bumps.
   *
   * A publish for a URI with no open tab is stored anyway. TexLab publishes for `\input` files
   * it parsed on its own; a map entry costs nothing and S4.x's project-wide problem list will
   * want exactly these.
   */
  publish(params: PublishDiagnosticsParams): void {
    const rows: EditorDiagnostic[] = [];
    for (const diagnostic of params.diagnostics) {
      const row = toEditorDiagnostic(diagnostic);
      if (row) rows.push(row);
    }
    if (rows.length === 0) this.byUri.delete(params.uri);
    else this.byUri.set(params.uri, rows);
    this.version += 1;
  }

  /** What the gutter should draw for this file. Empty when the server has said nothing about it,
   * and empty after a publish that cleared it — the caller cannot tell the two apart and has no
   * reason to. */
  forPath(absolutePath: string): EditorDiagnostic[] {
    return this.byUri.get(pathToUri(absolutePath)) ?? [];
  }

  /** URIs with at least one diagnostic. For tests and for S4.x's problem list. */
  uris(): string[] {
    return [...this.byUri.keys()];
  }

  /**
   * Forget everything. Called when the server stops and again when it restarts, before
   * `resync()` — a dead process's opinions must not outlive it, and a fresh one will republish
   * for every file it is re-sent. Without this, a diagnostic the author already fixed sits in
   * the gutter until the next publish for that exact file, which for a file that is now clean
   * never comes.
   */
  clear(): void {
    if (this.byUri.size === 0) return;
    this.byUri.clear();
    this.version += 1;
  }
}
