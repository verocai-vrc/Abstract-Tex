// Application state, as Svelte 5 runes. Components read from `app`; only the controller
// (controller.svelte.ts) writes to it. Keeping writes in one place makes the flows in
// DESIGN.md §6 traceable: every state change is a named function there.

import type { Diagnostic, EngineInfo, ProjectInfo } from './ipc';
import type { OpenDocument } from './document';
import { DEFAULT_FILTER, type DrawerFilter } from './drawer';
import type { OutlineItem } from './outline';

export type CompilePhase = 'idle' | 'running' | 'ok' | 'error' | 'failed';

export interface CompileState {
  phase: CompilePhase;
  /** Generation of the build this state describes; older events are ignored. */
  generation: number;
  startedAt: number;
  durationMs: number | null;
  diagnostics: Diagnostic[];
  /** Why a build could not run at all (no engine, spawn failure). */
  message: string | null;
  stderr: string;
  /** Latest engine stderr line while a build runs — a cold package fetch, mainly (DESIGN.md §6). */
  progress: string | null;
}

export interface Conflict {
  path: string;
  diskText: string;
}

class AppState {
  project = $state<ProjectInfo | null>(null);
  /** `undefined` = not probed yet; `null` = no engine found. */
  engine = $state<EngineInfo | null | undefined>(undefined);

  /** Is the language server answering? False when TexLab is missing or has stopped for good.
   * Everything the editor does apart from completion and hover works either way — the status
   * bar says so quietly rather than raising a notice (DESIGN.md §2, commitment 6). */
  lspReady = $state(false);
  /** Why language features are unavailable, for the status bar. `null` when they are. */
  lspMessage = $state<string | null>(null);

  /**
   * Bumped every time the language server publishes diagnostics for any file (S3.3b).
   *
   * A counter, not the diagnostics themselves. The rows live in a plain, non-reactive
   * `LspDiagnosticStore` in the controller; this is the only reactive thing about them, so
   * `Editor.svelte`'s gutter effect has something to depend on *without* depending on a map it
   * would then cause to be re-read and re-written (MEMORY: `effect_update_depth_exceeded`).
   *
   * Note what it deliberately does not touch: `errorCount` and `warningCount` below stay derived
   * from `compile.diagnostics` alone. LSP diagnostics reach the gutter and nothing else.
   */
  lspDiagnosticsVersion = $state(0);

  /**
   * Every file currently open in a tab, keyed by project-relative path (S2.3). Not deeply
   * reactive — each value is a class with its own listeners — so the controller replaces the
   * whole map, rather than mutating it in place, whenever a tab opens or closes; see
   * `documents.ts`, which owns the real bookkeeping this is a reactive snapshot of.
   */
  docs = $state.raw<Map<string, OpenDocument>>(new Map());
  /** Tab order, left to right. */
  openTabs = $state<string[]>([]);
  activePath = $state<string | null>(null);
  /** Paths with edits not yet on disk. Drives the dot on a tab and on its row in the tree. */
  dirtyPaths = $state<Set<string>>(new Set());

  /** The document behind the active tab, or `null` when no folder — or no file in it — is open. */
  activeDoc = $derived(this.activePath !== null ? this.docs.get(this.activePath) ?? null : null);
  /** Whether the *active* tab has edits not yet on disk. A background tab's dirtiness lives in
   * `dirtyPaths` instead; most of the UI only cares about the one currently in the editor. */
  dirty = $derived(this.activePath !== null && this.dirtyPaths.has(this.activePath));

  compile = $state<CompileState>({
    phase: 'idle',
    generation: 0,
    startedAt: 0,
    durationMs: null,
    diagnostics: [],
    message: null,
    stderr: '',
    progress: null,
  });

  /** How many of the last build's diagnostics are errors, and how many are warnings. The drawer
   * and the status bar both phrase themselves around these two numbers. */
  errorCount = $derived(this.compile.diagnostics.filter((d) => d.severity === 'error').length);
  warningCount = $derived(this.compile.diagnostics.filter((d) => d.severity === 'warning').length);

  /** Asset URL of the last *successful* PDF. Stays put when a build fails (DESIGN.md §6). */
  pdfUrl = $state<string | null>(null);

  drawerOpen = $state(false);
  /** The drawer's severity and "this file" filter (S6.3). Kept for the session, not reset per
   * build: an author chasing one error under "errors only" is still chasing it after rebuilding. */
  drawerFilter = $state<DrawerFilter>({ ...DEFAULT_FILTER });

  showRawLog = $state(false);
  /** `main.log` as read for the raw view; `''` until the view has been opened. Re-read every time
   * a build finishes while the view is showing, so it always describes the cards beside it. */
  rawLog = $state('');
  /** Which diagnostic's own words the raw view should scroll to and highlight (S6.3's per-card
   * "Raw log"). `nonce` makes a repeat request for the same card distinct, as `jumpRequest` does.
   * Cleared when a build finishes: the excerpt it named belongs to the log that build replaced. */
  rawLogFocus = $state<{ rawMessage: string; nonce: number } | null>(null);

  conflict = $state<Conflict | null>(null);
  notice = $state<string | null>(null);

  /** The `Ctrl P` file list (S2.4). */
  quickOpenVisible = $state(false);

  /** The "link a Zotero collection" picker (S8.2), opened from the status bar. */
  zoteroLinkVisible = $state(false);

  /** The `Ctrl K` command palette (S4.3): every action, plus the open file list and the active
   * file's outline, searched together. */
  commandPaletteVisible = $state(false);

  /** Focus mode (S4.5): dim every paragraph but the one under the cursor. Purely a CodeMirror
   * decoration — see `editor/focus.ts` — never persisted across restarts; every session starts
   * with both writing modes off. */
  focusModeEnabled = $state(false);
  /** Typewriter mode (S4.5): keep the cursor's line vertically centred as the author types or
   * moves. Purely a scroll-position effect — see `editor/typewriter.ts`. Independent of focus
   * mode; either, both, or neither can be on. */
  typewriterModeEnabled = $state(false);

  /** A request for the editor to move the cursor. `nonce` makes repeat requests distinct. */
  jumpRequest = $state<{ line: number; nonce: number } | null>(null);

  /** The Document map's rows for the active file (S4.2). Raw — like `docs`, `refreshOutline`
   * replaces this wholesale rather than mutating it in place, since it is recomputed from
   * scratch on every scan rather than patched incrementally. */
  outline = $state.raw<OutlineItem[]>([]);

  /** A request for the PDF pane to scroll to and highlight a point (S3.4's forward search).
   * `nonce` makes repeat requests to the same spot distinct, matching `jumpRequest`. */
  syncTexScrollRequest = $state<{ page: number; x: number; y: number; nonce: number } | null>(null);

  projectName = $derived(this.project ? (this.project.rootDir.split(/[\\/]/).filter(Boolean).pop() ?? '') : '');
}

export const app = new AppState();
