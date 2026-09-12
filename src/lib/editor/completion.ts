// Turning `textDocument/completion` into a CodeMirror `CompletionSource`.
//
// This is the seam DESIGN.md §4.1 draws: Rust and `lsp.ts` know nothing about CodeMirror, and
// this file knows nothing about JSON-RPC. It only calls the typed `LspClient.completion` and
// maps the answer onto `@codemirror/autocomplete`'s types.

import type { CompletionContext, CompletionResult, Completion } from '@codemirror/autocomplete';
import {
  CompletionItemKind,
  completionItemsOf,
  type CompletionItem,
  type CompletionList,
} from '../lsp-protocol';
import { offsetToPosition, positionToOffset } from './positions';

/** What `completion.ts` needs from the rest of the app: one function that asks for completions
 * at a position and resolves to whatever the server said (or `null`). `controller.svelte.ts`'s
 * `lspCompletion` satisfies this; this file never imports `LspClient` or `ipc` directly, keeping
 * the DESIGN.md §4.1 boundary — protocol lives in `lsp.ts`, CodeMirror adaptation lives here. */
export type CompletionRequester = (
  line: number,
  character: number,
) => Promise<CompletionItem[] | CompletionList | null>;

/** LSP's `CompletionItemKind` number → the type string CodeMirror uses to pick an icon
 * (`cm-completionIcon-<type>`, see the `Completion.type` doc comment in the autocomplete
 * package). Only the kinds TexLab's environment/command completions actually send are worth
 * naming; anything else falls through to `'text'`, CodeMirror's plainest icon. */
const KIND_TO_TYPE: Partial<Record<CompletionItemKind, string>> = {
  [CompletionItemKind.Text]: 'text',
  [CompletionItemKind.Method]: 'method',
  [CompletionItemKind.Function]: 'function',
  [CompletionItemKind.Constructor]: 'function',
  [CompletionItemKind.Field]: 'property',
  [CompletionItemKind.Variable]: 'variable',
  [CompletionItemKind.Class]: 'class',
  [CompletionItemKind.Interface]: 'interface',
  [CompletionItemKind.Module]: 'namespace',
  [CompletionItemKind.Property]: 'property',
  [CompletionItemKind.Unit]: 'unit',
  [CompletionItemKind.Value]: 'constant',
  [CompletionItemKind.Enum]: 'enum',
  [CompletionItemKind.Keyword]: 'keyword',
  [CompletionItemKind.Snippet]: 'text',
  [CompletionItemKind.Color]: 'constant',
  [CompletionItemKind.File]: 'text',
  [CompletionItemKind.Reference]: 'variable',
  [CompletionItemKind.Folder]: 'text',
  [CompletionItemKind.EnumMember]: 'enum',
  [CompletionItemKind.Constant]: 'constant',
  [CompletionItemKind.Struct]: 'class',
  [CompletionItemKind.Event]: 'variable',
  [CompletionItemKind.Operator]: 'keyword',
  [CompletionItemKind.TypeParameter]: 'type',
};

function typeOf(item: CompletionItem): string {
  if (item.kind === undefined) return 'text';
  return KIND_TO_TYPE[item.kind] ?? 'text';
}

/** One `CompletionItem` → the `Completion` CodeMirror renders and inserts.
 *
 * No `apply` function here: an earlier version resolved `textEdit.range` to offsets inside
 * `apply`, but `apply` fires whenever the option is eventually picked — possibly after more
 * typing has shifted the document — and CodeMirror only remaps position-dependent `apply`
 * functions through a `CompletionResult.map` this file did not provide. Typing `\begin{it`, then
 * `em` before pressing Enter, replayed the *stale* range and produced `\begin{itemizeem}`. Using
 * plain `label`/`insertText` as the `Completion.apply` string instead means CodeMirror inserts it
 * at its own `from`/`to` (from `CompletionResult`, below), which CodeMirror itself keeps mapped
 * across intervening edits — the problem disappears because the position tracking is no longer
 * this file's job. The one piece of `textEdit` this still needs is `newText`, when the server's
 * literal insertion text differs from the label it shows (TexLab's environment completions do
 * not differ, but nothing here assumes that stays true). */
function toCompletion(item: CompletionItem): Completion {
  return {
    label: item.label,
    detail: item.detail,
    type: typeOf(item),
    apply: item.textEdit?.newText ?? item.insertText ?? item.label,
  };
}

/** The single range every option in one response replaces, as `CompletionResult.from`/`to`.
 * LSP does not require every item in a list to share a range, but TexLab's completions always do
 * — they are all completing the same token under the cursor — so the first item stands in for
 * the list; a response with no `textEdit` at all falls back to `context.pos` for both ends
 * (insert at the cursor, replace nothing), matching what plain `completeFromList` would do. */
function rangeOf(context: CompletionContext, items: CompletionItem[]): { from: number; to: number } {
  const range = items[0]?.textEdit?.range;
  if (!range) return { from: context.pos, to: context.pos };
  return {
    from: positionToOffset(context.state.doc, range.start),
    to: positionToOffset(context.state.doc, range.end),
  };
}

/**
 * Build a CodeMirror `CompletionSource` backed by `request`.
 *
 * Returns `null` synchronously whenever there is nothing to ask — no server, an aborted context
 * — because "no server" must read as "plain CodeMirror completion continues," never as an error
 * (DESIGN.md §2 commitment 6). `controller.svelte.ts`'s `lspCompletion` already returns `null`
 * for "no server" and "no project," so this source does not duplicate that check; it only adds
 * the abort handling that is specific to CodeMirror's completion lifecycle.
 */
export function lspCompletionSource(
  request: CompletionRequester,
): (context: CompletionContext) => Promise<CompletionResult | null> {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    if (context.aborted) return null;

    const position = offsetToPosition(context.state.doc, context.pos);
    let response;
    try {
      response = await request(position.line, position.character);
    } catch {
      // A dead or confused server degrades to no completions, never a toast — commitment 6
      // again, and the same reasoning `controller.svelte.ts`'s `tellServer` already applies to
      // every other LSP call.
      return null;
    }
    if (context.aborted) return null; // the keystroke that started this is already stale

    const items = completionItemsOf(response);
    if (!items || items.length === 0) return null;

    return {
      ...rangeOf(context, items),
      options: items.map(toCompletion),
      // Without this, CodeMirror re-queries the server on every single keystroke while the
      // popup is open — each one a full IPC round trip through `lspCompletion`, which itself
      // awaits a `didChange` first (DESIGN.md §2 commitment 2: never block a keystroke on a
      // round trip). `validFor` tells CodeMirror the existing option list is still good as long
      // as the replaced text keeps looking like a bare LaTeX identifier, so typing "itemize"
      // after the popup opens filters the list it already has instead of asking again for every
      // letter. `*` is included because environment names like `verbatim*` use it.
      validFor: /^[A-Za-z*]*$/,
    };
  };
}
