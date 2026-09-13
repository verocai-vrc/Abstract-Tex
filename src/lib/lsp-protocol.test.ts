import { describe, expect, it } from 'vitest';
import {
  completionItemsOf,
  isCompletionList,
  isPublishDiagnosticsParams,
  markupToPlainText,
} from './lsp-protocol';

describe('isCompletionList', () => {
  it('accepts the {items: [...]} shape', () => {
    expect(isCompletionList({ isIncomplete: false, items: [] })).toBe(true);
  });

  it('rejects a bare array, which is the other valid shape but not this one', () => {
    expect(isCompletionList([{ label: 'itemize' }])).toBe(false);
  });

  it('rejects null and primitives', () => {
    expect(isCompletionList(null)).toBe(false);
    expect(isCompletionList(undefined)).toBe(false);
    expect(isCompletionList('itemize')).toBe(false);
    expect(isCompletionList(42)).toBe(false);
  });
});

describe('completionItemsOf', () => {
  it('unwraps a CompletionList', () => {
    const items = [{ label: 'itemize' }];
    expect(completionItemsOf({ isIncomplete: false, items })).toBe(items);
  });

  it('passes a bare array through unchanged', () => {
    const items = [{ label: 'enumerate' }];
    expect(completionItemsOf(items)).toBe(items);
  });

  it('returns null for a bare null answer (valid LSP for "no completions")', () => {
    expect(completionItemsOf(null)).toBeNull();
  });

  it('returns null for anything that is neither shape, rather than guessing', () => {
    expect(completionItemsOf({ unexpected: true })).toBeNull();
    expect(completionItemsOf('itemize')).toBeNull();
    expect(completionItemsOf(42)).toBeNull();
  });
});

describe('markupToPlainText', () => {
  it('passes a plain string through', () => {
    expect(markupToPlainText('just text')).toBe('just text');
  });

  it('reads MarkupContent.value', () => {
    expect(markupToPlainText({ kind: 'plaintext', value: 'hello' })).toBe('hello');
  });

  it('strips a fenced code block\'s markers but keeps the code', () => {
    expect(markupToPlainText({ kind: 'markdown', value: '```latex\n\\alpha\n```' })).toBe('\\alpha');
  });

  it('strips inline code backticks', () => {
    expect(markupToPlainText({ kind: 'markdown', value: 'the `\\alpha` command' })).toBe('the \\alpha command');
  });

  it('strips bold and italic markers', () => {
    expect(markupToPlainText({ kind: 'markdown', value: '**bold** and *italic*' })).toBe('bold and italic');
  });

  it('trims surrounding whitespace left behind by stripped fences', () => {
    expect(markupToPlainText({ kind: 'markdown', value: '```\ntext\n```\n' })).toBe('text');
  });
});

describe('isPublishDiagnosticsParams', () => {
  it('accepts a well-formed payload, including an empty diagnostics array', () => {
    expect(isPublishDiagnosticsParams({ uri: 'file:///proj/main.tex', diagnostics: [] })).toBe(true);
    expect(
      isPublishDiagnosticsParams({
        uri: 'file:///proj/main.tex',
        diagnostics: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, message: 'x' }],
      }),
    ).toBe(true);
  });

  /** The failure this guard exists for: without it a missing `uri` keys the store under
   * `undefined`, where no lookup can ever find it and no publish can ever clear it. */
  it('rejects a payload with no uri', () => {
    expect(isPublishDiagnosticsParams({ diagnostics: [] })).toBe(false);
    expect(isPublishDiagnosticsParams({ uri: 42, diagnostics: [] })).toBe(false);
  });

  it('rejects a payload whose diagnostics is not an array', () => {
    expect(isPublishDiagnosticsParams({ uri: 'file:///x.tex' })).toBe(false);
    expect(isPublishDiagnosticsParams({ uri: 'file:///x.tex', diagnostics: null })).toBe(false);
    expect(isPublishDiagnosticsParams({ uri: 'file:///x.tex', diagnostics: { 0: 'x' } })).toBe(false);
  });

  it('rejects null, primitives and arrays without throwing', () => {
    expect(isPublishDiagnosticsParams(null)).toBe(false);
    expect(isPublishDiagnosticsParams(undefined)).toBe(false);
    expect(isPublishDiagnosticsParams('file:///x.tex')).toBe(false);
    expect(isPublishDiagnosticsParams([])).toBe(false);
  });
});
