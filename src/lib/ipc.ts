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
  /** TeX's `l.NN` claim — often approximate, sometimes absent. */
  line: number | null;
  /** The file open when this was printed (S5.6), resolved from the log's own `(`/`)` trail —
   * `null` only when nothing was open at that point. Project-relative in spelling only when the
   * log itself reported it that way; nothing here re-resolves it against the file tree yet. */
  file: string | null;
  severity: 'error' | 'warning';
  /** The rule that matched, or `null` when the catalog has no explanation and `explanation`
   * is a fallback that quotes TeX. */
  rule: string | null;
  /** TeX's own words, kept for the raw view and for support reports. */
  rawMessage: string;
  /** A safe, unambiguous edit (S5.5), or `null` — most rules have none. *Applying* it is
   * S6.2's job; nothing reads this field yet. */
  fix: Fix | null;
}

/** One unambiguous edit from `texlog::Fix` (S5.5): a literal find/replace on the diagnosed
 * line, not a byte offset — `texlog` never reads the `.tex` source, only the log. */
export interface Fix {
  /** Shown on the button, e.g. "Escape as \_". */
  description: string;
  find: string;
  replace: string;
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

/** The project-wide bibliography index (S7.2, `src-tauri/src/bibliography.rs`): which `.bib`
 * files the document names, every entry in them, and every `\cite` key in the document's `.tex`
 * files. Rebuilt from disk whenever a `.bib` or `.tex` changes and sent whole as the
 * `bibliography:changed` event; the `.bib` files themselves stay the truth (DESIGN.md §2). */
export interface BibliographyIndex {
  /** In the order the document names them, each once. */
  files: BibFile[];
  /** File order. A key defined twice appears twice — that is a health-check finding (S8.3),
   * not something to hide here. */
  entries: BibEntrySummary[];
  /** Include-graph order, then line order. */
  citations: Citation[];
  /** Whether any `.tex` file has `\nocite{*}` — "treat every entry as cited". S8.3's never-cited
   * check reads this on the Rust side; the frontend has no reason to read it itself today, but it
   * rides along since `BibliographyIndex` is sent whole. */
  hasNociteStar: boolean;
}

export interface BibFile {
  /** Project-relative, forward slashes; or the argument as written when it points outside
   * the project. */
  path: string;
  /** False when the document names a file that is not on disk (or is outside the project). */
  exists: boolean;
  entryCount: number;
  /** One sentence per item `texbib` could not parse, with the byte offset it gave up at. */
  problems: Array<{ message: string; at: number }>;
  /** Why the index lists it (S8.6): a `ibliography`-style command inside or outside the
   * project, at that `.tex` file and line, or a linked Zotero export from `preamble.toml`. */
  origin: BibOrigin;
}

export type BibOrigin =
  | { kind: 'named'; file: string; line: number }
  | { kind: 'outside'; file: string; line: number }
  | { kind: 'linked' };

/** What `\cite` completion (S7.3) shows: resolved field text, names not yet split. */
export interface BibEntrySummary {
  key: string;
  /** As written (`article`, `Article`); compare ignoring case. */
  entryType: string;
  /** `author`, else `editor`, else the same from a `crossref` parent; `null` when none. */
  author: string | null;
  /** `year`, else the year of a BibLaTeX `date`, else inherited; `null` when none. */
  year: string | null;
  title: string | null;
  /** Project-relative path of the `.bib` the entry is in. */
  file: string;
  /** The whole entry's span in that file, in *bytes* — not UTF-16 units. Convert before
   * handing it to CodeMirror. */
  span: { start: number; end: number };
}

/** One key inside one `\cite{...}`. */
export interface Citation {
  key: string;
  /** Project-relative path of the `.tex` file. */
  file: string;
  /** 1-based line of the `\cite` command. */
  line: number;
}

/** How much the author should care about a health `Finding` — the same two words
 * `Diagnostic['severity']` uses, kept as its own type because a bibliography finding is not a
 * compile diagnostic. */
export type HealthSeverity = 'error' | 'warning';

/** Where a `Finding` points: a line in a `.tex` file, a `.bib` entry's byte span, or a file that
 * is not on disk (S8.6: a linked export not yet written) — nothing to open, but still a path the
 * panel can group by. */
export type Jump =
  | { kind: 'texLine'; file: string; line: number }
  | { kind: 'bibEntry'; file: string; span: { start: number; end: number } }
  | { kind: 'missingFile'; file: string };

/** One bibliography health-check result (S8.3, DESIGN.md §5.4): a sentence, a severity, and a
 * place to click, never a raw anything (DESIGN.md §2 rule 3). `rule` is one of
 * `'missing-bib-file' | 'undefined-citation' | 'never-cited' | 'duplicate-doi' | 'missing-field' |
 * 'page-range-dash'`,
 * left as `string` here since nothing on this side branches on it beyond display. */
export interface Finding {
  rule: string;
  severity: HealthSeverity;
  message: string;
  jump: Jump;
}

/** Which acquisition source (S7.4/S7.5) a pasted string was recognised as. */
export type PasteKind = 'doi' | 'arxiv' | 'isbn';

/** What paste-to-cite (S7.6) resolved a paste to. */
export interface PasteCiteResult {
  /** The citation key to insert as `\cite{key}` — either reused or freshly generated. */
  key: string;
  /** `false` when `key` already named an entry in the bibliography (nothing was written);
   * `true` when a new entry was appended. */
  created: boolean;
}

/** Whether Zotero, with the Better BibTeX plugin, answers on `127.0.0.1:23119` (S8.1,
 * `crates/texbib/src/acquire/zotero.rs`). `'not_running'` also covers "Zotero is not
 * installed" — this app cannot and does not try to tell the two apart. */
export type ZoteroStatus = 'not_running' | 'no_better_bibtex' | 'ready';

/** One Zotero library ("group" — the personal library is one too), with its collection tree
 * (S8.2). Mirrors `texbib::acquire::zotero::Library`. */
export interface ZoteroLibrary {
  id: number;
  name: string;
  collections: ZoteroCollection[];
}

/** One collection inside a library. `path` is what `linkZoteroCollection`'s `collectionPath`
 * argument wants, verbatim. */
export interface ZoteroCollection {
  name: string;
  path: string;
  children: ZoteroCollection[];
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

  /** The bibliography index, built fresh from disk (S7.2). Empty when there is no root file. */
  bibliographyIndex: () => invoke<BibliographyIndex>('bibliography_index'),
  /** The five bibliography health checks (S8.3), built fresh from disk. Empty when there is no
   * root file. A second read of the same files `bibliographyIndex` reads, not derived from its
   * result — call after a `bibliography:changed` event, not instead of listening for one. */
  bibliographyHealth: () => invoke<Finding[]>('bibliography_health'),

  /** Which of `doi` / `arxiv` / `isbn` a pasted string looks like, or `null` for plain text —
   * S7.6's paste-to-cite, checked before offering "Cite" so a normal paste is never delayed by a
   * network round trip. */
  identifyPaste: (pasted: string) => invoke<PasteKind | null>('identify_paste', { pasted }),
  /** Fetch the identified entry, deduplicate against the project's bibliography, and — on a
   * miss — append a new entry to the first `.bib` file the document names. Rejects with a
   * sentence: no network, nothing found for the identifier, or no `.bib` file to append to. */
  pasteCite: (pasted: string) => invoke<PasteCiteResult>('paste_cite', { pasted }),

  /** Probe once for a running Zotero + Better BibTeX (S8.1). Not polled automatically — call
   * this when the author opens whatever UI offers linking a collection (S8.2). */
  detectZotero: () => invoke<ZoteroStatus>('detect_zotero'),

  /** Every library and collection Zotero currently has (S8.2), for the "pick a collection" UI.
   * Rejects with a sentence if Zotero/Better BibTeX is not reachable. */
  listZoteroLibraries: () => invoke<ZoteroLibrary[]>('list_zotero_libraries'),
  /** Link a collection (S8.2): `outputPath` is project-relative, where the auto-exported `.bib`
   * should live; Better BibTeX keeps it current from then on, and this app only ever reads it. */
  linkZoteroCollection: (collectionPath: string, outputPath: string) =>
    invoke<void>('link_zotero_collection', { collectionPath, outputPath }),
  /** Unlink (S8.7): drop `path` from `preamble.toml`'s linked `.bib` files. Leaves the file on
   * disk and Zotero untouched, so it works with Zotero closed. */
  unlinkBibFile: (path: string) => invoke<void>('unlink_bib_file', { path }),

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
  /** A `.bib` or `.tex` changed — on disk or through our own `writeFile` — and the index was
   * rebuilt. The payload is the whole new index, so there is nothing to fetch afterwards. */
  onBibliographyChanged: (handler: (index: BibliographyIndex) => void): Promise<UnlistenFn> =>
    listen<BibliographyIndex>('bibliography:changed', (e) => handler(e.payload)),
};

export type Ipc = typeof ipc;
