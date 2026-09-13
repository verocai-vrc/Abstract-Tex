import { EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import { forwardSearchCommand } from './synctex';

/** Same stand-in `definition.test.ts` uses: `forwardSearchCommand` only reads `view.state.doc`
 * and `view.state.selection.main.head`, so a full `EditorState` gives it both for real without
 * needing a mounted `EditorView` (which needs `document` — see `hover.test.ts`'s own note). */
function stateWithCursor(doc: string, cursor: number): EditorView {
  const state = EditorState.create({ doc, selection: { anchor: cursor } });
  return { state } as unknown as EditorView;
}

describe('forwardSearchCommand', () => {
  it('asks for the 1-based line the cursor is on', () => {
    const request = vi.fn();
    const view = stateWithCursor('one\ntwo\nthree', 5); // inside "two", line 2
    const handled = forwardSearchCommand(request)(view);
    expect(handled).toBe(true);
    expect(request).toHaveBeenCalledWith(2);
  });

  it('reports line 1 for a cursor at the very start', () => {
    const request = vi.fn();
    const view = stateWithCursor('one\ntwo', 0);
    forwardSearchCommand(request)(view);
    expect(request).toHaveBeenCalledWith(1);
  });

  it('is always "handled", even though it only asks and does not act itself', () => {
    const view = stateWithCursor('x', 0);
    expect(forwardSearchCommand(() => {})(view)).toBe(true);
  });
});
