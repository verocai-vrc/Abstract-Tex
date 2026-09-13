// Turning the cursor position into a SyncTeX forward-search request (S3.4).
//
// Same seam as `definition.ts`: this file knows the cursor's line and nothing else — it does not
// know how to reach the Rust side or how the PDF pane shows the answer. `controller.svelte.ts`'s
// `syncTexForward` does the actual IPC call and updates `app.syncTexScrollRequest`.

import type { Command, KeyBinding } from '@codemirror/view';

/** What `synctex.ts` needs from the rest of the app: given a 1-based line, ask for its spot in
 * the PDF and act on the answer. Matches `DefinitionRequester`'s shape (`definition.ts`) for the
 * same reason — the jump target (a scroll in a different pane) is not something this module can
 * reach, so it stops at "ask", the same way go-to-definition stops at "ask" for a tab it cannot
 * open itself. */
export type ForwardSearchRequester = (line: number) => void;

/** `Ctrl-Alt-J` ("jump"), chosen to sit next to `Alt-F12` (go-to-definition) without colliding
 * with any binding already in `setup.ts`'s keymap or `shortcuts.ts`'s window-level table. */
export function forwardSearchCommand(request: ForwardSearchRequester): Command {
  return (view): boolean => {
    const line = view.state.doc.lineAt(view.state.selection.main.head).number;
    request(line);
    return true;
  };
}

export function forwardSearchKeymap(request: ForwardSearchRequester): KeyBinding[] {
  return [{ key: 'Ctrl-Alt-j', run: forwardSearchCommand(request) }];
}
