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
  | 'quick-open'
  | 'command-palette'
  | 'view-files'
  | 'view-source-control';

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
  { action: 'quick-open', keys: 'Mod-P', label: 'Go to file…' },
  { action: 'command-palette', keys: 'Mod-K', label: 'Command palette…' },
  // S10.3a: the activity bar, bound exactly as VS Code binds it (DESIGN.md §6). These are the
  // first chords here with Shift in them, which is why `shortcutFor` below had to grow.
  { action: 'view-files', keys: 'Mod-Shift-E', label: 'Files' },
  { action: 'view-source-control', keys: 'Mod-Shift-G', label: 'Source Control' },
];

/** The fields of a `KeyboardEvent` the table cares about, so tests need not build a real one. */
export interface KeyFacts {
  key: string;
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
    if (event.key.toLowerCase() === chord.key.toLowerCase()) return shortcut.action;
  }
  return null;
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
