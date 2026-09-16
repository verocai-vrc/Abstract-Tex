// CodeMirror 6 configuration for a LaTeX buffer bound to a Yjs document.
//
// Deliberately not `basicSetup`: that bundle includes CodeMirror's own history, which would
// fight the Yjs undo manager. Everything else from it is listed explicitly below so a reader
// can see what the editor is made of.

import { autocompletion, closeBrackets, closeBracketsKeymap, type CompletionSource } from '@codemirror/autocomplete';
import { defaultKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, HighlightStyle, indentOnInput, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import { stex } from '@codemirror/legacy-modes/mode/stex';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { Compartment, EditorState } from '@codemirror/state';
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';
// KaTeX's stylesheet (fonts, spacing classes) for the maths popover. A bare `import` of a CSS
// file is Vite's way of adding a stylesheet to the bundle; nothing is bound to a name.
import 'katex/dist/katex.min.css';
import { yCollab } from 'y-codemirror.next';
import type { OpenDocument } from '../document';
import { definitionClickHandler, definitionKeymap, type DefinitionRequester } from './definition';
import { diagnosticGutter } from './diagnostics';
import { focusModeExtension } from './focus';
import { lspHoverSource, type HoverRequester } from './hover';
import { mathPreview } from './math-preview';
import { forwardSearchKeymap, type ForwardSearchRequester } from './synctex';
import { typewriterModeExtension } from './typewriter';

// `Compartment` is CodeMirror's slot for an extension that needs to change after the editor is
// built, without tearing the whole `EditorView` down: `compartment.reconfigure(newExtension)` is
// a `StateEffect` you dispatch like any other transaction effect. Focus and typewriter mode
// (S4.5) are each held in one of these, module-level so every tab's view shares the same slot
// identity, which is what lets `setFocusMode`/`setTypewriterMode` below address any of them.
const focusModeCompartment = new Compartment();
const typewriterModeCompartment = new Compartment();

// Colours come from the CSS custom properties in app.css so light and dark both work.
const latexHighlight = HighlightStyle.define([
  { tag: tags.tagName, color: 'var(--syn-command)' },
  { tag: tags.keyword, color: 'var(--syn-keyword)', fontWeight: '600' },
  { tag: tags.atom, color: 'var(--syn-math)' },
  { tag: tags.comment, color: 'var(--syn-comment)', fontStyle: 'italic' },
  { tag: tags.bracket, color: 'var(--syn-bracket)' },
  { tag: tags.number, color: 'var(--syn-number)' },
  { tag: tags.string, color: 'var(--syn-string)' },
  { tag: tags.variableName, color: 'var(--syn-command)' },
]);

const theme = EditorView.theme({
  '&': { height: '100%', fontSize: '14px', backgroundColor: 'var(--bg-editor)', color: 'var(--fg)' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.65' },
  '.cm-content': { padding: '12px 0', caretColor: 'var(--accent)' },
  '.cm-gutters': {
    backgroundColor: 'var(--bg-editor)',
    color: 'var(--fg-muted)',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--bg-active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--bg-active-line)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--selection)',
  },
  '.cm-matchingBracket': { outline: '1px solid var(--syn-bracket)', backgroundColor: 'transparent' },
});

const hoverTheme = EditorView.baseTheme({
  '.cm-lsp-hover': {
    maxWidth: '480px',
    padding: '4px 8px',
    fontFamily: 'var(--font-mono)',
    fontSize: '12px',
    whiteSpace: 'pre-wrap',
  },
});

// The maths popover (S4.6). KaTeX's own CSS sizes the glyphs; this only frames them with the
// editor's tokens so light and dark both work, and colours the one-line parse message like the
// diagnostics drawer does.
const mathPreviewTheme = EditorView.baseTheme({
  '.cm-math-preview': {
    maxWidth: '640px',
    padding: '6px 12px',
    backgroundColor: 'var(--bg-panel)',
    color: 'var(--fg)',
    fontSize: '15px',
    overflowX: 'auto',
  },
  '.cm-math-preview-error': {
    maxWidth: '480px',
    padding: '4px 8px',
    backgroundColor: 'var(--bg-panel)',
    color: 'var(--error)',
    fontFamily: 'var(--font-ui)',
    fontSize: '12px',
  },
});

export function createEditor(
  parent: HTMLElement,
  doc: OpenDocument,
  completionSource?: CompletionSource,
  hoverRequest?: HoverRequester,
  definitionRequest?: DefinitionRequester,
  forwardSearchRequest?: ForwardSearchRequester,
  focusModeEnabled = false,
  typewriterModeEnabled = false,
): EditorView {
  const state = EditorState.create({
    // y-codemirror requires the initial CodeMirror document to equal the Y.Text content.
    doc: doc.text(),
    extensions: [
      lineNumbers(),
      diagnosticGutter(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      EditorView.lineWrapping,
      StreamLanguage.define(stex),
      syntaxHighlighting(latexHighlight),
      // `override` replaces CodeMirror's built-in word-scanning source entirely rather than
      // running alongside it — once a language server can answer, its answers are what a LaTeX
      // author wants (real environment and command names), not words already in the buffer. With
      // no source (no project open yet) this is simply omitted, and `closeBrackets` above is
      // still enough to make typing feel finished with no server at all (DESIGN.md §2 commitment
      // 6).
      ...(completionSource ? [autocompletion({ override: [completionSource] })] : []),
      // Same "omit when there is no server" rule as completion above.
      ...(hoverRequest ? [lspHoverSource(hoverRequest), hoverTheme] : []),
      // Unconditional, unlike the two above: the maths preview needs no server, only KaTeX in
      // the bundle, so it is the non-LSP path that still works with nothing running.
      mathPreview(),
      mathPreviewTheme,
      // No app shortcuts here: `Ctrl S`, `Ctrl B`, `F5` and the rest are handled once, on the
      // window, by App.svelte (S2.4). A binding in this keymap does not stop propagation, so a
      // copy here would fire the action twice for a keypress inside the editor.
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...(definitionRequest ? definitionKeymap(definitionRequest) : []),
        ...(forwardSearchRequest ? forwardSearchKeymap(forwardSearchRequest) : []),
        indentWithTab,
      ]),
      ...(definitionRequest
        ? [EditorView.domEventHandlers({ mousedown: definitionClickHandler(definitionRequest) })]
        : []),
      // No awareness yet (that is v0.8); the undo manager is the document's own.
      yCollab(doc.ytext, null, { undoManager: doc.undo }),
      theme,
      // S4.5's two writing modes: pure view behaviour behind a `Compartment` each, so
      // `setFocusMode`/`setTypewriterMode` can flip them per keystroke of the command palette
      // rather than needing a fresh `createEditor` call.
      focusModeCompartment.of(focusModeExtension(focusModeEnabled)),
      typewriterModeCompartment.of(typewriterModeExtension(typewriterModeEnabled)),
    ],
  });
  return new EditorView({ state, parent });
}

/** Turn focus mode on or off in a live view (S4.5). A no-op reconfigure (toggling to the state
 * it is already in) is harmless — CodeMirror simply redraws the same decoration set — so callers
 * need not check first. */
export function setFocusMode(view: EditorView, enabled: boolean): void {
  view.dispatch({ effects: focusModeCompartment.reconfigure(focusModeExtension(enabled)) });
}

/** Turn typewriter mode on or off in a live view (S4.5). Same reconfigure-is-idempotent contract
 * as `setFocusMode`. */
export function setTypewriterMode(view: EditorView, enabled: boolean): void {
  view.dispatch({ effects: typewriterModeCompartment.reconfigure(typewriterModeExtension(enabled)) });
}

/** Move the cursor to a 1-based line, centre it, and focus the editor. */
export function goToLine(view: EditorView, line: number): void {
  const clamped = Math.max(1, Math.min(line, view.state.doc.lines));
  const { from } = view.state.doc.line(clamped);
  view.dispatch({
    selection: { anchor: from },
    effects: EditorView.scrollIntoView(from, { y: 'center' }),
  });
  view.focus();
}
