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
};

export type Ipc = typeof ipc;
