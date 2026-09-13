import { Text } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';
import { hoverSource } from './hover';
import type { HoverRequester } from './hover';

/** `hoverSource` only ever reads `view.state.doc` — the same reason `completion.test.ts` builds
 * a bare `CompletionContext` over an `EditorState` rather than a mounted editor. This repo's
 * Vitest environment is plain Node (no DOM; see `vite.config.ts`), so a real `EditorView` cannot
 * be constructed here at all — it needs `document` to create its own DOM. A structural stand-in
 * for the one path this function actually reads is enough, and matches `HoverTooltipSource`'s
 * real parameter type closely enough that a signature change here would still be caught by the
 * compiler. */
function viewWithDoc(doc: string): EditorView {
  return { state: { doc: Text.of(doc.split('\n')) } } as unknown as EditorView;
}

describe('hoverSource', () => {
  it('renders plain-text contents as the tooltip body', async () => {
    const request: HoverRequester = async () => ({ contents: 'undefined command' });
    const tooltip = await hoverSource(viewWithDoc('\\fooo\n'), 2, request);

    expect(tooltip).not.toBeNull();
    expect(tooltip!.pos).toBe(2);
    // `create` builds a real DOM node via `document.createElement`, which this Node-only test
    // environment does not have (see `viewWithDoc`'s comment) — the text it would show is
    // asserted through `markupToPlainText` itself in `lsp-protocol.test.ts` instead; this test
    // only needs to know a tooltip was produced and where it sits.
    expect(typeof tooltip!.create).toBe('function');
  });

  it('resolves to null on an empty answer, not an error tooltip', async () => {
    const request: HoverRequester = async () => null;
    expect(await hoverSource(viewWithDoc('x'), 0, request)).toBeNull();
  });

  it('resolves to null for a payload with no contents field at all (fails isHover)', async () => {
    const request: HoverRequester = async () => ({} as never);
    expect(await hoverSource(viewWithDoc('x'), 0, request)).toBeNull();
  });

  it('resolves to null when contents is empty text rather than showing a blank tooltip', async () => {
    const request: HoverRequester = async () => ({ contents: '   ' });
    expect(await hoverSource(viewWithDoc('x'), 0, request)).toBeNull();
  });

  it('uses the range the server sent instead of a single point at the request offset', async () => {
    const request: HoverRequester = async () => ({
      contents: 'x',
      range: { start: { line: 0, character: 1 }, end: { line: 0, character: 4 } },
    });
    const tooltip = await hoverSource(viewWithDoc('\\alpha\n'), 2, request);

    expect(tooltip!.pos).toBe(1);
    expect(tooltip!.end).toBe(4);
  });

  it('falls back to a single point at the request offset when the server sends no range', async () => {
    const request: HoverRequester = async () => ({ contents: 'x' });
    const tooltip = await hoverSource(viewWithDoc('\\alpha\n'), 3, request);

    expect(tooltip!.pos).toBe(3);
    expect(tooltip!.end).toBe(3);
  });

  it('degrades to null when the request rejects (server crashed mid-flight)', async () => {
    const request: HoverRequester = async () => {
      throw new Error('disconnected channel');
    };
    await expect(hoverSource(viewWithDoc('x'), 0, request)).resolves.toBeNull();
  });
});
