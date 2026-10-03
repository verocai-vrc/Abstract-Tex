import { describe, expect, it } from 'vitest';
import { SHORTCUTS, shortcutFor, type KeyFacts } from './shortcuts';

function press(key: string, mods: Partial<KeyFacts> = {}): KeyFacts {
  return { key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods };
}

describe('shortcutFor', () => {
  it('maps the four card shortcuts, with Ctrl on Windows/Linux', () => {
    expect(shortcutFor(press('s', { ctrlKey: true }))).toBe('save');
    expect(shortcutFor(press('b', { ctrlKey: true }))).toBe('compile');
    expect(shortcutFor(press('o', { ctrlKey: true }))).toBe('open-folder');
    expect(shortcutFor(press('p', { ctrlKey: true }))).toBe('quick-open');
  });

  it('accepts ⌘ as the modifier on macOS', () => {
    expect(shortcutFor(press('s', { metaKey: true }))).toBe('save');
  });

  it('F5 builds with no modifier, and is not a chord', () => {
    expect(shortcutFor(press('F5'))).toBe('compile');
    expect(shortcutFor(press('F5', { ctrlKey: true }))).toBeNull();
  });

  it('a bare letter is typing, not a shortcut', () => {
    expect(shortcutFor(press('s'))).toBeNull();
    expect(shortcutFor(press('b'))).toBeNull();
  });

  it('reaches the activity bar the way VS Code does (S10.3a)', () => {
    expect(shortcutFor(press('E', { ctrlKey: true, shiftKey: true }))).toBe('view-files');
    expect(shortcutFor(press('G', { ctrlKey: true, shiftKey: true }))).toBe('view-source-control');
  });

  it('starts a project from a template with Ctrl Shift N (S11.11)', () => {
    expect(shortcutFor(press('N', { ctrlKey: true, shiftKey: true }))).toBe('new-project');
    expect(shortcutFor(press('n', { ctrlKey: true }))).toBeNull();
  });

  it('does not steal a chord that carries Shift or Alt and is not in the table', () => {
    // `Ctrl Shift S` is the platform's, and `Ctrl E` is not "Files" — Shift is matched exactly,
    // not ignored, so neither the chord with it nor the one without it reaches the other's action.
    expect(shortcutFor(press('S', { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(shortcutFor(press('e', { ctrlKey: true }))).toBeNull();
    expect(shortcutFor(press('s', { ctrlKey: true, altKey: true }))).toBeNull();
    expect(shortcutFor(press('E', { ctrlKey: true, shiftKey: true, altKey: true }))).toBeNull();
    expect(shortcutFor(press('F5', { altKey: true }))).toBeNull();
  });

  it('ignores the case the browser reports the key in', () => {
    // With Caps Lock on, `key` is 'S' rather than 's'.
    expect(shortcutFor(press('S', { ctrlKey: true }))).toBe('save');
  });

  it('every entry in the table is reachable', () => {
    for (const shortcut of SHORTCUTS) {
      const parts = shortcut.keys.split('-');
      const key = parts.pop()!;
      expect(shortcutFor(press(key, { ctrlKey: parts.includes('Mod'), shiftKey: parts.includes('Shift') }))).toBe(
        shortcut.action,
      );
    }
  });
});
