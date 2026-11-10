// A guard over the markup, not a render: the repository has no component harness, and the rule it
// protects is one a template can quietly stop obeying. Every modal dialog (a `role="dialog"` over a
// `.backdrop`) must keep keyboard focus inside itself (`use:modalFocus`, `lib/dialog.ts`), close on
// Escape, and say it is modal to a screen reader (`aria-modal`).
//
// Found by an agentic review (R4.8, R4.14): `BibliographyHealth` and `ZoteroLink` had neither the
// focus action nor an Escape handler, which is exactly the bug `dialog.ts` was written for (focus
// stays on the editor behind, and typed text lands in the manuscript), and no dialog set `aria-modal`.

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const COMPONENTS = join(__dirname);

/** Palettes that put focus in their own text field when they open, and say so in their own code. */
const FOCUS_THEIR_OWN_INPUT = new Set(['CommandPalette.svelte', 'QuickOpen.svelte']);

const modalDialogs = readdirSync(COMPONENTS)
  .filter((name) => name.endsWith('.svelte'))
  .map((name) => ({ name, source: readFileSync(join(COMPONENTS, name), 'utf8') }))
  .filter(({ source }) => source.includes('role="dialog"') && source.includes('class="backdrop"'));

describe('every modal dialog', () => {
  it('is found (so this test cannot pass by matching nothing)', () => {
    expect(modalDialogs.length).toBeGreaterThanOrEqual(10);
  });

  for (const { name, source } of modalDialogs) {
    it(`${name} closes on Escape`, () => {
      expect(source, `${name} has no Escape handler`).toMatch(/['"]Escape['"]/);
    });

    it(`${name} says it is modal to assistive technology`, () => {
      expect(source, `${name} has no aria-modal="true"`).toContain('aria-modal="true"');
    });

    if (!FOCUS_THEIR_OWN_INPUT.has(name)) {
      it(`${name} keeps keyboard focus inside itself`, () => {
        expect(source, `${name} does not use modalFocus`).toContain('use:modalFocus');
      });
    }
  }
});
