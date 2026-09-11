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

  it('does not steal chords that carry Shift or Alt', () => {
    expect(shortcutFor(press('S', { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(shortcutFor(press('s', { ctrlKey: true, altKey: true }))).toBeNull();
    expect(shortcutFor(press('F5', { altKey: true }))).toBeNull();
  });

  it('ignores the case the browser reports the key in', () => {
    // With Caps Lock on, `key` is 'S' rather than 's'.
    expect(shortcutFor(press('S', { ctrlKey: true }))).toBe('save');
  });

  it('every entry in the table is reachable', () => {
    for (const shortcut of SHORTCUTS) {
      const [modifier, key] = shortcut.keys.includes('-') ? shortcut.keys.split('-') : [null, shortcut.keys];
      expect(shortcutFor(press(key!, { ctrlKey: modifier === 'Mod' }))).toBe(shortcut.action);
    }
  });
});
