// Application state, as Svelte 5 runes. Components read from `app`; only the controller
// (controller.svelte.ts) writes to it. Keeping writes in one place makes the flows in
// DESIGN.md §6 traceable: every state change is a named function there.

import type { Diagnostic, EngineInfo, ProjectInfo } from './ipc';
import type { OpenDocument } from './document';

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
  showRawLog = $state(false);
  rawLog = $state('');

  conflict = $state<Conflict | null>(null);
  notice = $state<string | null>(null);

  /** The `Ctrl P` file list (S2.4). The seed of the command palette, S4.3. */
  quickOpenVisible = $state(false);

  /** A request for the editor to move the cursor. `nonce` makes repeat requests distinct. */
  jumpRequest = $state<{ line: number; nonce: number } | null>(null);

  projectName = $derived(this.project ? (this.project.rootDir.split(/[\\/]/).filter(Boolean).pop() ?? '') : '');
}

export const app = new AppState();
