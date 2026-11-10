// A guard for S16.4: every text colour in the theme keeps WCAG AA contrast (4.5:1) against the
// surfaces it is drawn on, in both the light and the dark palette. Reads `app.css` itself, so
// changing a token without checking it fails here, rather than in front of someone who cannot
// read the result.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const css = readFileSync(path.resolve('src/app.css'), 'utf8');

function tokens(block: string): Record<string, string> {
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1]!, m[2]!]));
}

const darkStart = css.indexOf('@media (prefers-color-scheme: dark)');
const light = tokens(css.slice(css.indexOf(':root'), darkStart));
const darkOverrides = tokens(css.slice(darkStart, css.indexOf('\n}\n', css.indexOf(':root', darkStart))));
const palettes = { light, dark: { ...light, ...darkOverrides } };

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const channel = parseInt(hex.slice(i, i + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

export function contrast(a: string, b: string): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter! + 0.05) / (darker! + 0.05);
}

const TEXT = ['fg', 'fg-muted', 'accent', 'ok', 'warn', 'error', 'syn-command', 'syn-keyword', 'syn-comment', 'syn-math', 'syn-bracket', 'syn-number', 'syn-string'];
const SURFACES = ['bg', 'bg-panel', 'bg-editor'];

describe('theme contrast', () => {
  for (const [name, palette] of Object.entries(palettes)) {
    it(`${name}: every text colour reads at 4.5:1 on every surface`, () => {
      const failures: string[] = [];
      for (const text of TEXT) {
        for (const surface of SURFACES) {
          const ratio = contrast(palette[text]!, palette[surface]!);
          if (ratio < 4.5) failures.push(`${text} on ${surface}: ${ratio.toFixed(2)}`);
        }
      }
      expect(failures).toEqual([]);
    });

    it(`${name}: text on a primary button reads at 4.5:1`, () => {
      expect(contrast(palette['accent-fg']!, palette['accent']!)).toBeGreaterThanOrEqual(4.5);
    });
  }
});
