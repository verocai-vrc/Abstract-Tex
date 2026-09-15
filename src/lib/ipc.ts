// The only file that talks to Tauri. Components import `ipc`, never `@tauri-apps/*`.
//
// Every type here mirrors a Rust struct in src-tauri/src (serde renames fields to camelCase),
// so if a shape changes on one side, change it here in the same commit.

import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

export interface TreeNode {
  name: string;
  /** Project-relative, forward slashes. Stable key for the tree. */
  path: string;
  isDir: boolean;
  children: TreeNode[];
}

export interface ProjectInfo {
  rootDir: string;
  rootFile: string | null;
  buildDir: string;
  tree: TreeNode[];
  /** The \input/\include/\subfile graph's nodes, root first, project-relative (S4.1). Empty
   * when there is no root file. */
  documentFiles: string[];
  /** False when at least one directive in the document could not be resolved. `shouldCompileFor`
   * (paths.ts) then treats every `.tex` file as part of the document rather than risk ignoring
   * one that actually is. */
  documentFilesComplete: boolean;
}

export interface EngineInfo {
  name: string;
  version: string;
  path: string;
}

/** One explained problem from `texlog::Diagnostic` (S2.6): sentences, never a log line. */
export interface Diagnostic {
  /** A short noun phrase naming the mistake: "Underscore used outside maths". */
  title: string;
  /** Complete sentences saying what happened and what to do. */
  explanation: string;
  /** TeX's `l.NN` claim — often approximate, sometimes absent. No file yet: that is S5.2. */
  line: number | null;
  severity: 'error' | 'warning';
  /** The rule that matched, or `null` when the catalog has no explanation and `explanation`
   * is a fallback that quotes TeX. */
  rule: string | null;
  /** TeX's own words, kept for the raw view and for support reports. */
  rawMessage: string;
}

export type CompileEvent =
  | { status: 'started'; generation: number; rootFile: string }
  | { status: 'progress'; generation: number; message: string }
  | {
      status: 'finished';
      generation: number;
      success: boolean;
      pdfPath: string | null;
      logPath: string | null;
      diagnostics: Diagnostic[];
      durationMs: number;
      stderr: string;
    }
  | { status: 'failed'; generation: number; message: string };

/** Anything the language server says without being asked (`src-tauri/src/lsp.rs`).
 *
 * `method` and `params` are LSP's own, passed through untouched: Rust owns the process and the
 * JSON-RPC correlation, TypeScript owns what a method *means* for the editor (DESIGN.md §4.1). */
export type LspEvent =
  | { kind: 'notification'; method: string; params: unknown }
  | { kind: 'request'; id: unknown; method: string; params: unknown }
  /** The server died and came back. It remembers nothing, so open documents must be re-sent. */
  | { kind: 'restarted'; restarts: number }
  | { kind: 'stopped'; message: string };

export interface FsEvent {
  /** Absolute path as the watcher saw it. */
  path: string;
  exists: boolean;
}

/** One CRDT edit from preamble-reconcile: at `index` (UTF-16 units) delete, then insert. */
export interface TextOp {
  index: number;
  delete: number;
  insert: string;
}

/** Where a source line lands in the compiled PDF (S3.4), in PDF points from the page's top-left
 * corner — the same coordinate system pdf.js's viewport uses at scale 1. */
export interface SyncTexForwardResult {
  page: number;
  x: number;
  y: number;
}

/** Where a click in the PDF came from in the source (S3.5). `file` is `null` when the click
 * resolved to something outside the project (a package's own file) that has no tab to open. */
export interface SyncTexInverseResult {
  file: string | null;
  line: number;
}

export const ipc = {
  initialProject: () => invoke<string | null>('initial_project'),
  engineInfo: () => invoke<EngineInfo | null>('engine_info'),
  openProject: (path: string) => invoke<ProjectInfo>('open_project', { path }),
  refreshTree: () => invoke<ProjectInfo>('refresh_tree'),
  readFile: (path: string) => invoke<string>('read_file', { path }),
  writeFile: (path: string, contents: string) => invoke<void>('write_file', { path, contents }),
  createFile: (path: string) => invoke<ProjectInfo>('create_file', { path }),
  setRootFile: (path: string) => invoke<ProjectInfo>('set_root_file', { path }),
  compile: () => invoke<number>('compile'),
  cancelCompile: () => invoke<void>('cancel_compile'),
  readLog: () => invoke<string>('read_log'),
  diffOps: (oldText: string, newText: string) => invoke<TextOp[]>('diff_ops', { old: oldText, new: newText }),

  /** Forward search: a source line to a spot in the last build's PDF (S3.4). Rejects with a
   * sentence — no build yet, or nothing typeset for that line — rather than an engine detail. */
  synctexForward: (file: string, line: number) =>
    invoke<SyncTexForwardResult>('synctex_forward', { query: { file, line } }),
  /** Inverse search: a click in the PDF to a source line (S3.5). */
  synctexInverse: (page: number, x: number, y: number) =>
    invoke<SyncTexInverseResult>('synctex_inverse', { query: { page, x, y } }),

  /** Start TexLab for the open project; resolves to its capabilities. Rejects with a sentence
   * if the binary is missing — the editor keeps working without it. */
  lspStart: () => invoke<Record<string, unknown>>('lsp_start'),
  /** Ask the server something and wait for its answer. */
  lspRequest: <T = unknown>(method: string, params: unknown) => invoke<T>('lsp_request', { method, params }),
  /** Tell the server something. Returns once handed to the bridge, not once the server read it. */
  lspNotify: (method: string, params: unknown) => invoke<void>('lsp_notify', { method, params }),
  /** Answer a request the server made of us, quoting the id from its `lsp` event. */
  lspRespond: (id: unknown, result: unknown) => invoke<void>('lsp_respond', { id, result }),

  /** Native folder picker. Resolves to null if the user cancels. */
  pickFolder: async (): Promise<string | null> => {
    const chosen = await openDialog({ directory: true, multiple: false, title: 'Open a LaTeX project folder' });
    return typeof chosen === 'string' ? chosen : null;
  },

  /** A URL the webview may fetch for a file inside an allowed scope (the build folder). */
  assetUrl: (absolutePath: string) => convertFileSrc(absolutePath),

  onCompile: (handler: (event: CompileEvent) => void): Promise<UnlistenFn> =>
    listen<CompileEvent>('compile', (e) => handler(e.payload)),
  onFsChanged: (handler: (event: FsEvent) => void): Promise<UnlistenFn> =>
    listen<FsEvent>('fs:changed', (e) => handler(e.payload)),
  onLsp: (handler: (event: LspEvent) => void): Promise<UnlistenFn> =>
    listen<LspEvent>('lsp', (e) => handler(e.payload)),
};

export type Ipc = typeof ipc;
