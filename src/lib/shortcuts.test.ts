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

  it('opens Home with Ctrl Shift H (S17.5)', () => {
    expect(shortcutFor(press('H', { ctrlKey: true, shiftKey: true }))).toBe('home');
    expect(shortcutFor(press('h', { ctrlKey: true }))).toBeNull();
  });

  it('jumps to the next placeholder with F8 alone (S11.12)', () => {
    expect(shortcutFor(press('F8'))).toBe('next-placeholder');
    expect(shortcutFor(press('F8', { ctrlKey: true }))).toBeNull();
  });

  it('opens the search of the PDF with Ctrl Shift F', () => {
    expect(shortcutFor(press('F', { ctrlKey: true, shiftKey: true }))).toBe('find-in-pdf');
    // Plain Ctrl F belongs to whatever has focus: the editor's own search, and the PDF pane's.
    expect(shortcutFor(press('f', { ctrlKey: true }))).toBeNull();
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

// R7.5. With a layout that does not type Latin letters, Ctrl S reports the layout's own character.
describe('shortcutFor on a layout that does not type Latin letters (R7.5)', () => {
  it('uses the key\'s place on the keyboard, so a Russian layout still saves, builds and opens', () => {
    expect(shortcutFor(press('ы', { ctrlKey: true, code: 'KeyS' }))).toBe('save');
    expect(shortcutFor(press('и', { ctrlKey: true, code: 'KeyB' }))).toBe('compile');
    expect(shortcutFor(press('щ', { ctrlKey: true, code: 'KeyO' }))).toBe('open-folder');
    expect(shortcutFor(press('З', { ctrlKey: true, shiftKey: true, code: 'KeyP' }))).toBeNull(); // Ctrl Shift P is not a chord
    expect(shortcutFor(press('У', { ctrlKey: true, shiftKey: true, code: 'KeyE' }))).toBe('view-files');
  });

  it('keeps the letter a Latin layout prints, whatever key it is on (Dvorak puts S where QWERTY has O)', () => {
    expect(shortcutFor(press('s', { ctrlKey: true, code: 'KeyO' }))).toBe('save');
    expect(shortcutFor(press('o', { ctrlKey: true, code: 'KeyS' }))).toBe('open-folder');
  });

  it('still needs the modifier, and still ignores Alt', () => {
    expect(shortcutFor(press('ы', { code: 'KeyS' }))).toBeNull();
    expect(shortcutFor(press('ы', { ctrlKey: true, altKey: true, code: 'KeyS' }))).toBeNull();
  });

  it('does not guess from a key that has no letter on it', () => {
    expect(shortcutFor(press('1', { ctrlKey: true, code: 'Digit1' }))).toBeNull();
    expect(shortcutFor(press('Dead', { ctrlKey: true, code: 'KeyS' }))).toBeNull();
  });
});
