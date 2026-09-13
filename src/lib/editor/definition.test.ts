import { EditorState, Text } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import { definitionClickHandler, definitionCommand, firstLocation } from './definition';
import type { DefinitionRequester } from './definition';

/** `definitionCommand` reads `view.state.doc` and `view.state.selection.main.head`; a full
 * `EditorState` (not a mounted `EditorView`, which needs `document` — see `hover.test.ts`) gives
 * it both for real, so the offset math is exercised rather than faked. */
function stateWithCursor(doc: string, cursor: number): EditorView {
  const state = EditorState.create({ doc, selection: { anchor: cursor } });
  return { state } as unknown as EditorView;
}

/** `definitionClickHandler` reads only `posAtCoords` and `state.doc` off its second argument. */
function fakeView(doc: string, offsetAtClick: number | null): EditorView {
  return {
    state: { doc: Text.of(doc.split('\n')) },
    posAtCoords: () => offsetAtClick,
  } as unknown as EditorView;
}

/** A structural stand-in for the one `MouseEvent` shape `definitionClickHandler` reads —
 * `ctrlKey`/`metaKey`/`clientX`/`clientY` — since this Node-only Vitest environment has no
 * `MouseEvent` constructor (see `vite.config.ts`'s `environment: 'node'`). */
function click(modifiers: { ctrlKey?: boolean; metaKey?: boolean }): MouseEvent {
  return { ctrlKey: false, metaKey: false, clientX: 1, clientY: 1, ...modifiers } as MouseEvent;
}

describe('firstLocation', () => {
  const location = { uri: 'file:///a.tex', range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } } };

  it('passes a single Location through unchanged', () => {
    expect(firstLocation(location)).toBe(location);
  });

  it('takes the first of several locations', () => {
    const second = { ...location, uri: 'file:///b.tex' };
    expect(firstLocation([location, second])).toBe(location);
  });

  it('returns null for an empty array', () => {
    expect(firstLocation([])).toBeNull();
  });

  it('returns null for a null answer (valid LSP for "no definition")', () => {
    expect(firstLocation(null)).toBeNull();
  });
});

describe('definitionCommand', () => {
  it('asks at the cursor position, converted to a zero-based LSP position', () => {
    const request: DefinitionRequester = vi.fn(async () => true);
    const view = stateWithCursor('\\ref{fig:one}\n', 5);
    const command = definitionCommand(request);

    const handled = command(view);

    expect(handled).toBe(true);
    expect(request).toHaveBeenCalledWith(0, 5);
  });

  it('reports itself as handled even when nothing comes back, so no other binding also fires', () => {
    const request: DefinitionRequester = async () => false;
    const view = stateWithCursor('x', 0);
    expect(definitionCommand(request)(view)).toBe(true);
  });
});

describe('definitionClickHandler', () => {
  it('ignores a plain click with no modifier key', () => {
    const request: DefinitionRequester = vi.fn(async () => true);
    const handler = definitionClickHandler(request);

    expect(handler(click({}), fakeView('\\ref{fig:one}\n', 5))).toBe(false);
    expect(request).not.toHaveBeenCalled();
  });

  it('handles a Ctrl-click by requesting the definition at the clicked offset', () => {
    const request: DefinitionRequester = vi.fn(async () => true);
    const handler = definitionClickHandler(request);

    expect(handler(click({ ctrlKey: true }), fakeView('\\ref{fig:one}\n', 5))).toBe(true);
    expect(request).toHaveBeenCalledWith(0, 5);
  });

  it('treats a Cmd-click (metaKey) the same as Ctrl-click, for macOS', () => {
    const request: DefinitionRequester = vi.fn(async () => true);
    const handler = definitionClickHandler(request);

    expect(handler(click({ metaKey: true }), fakeView('\\ref{fig:one}\n', 5))).toBe(true);
  });

  it('leaves a Ctrl-click outside any character unhandled', () => {
    const request: DefinitionRequester = vi.fn(async () => true);
    const handler = definitionClickHandler(request);

    expect(handler(click({ ctrlKey: true }), fakeView('\\ref{fig:one}\n', null))).toBe(false);
    expect(request).not.toHaveBeenCalled();
  });
});
