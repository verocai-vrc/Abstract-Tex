// Turning `textDocument/documentSymbol` into a flat list, for whatever first wants one.
//
// Nothing renders a symbol tree yet — S4.2's Document map panel is where sections, figures and
// labels get a UI. What this loop owes is the plumbing: a typed request already exists on
// `LspClient` (S3.3a), and this file is the one place that decides how the tree TexLab returns
// gets read, so S4.2 does not have to invent that decision under UI pressure. Deliberately no
// CodeMirror or Svelte import — this is plain data in, plain data out, like `lsp-diagnostics.ts`.

import type { DocumentSymbol } from '../lsp-protocol';

/** One symbol, flattened out of whatever nesting the server sent, with its depth kept so a
 * future tree view can still indent without re-walking the original structure. */
export interface FlatSymbol {
  name: string;
  detail: string | null;
  kind: number;
  /** 1-based, the way CodeMirror and the rest of the editor layer count lines. */
  line: number;
  /** How deeply nested this symbol was — 0 for a top-level section, 1 for a child of one, and
   * so on. `DocumentSymbol.children` is exactly this nesting; a flat list needs its own way to
   * say "this belonged under that one." */
  depth: number;
}

/** Depth-first flatten of a `documentSymbol` response, in the order the server listed them. A
 * response of `null` (no symbols, or the server does not support the request) flattens to an
 * empty list rather than throwing — the same "missing capability is not an error" rule every
 * other LSP call in this app already follows. */
export function flattenSymbols(symbols: DocumentSymbol[] | null): FlatSymbol[] {
  if (!symbols) return [];
  const flat: FlatSymbol[] = [];
  const visit = (nodes: DocumentSymbol[], depth: number) => {
    for (const node of nodes) {
      flat.push({
        name: node.name,
        detail: node.detail ?? null,
        kind: node.kind,
        line: node.selectionRange.start.line + 1,
        depth,
      });
      if (node.children && node.children.length > 0) visit(node.children, depth + 1);
    }
  };
  visit(symbols, 0);
  return flat;
}
