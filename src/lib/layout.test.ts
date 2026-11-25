// S17.13: when three panes do not fit, the window shows one. The numbers are read from the stylesheet, so
// that a pane that is made wider moves the fold with it or fails here.

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { FOLDED } from './layout';

const css = readFileSync(new URL('../app.css', import.meta.url), 'utf8');

/** Everything between the braces that follow `start`. */
function block(start: string): string {
  const from = css.indexOf(start);
  expect(from, `${start} is in app.css`).toBeGreaterThan(-1);
  let depth = 0;
  for (let at = css.indexOf('{', from); at < css.length; at += 1) {
    if (css[at] === '{') depth += 1;
    if (css[at] === '}' && --depth === 0) return css.slice(css.indexOf('{', from) + 1, at);
  }
  throw new Error(`${start} never ends`);
}

const pixels = (pattern: RegExp, text: string) => [...text.matchAll(pattern)].map((match) => Number(match[1]));

describe('the panes fold', () => {
  const foldRules = block(`@media ${FOLDED}`);

  it('exactly below the width three panes need', () => {
    const [activityBar] = pixels(/--activity-bar-width: (\d+)px/g, css);
    const [sidebar] = pixels(/--sidebar-width: (\d+)px/g, css);
    const [editorMinimum, pdfMinimum] = pixels(/minmax\((\d+)px, 1fr\)/g, block('\n.app {'));
    const [foldWidth] = pixels(/max-width: (\d+)px/g, FOLDED);
    expect(foldWidth).toBe(activityBar! + sidebar! + editorMinimum! + pdfMinimum! - 1);
  });

  it('and a hidden pane stays laid out, so that pdf.js and the editor can go on measuring', () => {
    expect(foldRules).toContain('visibility: hidden');
    const hiding = foldRules.split('}').find((rule) => rule.includes('visibility: hidden'))!;
    expect(hiding).not.toContain('display: none');
    for (const pane of ['.left-pane', '.editor-column', '.pdf-column']) {
      expect(hiding).toContain(pane);
      expect(foldRules.replace(hiding, '')).toMatch(new RegExp(`data-pane='\\w+'\\] > \\${pane}`));
    }
  });

  it('and what the window has to say is seen whichever pane is', () => {
    expect(foldRules).toMatch(/\.editor-column > \.messages \{[^}]*visibility: visible/);
  });

  it('and the choice between them is drawn only there', () => {
    expect(block('\n.pane-choice {')).toContain('display: none');
    expect(foldRules).toMatch(/\.pane-choice \{\s*display: flex/);
  });

  it('and nothing in a pane shows through when the pane is hidden', () => {
    for (const name of ['Sidebar', 'SourceControl', 'CommentsView', 'AssistantView', 'Editor', 'Tabs', 'Drawer', 'PdfPane', 'TreeItem', 'DocumentMap']) {
      const component = readFileSync(new URL(`../components/${name}.svelte`, import.meta.url), 'utf8');
      expect(component, name).not.toMatch(/visibility: visible/);
    }
  });
});
