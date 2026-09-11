// Keeping the language server's idea of each file in step with the editor's (S3.2, frontend half).
//
// TexLab can only answer questions about documents it has been told about, and it counts
// versions: every `didChange` must carry a number higher than the last for that file, or the
// server quietly stops trusting what it has. This module owns exactly that bookkeeping —
// which files are open, and what version each is on.
//
// It is deliberately Tauri- and Svelte-free, like `DocumentManager`: it takes a transport and
// calls it. That is what lets `lsp.test.ts` run the restart case without a language server.

/** The three calls this module needs. `ipc` satisfies it; tests pass a recorder. */
export interface LspTransport {
  request<T = unknown>(method: string, params: unknown): Promise<T>;
  notify(method: string, params: unknown): Promise<void> | void;
}

/** Absolute path → `file://` URI. Mirrors `bridge::path_to_uri` in Rust, and for the same
 * reason: a Windows path is `C:\Users\…`, which needs forward slashes and a third slash before
 * the drive letter, or the server reads `C:` as a host name. */
export function pathToUri(absolutePath: string): string {
  const text = absolutePath.replace(/\\/g, '/');
  return text.startsWith('/') ? `file://${text}` : `file:///${text}`;
}

/** `file://` URI → absolute path, for turning a server's answer back into something the tree
 * understands. Undoes `pathToUri`, including the percent-encoding servers sometimes add. */
export function uriToPath(uri: string): string {
  const withoutScheme = decodeURIComponent(uri.replace(/^file:\/\//, ''));
  // `/C:/x` came from a Windows path; `/home/x` did not.
  return /^\/[A-Za-z]:/.test(withoutScheme) ? withoutScheme.slice(1) : withoutScheme;
}

/** Everything TexLab needs to know about one file we have open. */
interface SyncedDocument {
  uri: string;
  version: number;
  text: string;
  languageId: string;
}

/** `.tex` and `.bib` are the two the server treats differently; anything else we do not send. */
export function languageIdFor(path: string): string | null {
  if (path.endsWith('.tex')) return 'latex';
  if (path.endsWith('.bib')) return 'bibtex';
  return null;
}

export class LspClient {
  /** Keyed by URI, because that is what both sides of the protocol use. */
  private readonly open = new Map<string, SyncedDocument>();

  constructor(private readonly transport: LspTransport) {}

  /** URIs the server currently believes are open. */
  openUris(): string[] {
    return [...this.open.keys()];
  }

  versionOf(uri: string): number | undefined {
    return this.open.get(uri)?.version;
  }

  /** Tell the server a file is open. A second call for the same file is a no-op rather than a
   * duplicate `didOpen`, which servers are entitled to treat as a protocol error. */
  async didOpen(absolutePath: string, text: string): Promise<void> {
    const languageId = languageIdFor(absolutePath);
    if (!languageId) return;
    const uri = pathToUri(absolutePath);
    if (this.open.has(uri)) return;

    this.open.set(uri, { uri, version: 1, text, languageId });
    await this.transport.notify('textDocument/didOpen', {
      textDocument: { uri, languageId, version: 1, text },
    });
  }

  /** Send the new text of a file. Full-text sync, not incremental: TexLab accepts it, and the
   * alternative is mirroring CodeMirror's ranges into LSP's, which is a second source of truth
   * for no measured gain. Revisit if a large file ever shows up slow. */
  async didChange(absolutePath: string, text: string): Promise<void> {
    const uri = pathToUri(absolutePath);
    const document = this.open.get(uri);
    if (!document) return; // never opened, or already closed — nothing to update
    if (document.text === text) return; // a save with no edits is not a change

    document.version += 1;
    document.text = text;
    await this.transport.notify('textDocument/didChange', {
      textDocument: { uri, version: document.version },
      contentChanges: [{ text }],
    });
  }

  async didSave(absolutePath: string): Promise<void> {
    const uri = pathToUri(absolutePath);
    if (!this.open.has(uri)) return;
    await this.transport.notify('textDocument/didSave', { textDocument: { uri } });
  }

  async didClose(absolutePath: string): Promise<void> {
    const uri = pathToUri(absolutePath);
    if (!this.open.delete(uri)) return;
    await this.transport.notify('textDocument/didClose', { textDocument: { uri } });
  }

  /** Re-open everything after the server restarted.
   *
   * The new process knows nothing, so every document goes back at version 1 — continuing from
   * the old numbers against a server that has never seen them is the bug this method exists to
   * prevent. Kept as one method so the restart path has one obvious call site. */
  async resync(): Promise<void> {
    const documents = [...this.open.values()];
    this.open.clear();
    for (const document of documents) {
      this.open.set(document.uri, { ...document, version: 1 });
      await this.transport.notify('textDocument/didOpen', {
        textDocument: {
          uri: document.uri,
          languageId: document.languageId,
          version: 1,
          text: document.text,
        },
      });
    }
  }

  /** Forget every document without telling the server — for when the session itself is gone. */
  reset(): void {
    this.open.clear();
  }

  /** Completions at a position. `line` and `character` are zero-based, as LSP counts them;
   * CodeMirror counts lines from one, so the caller converts. */
  completion(absolutePath: string, line: number, character: number): Promise<unknown> {
    return this.transport.request('textDocument/completion', {
      textDocument: { uri: pathToUri(absolutePath) },
      position: { line, character },
    });
  }

  hover(absolutePath: string, line: number, character: number): Promise<unknown> {
    return this.transport.request('textDocument/hover', {
      textDocument: { uri: pathToUri(absolutePath) },
      position: { line, character },
    });
  }

  definition(absolutePath: string, line: number, character: number): Promise<unknown> {
    return this.transport.request('textDocument/definition', {
      textDocument: { uri: pathToUri(absolutePath) },
      position: { line, character },
    });
  }

  documentSymbols(absolutePath: string): Promise<unknown> {
    return this.transport.request('textDocument/documentSymbol', {
      textDocument: { uri: pathToUri(absolutePath) },
    });
  }
}
