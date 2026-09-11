// Keyboard shortcuts as one table (S2.4). App.svelte turns a keydown into an action name with
// `shortcutFor` and dispatches it; nothing else in the app binds a global key. When S4.3 grows
// this into the command palette, the table is what the palette lists — which is why it holds a
// human-readable label next to each chord instead of only the chord.
//
// One dispatcher, not two: CodeMirror's keymap used to bind `Mod-s`/`Mod-b`/`F5` as well, and a
// CodeMirror binding that handles a key calls preventDefault but does *not* stop propagation,
// so `Ctrl B` pressed inside the editor reached both handlers and compiled twice.

export type Action = 'save' | 'compile' | 'open-folder' | 'quick-open';

export interface Shortcut {
  action: Action;
  /** `Mod` is Ctrl on Windows and Linux, ⌘ on macOS. Displayed as written, so keep it short. */
  keys: string;
  label: string;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { action: 'save', keys: 'Mod-S', label: 'Save' },
  { action: 'compile', keys: 'Mod-B', label: 'Build' },
  { action: 'compile', keys: 'F5', label: 'Build' },
  { action: 'open-folder', keys: 'Mod-O', label: 'Open folder…' },
  { action: 'quick-open', keys: 'Mod-P', label: 'Go to file…' },
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
  // Alt and Shift are not part of any chord here, and a chord that ignores them would steal
  // `Ctrl Shift S` or `Alt F5` from whatever the platform means by those.
  if (event.altKey || event.shiftKey) return null;
  const mod = event.ctrlKey || event.metaKey;
  for (const shortcut of SHORTCUTS) {
    const [modifier, key] = splitChord(shortcut.keys);
    if (modifier === 'Mod' && !mod) continue;
    if (modifier === null && mod) continue;
    if (event.key.toLowerCase() === key.toLowerCase()) return shortcut.action;
  }
  return null;
}

/** `'Mod-S'` → `['Mod', 'S']`; `'F5'` → `[null, 'F5']`. */
function splitChord(keys: string): [string | null, string] {
  const dash = keys.indexOf('-');
  if (dash === -1) return [null, keys];
  return [keys.slice(0, dash), keys.slice(dash + 1)];
}
