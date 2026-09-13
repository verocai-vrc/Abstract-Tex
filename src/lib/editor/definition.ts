// Turning `textDocument/definition` into a CodeMirror command.
//
// Same seam as `completion.ts` and `hover.ts`: this file knows nothing about JSON-RPC, and
// nothing about tabs or the document tree either — the LSP spec answers in URIs, and only the
// controller knows how to turn a URI into an open tab (S2.3's `DocumentManager`). So this module
// stops at *deciding where the answer points*; `controller.svelte.ts`'s `lspGoToDefinition` is
// what actually opens a file or moves the cursor.

import type { Command, EditorView, KeyBinding } from '@codemirror/view';
import { offsetToPosition } from './positions';
import type { Location } from '../lsp-protocol';

/** What `definition.ts` needs from the rest of the app: ask for the definition at a position in
 * the *current* file, and be told what happened. Unlike `CompletionRequester`/`HoverRequester`,
 * this one does the whole job — including jumping — because "jump" can mean "open a different
 * tab," which only the controller can do; a `Promise<Location | null>` here would leave this
 * file needing to open tabs itself, crossing the boundary DESIGN.md §4.1 draws. */
export type DefinitionRequester = (line: number, character: number) => Promise<boolean>;

/** Build the go-to-definition command, for both the keybinding below and a click handler.
 *
 * A plain `Command` (`(view) => boolean`, CodeMirror's shape for anything bound in a `keymap`).
 * It reads
 * the cursor position rather than taking one as an argument, matching how CodeMirror's own
 * built-in commands (`cursorDown`, etc.) are shaped. */
export function definitionCommand(request: DefinitionRequester): Command {
  return (view): boolean => {
    const position = offsetToPosition(view.state.doc, view.state.selection.main.head);
    void request(position.line, position.character);
    // Always "handled": even when the server has nothing to say, this is the LaTeX author's own
    // go-to-definition action and no other keymap entry should also fire for the same chord.
    return true;
  };
}

/** `Alt-F12`, matching the chord most editors already use for peek/go-to-definition; `F12` alone
 * is taken by devtools in the webview and `Ctrl-click` (below) is the mouse path DESIGN.md's
 * "click the PDF and land on the right line" exit demo expects an equivalent for in the editor. */
export function definitionKeymap(request: DefinitionRequester): KeyBinding[] {
  return [{ key: 'Alt-F12', run: definitionCommand(request) }];
}

/** `Ctrl`-click (`Cmd`-click on macOS) anywhere in the editor jumps to that position's
 * definition, mirroring the "click the PDF" gesture from the sprint 3 exit demo. Returning
 * `true` from a `mousedown` handler tells CodeMirror the event is handled, which stops it from
 * also placing the cursor there via a plain click — go-to-definition replaces the click rather
 * than happening alongside it. */
export function definitionClickHandler(request: DefinitionRequester) {
  return (event: MouseEvent, view: EditorView): boolean => {
    if (!(event.ctrlKey || event.metaKey)) return false;
    const offset = view.posAtCoords({ x: event.clientX, y: event.clientY });
    if (offset === null) return false;
    const position = offsetToPosition(view.state.doc, offset);
    void request(position.line, position.character);
    return true;
  };
}

/** The first location in whichever of LSP's three legal shapes the server answered with — one
 * `Location`, several (a symbol defined more than once), or nothing. Several is resolved to "the
 * first" rather than a picker: `DESIGN.md` §1.3 rules out building UI infrastructure ahead of a
 * proven need, and TexLab's own definitions (labels, `\newcommand`) are effectively always
 * single-valued today. Exported so the controller and its test do not have to re-derive the
 * union-narrowing. */
export function firstLocation(answer: Location | Location[] | null): Location | null {
  if (answer === null) return null;
  return Array.isArray(answer) ? (answer[0] ?? null) : answer;
}
