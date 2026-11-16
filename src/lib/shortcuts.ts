// Keyboard shortcuts as one table (S2.4). App.svelte turns a keydown into an action name with
// `shortcutFor` and dispatches it; nothing else in the app binds a global key. When S4.3 grows
// this into the command palette, the table is what the palette lists — which is why it holds a
// human-readable label next to each chord instead of only the chord.
//
// One dispatcher, not two: CodeMirror's keymap used to bind `Mod-s`/`Mod-b`/`F5` as well, and a
// CodeMirror binding that handles a key calls preventDefault but does *not* stop propagation,
// so `Ctrl B` pressed inside the editor reached both handlers and compiled twice.

export type Action =
  | 'save'
  | 'compile'
  | 'open-folder'
  | 'new-project'
  | 'next-placeholder'
  | 'quick-open'
  | 'command-palette'
  | 'view-files'
  | 'view-source-control'
  | 'find-in-pdf';

export interface Shortcut {
  action: Action;
  /** `Mod` is Ctrl on Windows and Linux, ⌘ on macOS; `Shift` is itself. Displayed as written,
   * so keep it short. */
  keys: string;
  label: string;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { action: 'save', keys: 'Mod-S', label: 'Save' },
  { action: 'compile', keys: 'Mod-B', label: 'Build' },
  { action: 'compile', keys: 'F5', label: 'Build' },
  { action: 'open-folder', keys: 'Mod-O', label: 'Open folder…' },
  // S11.11: the front door for someone with no .tex yet (DESIGN.md §6).
  // S11.12: F8 is "next problem" in VS Code, and a placeholder is the same kind of thing: a place the
  // document is not finished.
  { action: 'next-placeholder', keys: 'F8', label: 'Go to next placeholder' },
  { action: 'new-project', keys: 'Mod-Shift-N', label: 'New project from template…' },
  { action: 'quick-open', keys: 'Mod-P', label: 'Go to file…' },
  { action: 'command-palette', keys: 'Mod-K', label: 'Command palette…' },
  // S10.3a: the activity bar, bound exactly as VS Code binds it (DESIGN.md §6). These are the
  // first chords here with Shift in them, which is why `shortcutFor` below had to grow.
  { action: 'view-files', keys: 'Mod-Shift-E', label: 'Files' },
  { action: 'view-source-control', keys: 'Mod-Shift-G', label: 'Source Control' },
  // Searching the text of the PDF. Plain Ctrl F is not here on purpose: it belongs to whatever has
  // focus (the editor's search, or the PDF pane's own bar). This one works from anywhere.
  { action: 'find-in-pdf', keys: 'Mod-Shift-F', label: 'Find in the PDF' },
];

/** The fields of a `KeyboardEvent` the table cares about, so tests need not build a real one. */
export interface KeyFacts {
  key: string;
  /** Where the key is on the keyboard (`KeyS`), whatever the layout prints on it. Optional so tests
   * and callers that have only the letter still work. */
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/** Which action, if any, this keypress asks for. */
export function shortcutFor(event: KeyFacts): Action | null {
  // Alt is not part of any chord here, and a chord that ignored it would steal `Alt F5` from
  // whatever the platform means by that. Shift *is* part of two chords since S10.3a, so it is
  // matched exactly rather than rejected: `Ctrl Shift S` must still reach the platform, which
  // it does, because no entry in the table asks for it.
  if (event.altKey) return null;
  const mod = event.ctrlKey || event.metaKey;
  for (const shortcut of SHORTCUTS) {
    const chord = splitChord(shortcut.keys);
    if (chord.mod !== mod) continue;
    if (chord.shift !== event.shiftKey) continue;
    if (keyForChord(event).toLowerCase() === chord.key.toLowerCase()) return shortcut.action;
  }
  return null;
}

/**
 * The key a chord is matched against (R7.5). Normally what the layout prints: Dvorak and AZERTY
 * users press the letter they see. But a layout that does not type Latin letters (Cyrillic, Greek,
 * Hebrew, Arabic) reports its own character for Ctrl S, which no chord names, so for those — and
 * only those — the key's place on the keyboard stands in: `KeyS` is the S of a QWERTY board.
 */
function keyForChord(event: KeyFacts): string {
  const printable = event.key.length === 1;
  const isAscii = printable && event.key.charCodeAt(0) < 128;
  if (!printable || isAscii) return event.key;
  const place = event.code;
  return place !== undefined && /^Key[A-Z]$/.test(place) ? place.slice(3) : event.key;
}

interface Chord {
  mod: boolean;
  shift: boolean;
  key: string;
}

/** `'Mod-S'` → Ctrl and `S`; `'Mod-Shift-E'` → Ctrl, Shift and `E`; `'F5'` → no modifier. */
function splitChord(keys: string): Chord {
  const parts = keys.split('-');
  const key = parts.pop() ?? keys;
  return { mod: parts.includes('Mod'), shift: parts.includes('Shift'), key };
}
