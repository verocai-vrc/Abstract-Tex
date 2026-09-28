import { autocompletion, CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it, vi } from 'vitest';
import type { CompletionRequester } from './completion';
import { lspCompletionSource } from './completion';

/**
 * A real `textDocument/completion` reply from TexLab 5.26.0, captured by temporarily printing
 * the response inside `crates/abstract-tex-lsp/tests/bridge.rs`'s
 * `the_real_texlab_completes_an_environment_name` (S3.3a card, rung 3) rather than spawning a
 * server from Vitest. The request was completion inside `\begin{` on
 * `\documentclass{article}\n\begin{document}\n\begin{}\n\end{document}\n`, line 2 character 7 —
 * the same document and position the Rust test builds. Trimmed to the fields this file reads
 * plus a representative few items; the full reply lists 39 environments and this keeps the
 * fixture readable while still exercising both the array-vs-list ambiguity (this is the `{items:
 * [...]}` shape) and a real `textEdit` range.
 */
const REAL_TEXLAB_COMPLETION_LIST = {
  isIncomplete: false,
  items: [
    {
      detail: 'built-in',
      kind: 1,
      label: 'abstract',
      preselect: false,
      sortText: '00',
      textEdit: { newText: 'abstract', range: { end: { character: 7, line: 2 }, start: { character: 7, line: 2 } } },
    },
    {
      detail: 'built-in',
      kind: 1,
      label: 'document',
      preselect: false,
      sortText: '06',
      textEdit: { newText: 'document', range: { end: { character: 7, line: 2 }, start: { character: 7, line: 2 } } },
    },
    {
      detail: 'built-in',
      kind: 1,
      label: 'itemize',
      preselect: false,
      sortText: '17',
      textEdit: { newText: 'itemize', range: { end: { character: 7, line: 2 }, start: { character: 7, line: 2 } } },
    },
  ],
};

/** A minimal `CompletionContext` over a one-line-per-string document, positioned at `pos`.
 * `autocompletion()` is included in the extensions only because `CompletionContext` expects a
 * state that could run one; nothing here triggers it. */
function contextAt(doc: string, pos: number, explicit = true): CompletionContext {
  const state = EditorState.create({ doc, extensions: [autocompletion()] });
  return new CompletionContext(state, pos, explicit);
}

describe('lspCompletionSource', () => {
  it('maps a real TexLab CompletionList onto CodeMirror options with a plain-string apply', async () => {
    const request: CompletionRequester = vi.fn(async () => REAL_TEXLAB_COMPLETION_LIST);
    const source = lspCompletionSource(request);

    // "\begin{document}\n\begin{}\n" — the cursor lands just after the second "\begin{".
    const doc = '\\begin{document}\n\\begin{}\n';
    const cursorOffset = doc.indexOf('\\begin{}') + '\\begin{'.length;
    const result = await source(contextAt(doc, cursorOffset));

    expect(result).not.toBeNull();
    const labels = result!.options.map((option) => option.label);
    expect(labels).toEqual(['abstract', 'document', 'itemize']);
    // A string `apply`, not a function: CodeMirror keeps `from`/`to` mapped across intervening
    // edits itself, which a captured-offset function cannot (the bug the reviewer found).
    expect(result!.options.map((option) => option.apply)).toEqual(['abstract', 'document', 'itemize']);
    expect(result!.validFor).toEqual(/^[A-Za-z*]*$/);
    // Line 1 (0-based), character 7 — matches the position the Rust fixture was captured at.
    expect(request).toHaveBeenCalledWith(1, 7);
  });

  it('uses both ends of the first item\'s textEdit range for from/to, not just the start', async () => {
    const withWiderRange = {
      isIncomplete: false,
      items: [
        {
          label: 'itemize',
          kind: 1,
          textEdit: { newText: 'itemize', range: { start: { line: 1, character: 4 }, end: { line: 1, character: 7 } } },
        },
      ],
    };
    const request: CompletionRequester = async () => withWiderRange;
    const source = lspCompletionSource(request);

    const doc = '\\begin{document}\n\\begin{it}\n'; // "it" sits at characters 4-6 on line 1
    const cursorOffset = doc.indexOf('{it}') + 3; // just after "it"
    const result = await source(contextAt(doc, cursorOffset));

    expect(result).not.toBeNull();
    // Both ends come from the server's range, not from `context.pos` alone — from=4, to=7 on
    // line 1, whose line starts right after the first "\n".
    const line1Start = doc.indexOf('\n') + 1;
    expect(result!.from).toBe(line1Start + 4);
    expect(result!.to).toBe(line1Start + 7);
  });

  it('degrades to null, not an error, when the request rejects (server crashed mid-flight)', async () => {
    const request: CompletionRequester = async () => {
      throw new Error('disconnected channel');
    };
    const source = lspCompletionSource(request);

    await expect(source(contextAt('\\begin{}', 7))).resolves.toBeNull();
  });

  it('returns null once the context is already aborted, never applying a stale answer', async () => {
    const request: CompletionRequester = vi.fn(async () => REAL_TEXLAB_COMPLETION_LIST);
    const source = lspCompletionSource(request);

    const context = contextAt('\\begin{}', 7);
    // `aborted` is `true` once every abort listener has fired and been cleared — CodeMirror's own
    // signal that the keystroke this context was for is stale. Registering a listener through the
    // public `addEventListener` API, then driving the internal list to null the way CodeMirror's
    // own completion machinery does, is the only way to force this without a real editor.
    context.addEventListener('abort', () => {});
    // @ts-expect-error -- there is no public way to fire the abort; this reaches into the one
    // field the library exposes no setter for.
    context.abortListeners = null;
    expect(context.aborted).toBe(true);

    await expect(source(context)).resolves.toBeNull();
    expect(request).not.toHaveBeenCalled();
  });

  it('returns null when no completions come back at all (bare null, valid per the spec)', async () => {
    const request: CompletionRequester = async () => null;
    const source = lspCompletionSource(request);

    await expect(source(contextAt('\\begin{}', 7))).resolves.toBeNull();
  });

  it('returns null for an empty list rather than an empty-but-open popup', async () => {
    const request: CompletionRequester = async () => ({ isIncomplete: false, items: [] });
    const source = lspCompletionSource(request);

    await expect(source(contextAt('\\begin{}', 7))).resolves.toBeNull();
  });

  it('also accepts the bare-array shape TexLab sometimes sends instead of a CompletionList', async () => {
    const request: CompletionRequester = async () => REAL_TEXLAB_COMPLETION_LIST.items;
    const source = lspCompletionSource(request);

    const result = await source(contextAt('\\begin{}', 7));
    expect(result?.options.map((o) => o.label)).toEqual(['abstract', 'document', 'itemize']);
  });
});
