// A guard for S16.4: every text colour in the theme keeps its contrast against the surfaces it is drawn on,
// WCAG AA (4.5:1) in the ordinary palettes and AAA (7:1) in the high-contrast ones (S17.12). Reads
// `app.css` itself, so changing a token without checking it fails here, rather than in front of someone
// who cannot read the result.

import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const css = readFileSync(path.resolve('src/app.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');

/** The tokens one selector sets. By the selector and not by where it is in the file. */
function tokens(selector: string): Record<string, string> {
  const block = css.split('}').find((rule) => rule.split('{')[0]!.trim() === selector);
  if (block === undefined) throw new Error(`app.css has no ${selector}`);
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1]!, m[2]!.trim()]));
}

const light = tokens(':root');
const palettes: Record<string, Record<string, string>> = {
  light,
  ...Object.fromEntries(
    [...css.matchAll(/:root\[data-look='([\w-]+)'\]/g)].map((m) => [m[1]!, { ...light, ...tokens(m[0]) }]),
  ),
};

type Rgb = [number, number, number];

function rgb(hex: string): Rgb {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) throw new Error(`not a plain colour: ${hex}`);
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb;
}

/** What a tint looks like on a surface. */
function over(tint: string, surface: string): Rgb {
  const match = /^rgba\((\d+), (\d+), (\d+), ([\d.]+)\)$/.exec(tint);
  if (!match) throw new Error(`not a tint: ${tint}`);
  const [r, g, b, alpha] = match.slice(1).map(Number) as [number, number, number, number];
  return rgb(surface).map((under, i) => [r, g, b][i]! * alpha + under * (1 - alpha)) as Rgb;
}

function luminance(colour: Rgb): number {
  const [r, g, b] = colour.map((value) => {
    const channel = value / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(a: Rgb, b: Rgb): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter! + 0.05) / (darker! + 0.05);
}

const TEXT = ['fg', 'fg-muted', 'accent', 'ok', 'warn', 'error', 'syn-command', 'syn-keyword', 'syn-comment', 'syn-math', 'syn-bracket', 'syn-number', 'syn-string'];
const SURFACES = ['bg', 'bg-panel', 'bg-editor'];
/** A line, a hovered or chosen row, the selection. What is written there stays readable, too. */
const TINTS = ['bg-active-line', 'bg-hover', 'bg-selected'];

it('finds the four palettes', () => {
  expect(Object.keys(palettes)).toEqual(['light', 'dark', 'contrast-light', 'contrast-dark']);
});

describe.each(Object.entries(palettes))('%s', (name, palette) => {
  const isHighContrast = name.startsWith('contrast');
  const enough = isHighContrast ? 7 : 4.5;

  it(`every text colour reads at ${enough}:1 on every surface`, () => {
    const failures: string[] = [];
    for (const text of TEXT) {
      for (const surface of SURFACES) {
        const ratio = contrast(rgb(palette[text]!), rgb(palette[surface]!));
        if (ratio < enough) failures.push(`${text} on ${surface}: ${ratio.toFixed(2)}`);
      }
    }
    expect(failures).toEqual([]);
  });

  it(`text on a primary button reads at ${enough}:1`, () => {
    expect(contrast(rgb(palette['accent-fg']!), rgb(palette['accent']!))).toBeGreaterThanOrEqual(enough);
  });

  it('selected text in the editor reads at 4.5:1', () => {
    expect(contrast(rgb(palette['fg']!), over(palette['selection']!, palette['bg-editor']!))).toBeGreaterThanOrEqual(4.5);
  });

  // What is hovered, pressed or chosen has a tint, and the muted colour does not read on all of them.
  it(`the text of what is hovered, pressed or chosen reads at ${enough}:1`, () => {
    const failures: string[] = [];
    for (const tint of TINTS) {
      for (const surface of SURFACES) {
        for (const text of ['fg', 'accent']) {
          const ratio = contrast(rgb(palette[text]!), over(palette[tint]!, palette[surface]!));
          if (ratio < enough) failures.push(`${text} on ${tint} on ${surface}: ${ratio.toFixed(2)}`);
        }
      }
    }
    for (const text of TEXT) {
      const ratio = contrast(rgb(palette[text]!), over(palette['bg-active-line']!, palette['bg-editor']!));
      if (ratio < enough) failures.push(`${text} on the line the cursor is in: ${ratio.toFixed(2)}`);
    }
    expect(failures).toEqual([]);
  });

  if (!isHighContrast) return;

  it('every text colour also reads on a chosen row, and the text on a selection', () => {
    const failures: string[] = [];
    const check = (text: string, tint: string, least: number) => {
      for (const surface of SURFACES) {
        const ratio = contrast(rgb(palette[text]!), over(palette[tint]!, palette[surface]!));
        if (ratio < least) failures.push(`${text} on ${tint} on ${surface}: ${ratio.toFixed(2)}`);
      }
    };
    for (const tint of TINTS) for (const text of TEXT) check(text, tint, 7);
    for (const text of TEXT) check(text, 'selection', 4.5);
    check('error', 'error-bg', 7);
    check('fg', 'error-bg', 7);
    check('ok', 'ok-bg', 7);
    expect(failures).toEqual([]);
  });

  it('borders and the focus ring are seen at 3:1', () => {
    for (const line of ['border', 'accent']) {
      for (const surface of SURFACES) {
        expect(contrast(rgb(palette[line]!), rgb(palette[surface]!)), `${line} on ${surface}`).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

it('says of each palette whether it is dark, so that the scrollbars and the boxes follow', () => {
  const schemes = [':root', ...['dark', 'contrast-light', 'contrast-dark'].map((name) => `:root[data-look='${name}']`)].map(
    (selector) => /color-scheme:\s*(\w+)/.exec(css.split('}').find((rule) => rule.split('{')[0]!.trim() === selector)!)?.[1],
  );
  expect(schemes).toEqual(['light', 'dark', 'light', 'dark']);
});

// CodeMirror paints a selection in an editor that has the focus a pale lilac, with a selector as particular
// as this; a shorter one of ours loses to it, and the text of a dark look is then pale on pale (S17.17).
it('the colour of a selection is ours also while the editor has the focus', () => {
  const selector = '.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground';
  const library = readFileSync(path.resolve('node_modules/@codemirror/view/dist/index.js'), 'utf8');
  expect(library).toContain(`"&light${selector}"`); // else it is another one that has to be beaten now
  expect(readFileSync(path.resolve('src/lib/editor/setup.ts'), 'utf8')).toContain(`&${selector}`);
});

it('no component has a colour of its own', () => {
  const folder = path.resolve('src/components');
  const literalColours = readdirSync(folder)
    .filter((file) => file.endsWith('.svelte'))
    .flatMap((file) => {
      const style = /<style>([\s\S]*)<\/style>/.exec(readFileSync(path.join(folder, file), 'utf8'))?.[1] ?? '';
      const colours = style
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .match(/#[0-9a-f]{3,8}\b|\b(rgba?|hsla?)\([^)]*\)|:\s*(white|black|red|green|gr[ae]y)\b/gi);
      // Behind a dialog and around it, black; and paper is white whatever the window is.
      return (colours ?? [])
        .filter((colour) => !/^rgba\(0, 0, 0, [\d.]+\)$/.test(colour) && !/^:\s*white$/.test(colour))
        .map((colour) => `${file}: ${colour}`);
    });
  expect(literalColours).toEqual([]);
});

// The start screen's own rules (S17.18). What a box is for, before there is anything in it, is drawn in the
// webview's grey unless we say otherwise (WebKitGTK's is 2.3:1), and the edge of a box in `border` is 1.4:1.
it('the start screen draws no faint text and no faint box', () => {
  const screen = readFileSync(path.resolve('src/components/StartScreen.svelte'), 'utf8');
  const rule = (selector: string) => screen.split(`  :global(${selector}) {`)[1]?.split('}')[0] ?? '';
  expect(rule('.start button:hover .muted')).toMatch(/color: var\(--fg\);/);
  expect(rule('.start input::placeholder')).toMatch(/color: var\(--fg-muted\);[\s\S]*opacity: 1;/);
  expect(screen).toMatch(/:global\(\.start input\) \{[^}]*border: 1px solid var\(--fg-muted\);/);
  // Faded text is faint text. Only what cannot be pressed now may be.
  const faded = readdirSync(path.resolve('src/components'))
    .filter((file) => /^(Start\w+|OpeningNotice|SavedPdfNotice|FirstPdf)\.svelte$/.test(file))
    .flatMap((file) =>
      [...readFileSync(path.resolve('src/components', file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]*)\{[^{}]*[^-]opacity: 0?\.\d/g)].map((m) => m[1]!.trim()),
    );
  expect(faded).toEqual([":global(.start [aria-disabled='true'])"]);
});
