import { describe, expect, it } from 'vitest';
import { searchKeymap } from '@codemirror/search';
import { SHORTCUTS } from '../shortcuts';
import { editorSearchKeymap } from './setup';

describe('the editor search keymap', () => {
  it('does not bind find-next on Mod-g, which carries Ctrl Shift G, the app’s Source Control chord', () => {
    expect(searchKeymap.some((binding) => binding.key === 'Mod-g')).toBe(true); // the library still has it
    expect(editorSearchKeymap.some((binding) => binding.key === 'Mod-g')).toBe(false);
  });

  it('keeps find, F3 and Shift F3', () => {
    const keys = editorSearchKeymap.map((binding) => binding.key);
    expect(keys).toContain('Mod-f');
    const f3 = editorSearchKeymap.find((binding) => binding.key === 'F3');
    expect(f3?.run).toBeTypeOf('function'); // find next
    expect(f3?.shift).toBeTypeOf('function'); // Shift F3: find previous
  });

  it('binds no key that an app shortcut already owns, so one keypress does one thing', () => {
    const chords = SHORTCUTS.map((shortcut) => shortcut.keys.toLowerCase());
    for (const binding of editorSearchKeymap) {
      const keys = [binding.key, binding.mac, binding.win, binding.linux].filter((key): key is string => !!key);
      for (const key of keys) {
        expect(chords, `${key} is bound by both the editor search and the app`).not.toContain(key.toLowerCase());
        if (binding.shift) expect(chords).not.toContain(`${key}-shift`.toLowerCase());
      }
    }
  });
});
