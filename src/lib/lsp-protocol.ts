// Narrow, hand-written LSP types for the subset `lsp.ts` and the editor layer use.
//
// Not `vscode-languageserver-types`: the full package models the whole spec, and this app reads
// maybe thirty fields of it. A dependency earns its place by saving more reading than it costs;
// here the real spec text (https://microsoft.github.io/language-server-protocol/) is a shorter
// path to the truth than a generated types package the learner-reader would have to detour into.
// Every interface below is a strict subset of the real one — extra fields TexLab sends are simply
// not modelled, which is safe because we only ever read named fields, never enumerate them.

/** Zero-based line and UTF-16-code-unit column, exactly as LSP defines `Position`. */
export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

export interface Location {
  uri: string;
  range: Range;
}

/** `MarkupContent.kind`: plain text or a CommonMark string. */
export type MarkupKind = 'plaintext' | 'markdown';

export interface MarkupContent {
  kind: MarkupKind;
  value: string;
}

/** The numeric `CompletionItemKind` values TexLab actually emits, named for the icon table in
 * `completion.ts`. LSP defines more than this (up to 25); anything not listed here falls back to
 * a generic type in the icon lookup rather than growing this enum to match a spec we do not fully
 * implement. */
export enum CompletionItemKind {
  Text = 1,
  Method = 2,
  Function = 3,
  Constructor = 4,
  Field = 5,
  Variable = 6,
  Class = 7,
  Interface = 8,
  Module = 9,
  Property = 10,
  Unit = 11,
  Value = 12,
  Enum = 13,
  Keyword = 14,
  Snippet = 15,
  Color = 16,
  File = 17,
  Reference = 18,
  Folder = 19,
  EnumMember = 20,
  Constant = 21,
  Struct = 22,
  Event = 23,
  Operator = 24,
  TypeParameter = 25,
}

/** A single-range text edit, the shape TexLab sends for every completion item in the fixture. LSP
 * also allows `InsertReplaceEdit` (separate insert/replace ranges); we do not model that variant
 * because TexLab does not send it and a shape we cannot exercise is a shape we cannot trust. */
export interface TextEdit {
  range: Range;
  newText: string;
}

export interface CompletionItem {
  label: string;
  kind?: CompletionItemKind;
  detail?: string;
  documentation?: string | MarkupContent;
  /** Present when the server wants a specific range replaced (TexLab always sends this). */
  textEdit?: TextEdit;
  /** Present instead of `textEdit` on servers that let the client pick the range — CodeMirror's
   * completion source falls back to this when `textEdit` is absent. */
  insertText?: string;
  sortText?: string;
  preselect?: boolean;
}

export interface CompletionList {
  isIncomplete: boolean;
  items: CompletionItem[];
}

export interface Hover {
  contents: MarkupContent | string;
  range?: Range;
}

/** `SymbolKind` is a much longer enum in the spec; nothing here reads the number today, so it
 * stays untyped rather than duplicating a table that has no reader yet. */
export interface DocumentSymbol {
  name: string;
  detail?: string;
  kind: number;
  range: Range;
  selectionRange: Range;
  children?: DocumentSymbol[];
}

export type LspSeverity = 1 | 2 | 3 | 4; // Error, Warning, Information, Hint

export interface PublishDiagnosticsParams {
  uri: string;
  diagnostics: Array<{
    range: Range;
    severity?: LspSeverity;
    message: string;
    source?: string;
  }>;
}

/** True when `value` has the shape of a `CompletionList` rather than a bare `CompletionItem[]`.
 * TexLab's own behaviour, noted in `bridge.rs`'s test comment: "either a bare array or
 * `{items: [...]}` depending on the server's mood" — both are valid under the spec, so the
 * client has to check rather than assume. A narrowing function instead of an `as` cast, because
 * an `as` would happily lie about a response that matches neither shape (the risk the architect
 * flagged: silent bad behaviour on a protocol mismatch, as happened with the serde rename bug). */
export function isCompletionList(value: unknown): value is CompletionList {
  return (
    typeof value === 'object' &&
    value !== null &&
    Array.isArray((value as { items?: unknown }).items)
  );
}

/** True when `value` looks like a `CompletionItem[]`, the other shape TexLab may answer with. */
export function isCompletionItemArray(value: unknown): value is CompletionItem[] {
  return Array.isArray(value);
}

/** Pull the item list out of either valid shape, or `null` for anything else (including `null`
 * itself, which LSP allows as "no completions"). Centralising this is what keeps `completion.ts`
 * free of casts. */
export function completionItemsOf(value: unknown): CompletionItem[] | null {
  if (isCompletionList(value)) return value.items;
  if (isCompletionItemArray(value)) return value;
  return null;
}

/** True when `value` has the shape of a `Hover` response. */
export function isHover(value: unknown): value is Hover {
  return typeof value === 'object' && value !== null && 'contents' in value;
}

/** Reduce a `Hover.contents`-shaped value to plain text, stripping markdown syntax only in the
 * crude sense of "good enough for a first line" — a real renderer is not this loop's job. */
export function markupToPlainText(contents: MarkupContent | string): string {
  const raw = typeof contents === 'string' ? contents : contents.value;
  return raw
    .replace(/```[a-zA-Z]*\n?/g, '')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/\*\*([^*]*)\*\*/g, '$1')
    .replace(/\*([^*]*)\*/g, '$1')
    .trim();
}
