import { describe, expect, it } from 'vitest';
import { flattenSymbols } from './symbols';
import type { DocumentSymbol } from '../lsp-protocol';

function symbol(name: string, line: number, children?: DocumentSymbol[]): DocumentSymbol {
  const position = { line, character: 0 };
  return {
    name,
    kind: 1,
    range: { start: position, end: position },
    selectionRange: { start: position, end: position },
    ...(children ? { children } : {}),
  };
}

describe('flattenSymbols', () => {
  it('returns an empty list for null (no symbols, or the server does not support the request)', () => {
    expect(flattenSymbols(null)).toEqual([]);
  });

  it('returns an empty list for an empty array', () => {
    expect(flattenSymbols([])).toEqual([]);
  });

  it('flattens a single top-level symbol, converting to a 1-based line', () => {
    const result = flattenSymbols([symbol('section', 4)]);
    expect(result).toEqual([{ name: 'section', detail: null, kind: 1, line: 5, depth: 0 }]);
  });

  it('keeps depth-first order and records each child\'s depth', () => {
    const tree = [
      symbol('chapter', 0, [symbol('section', 1, [symbol('subsection', 2)]), symbol('section2', 5)]),
    ];
    const result = flattenSymbols(tree);

    expect(result.map((s) => [s.name, s.depth])).toEqual([
      ['chapter', 0],
      ['section', 1],
      ['subsection', 2],
      ['section2', 1],
    ]);
  });

  it('passes detail through when present, and null when absent', () => {
    const withDetail: DocumentSymbol = { ...symbol('fig', 0), detail: 'Figure 1' };
    expect(flattenSymbols([withDetail])[0]!.detail).toBe('Figure 1');
    expect(flattenSymbols([symbol('fig', 0)])[0]!.detail).toBeNull();
  });

  it('uses selectionRange, not range, for the reported line', () => {
    const node: DocumentSymbol = {
      name: 'sec',
      kind: 1,
      range: { start: { line: 0, character: 0 }, end: { line: 10, character: 0 } },
      selectionRange: { start: { line: 3, character: 0 }, end: { line: 3, character: 5 } },
    };
    expect(flattenSymbols([node])[0]!.line).toBe(4);
  });

  it('treats an empty children array the same as no children at all', () => {
    const node: DocumentSymbol = { ...symbol('sec', 0), children: [] };
    expect(flattenSymbols([node])).toHaveLength(1);
  });
});
