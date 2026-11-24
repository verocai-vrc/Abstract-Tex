// Guards over the start screen's markup, not renders (as `dialogs.test.ts`): rules a template can
// quietly stop obeying. Every sentence lives in `words.ts`, so that it can be translated; none of them
// uses a word this audience should not need; nothing is too small to read or to press (S17.4).
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { words } from '../lib/words';

const components = readdirSync(__dirname)
  .filter((name) => /^(Start\w+|OpeningNotice|SavedPdfNotice)\.svelte$/.test(name))
  .map((name) => ({ name, source: readFileSync(join(__dirname, name), 'utf8') }));

/** What is left of a template once everything that is not its own text is gone. */
function ownText(source: string): string {
  let rest = source
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '');
  // Innermost braces first, until none are left: an expression may hold others.
  for (let before = ''; before !== rest; ) {
    before = rest;
    rest = rest.replace(/\{[^{}]*\}/g, '');
  }
  return rest.replace(/<[^>]*>/g, '');
}

/** Every sentence in `words`, with a stand-in for what is filled in. */
function everySentence(value: unknown): string[] {
  if (typeof value === 'string') return [value];
  if (typeof value === 'function') return everySentence(value('x', 2, 'x', 'x'));
  if (value && typeof value === 'object') return Object.values(value).flatMap(everySentence);
  return [];
}

it('finds the components, so these guards cannot pass by matching nothing', () => {
  expect(components.map((c) => c.name)).toEqual(
    expect.arrayContaining(['StartScreen.svelte', 'StartHome.svelte', 'StartOpen.svelte', 'OpeningNotice.svelte']),
  );
});

describe.each(components)('$name', ({ source }) => {
  it('has no words of its own', () => {
    expect(ownText(source).replace(/[\s›‹·×]/g, '')).toBe('');
    expect(source).not.toMatch(/(aria-label|placeholder|title|alt)="[^"{]+"/);
  });

  it('sets no text under 16 px and no target under 44 px', () => {
    const sizes = (property: string) =>
      [...source.matchAll(new RegExp(`[^-]${property}:\\s*([\\d.]+)px`, 'g'))].map((match) => Number(match[1]));
    expect(sizes('font-size').filter((px) => px < 16)).toEqual([]);
    expect(source).not.toMatch(/font-size:\s*[\d.]+(em|rem|%)/);
    expect(sizes('min-height').filter((px) => px < 44)).toEqual([]);
  });
});

it('every button of the start screen is at least 44 px high', () => {
  const screen = components.find((c) => c.name === 'StartScreen.svelte')!.source;
  expect(screen).toMatch(/:global\(\.start :is\(button, input\)\) \{[^}]*min-height: 44px/);
});

it('never says project, repository or compile', () => {
  const sentences = everySentence(words);
  expect(sentences.length).toBeGreaterThan(20);
  expect(sentences.join('\n')).not.toMatch(/\b(project|repositor|compil)/i);
});
