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

// ---- One word for it, in the editor too (S17.16) -------------------------------------------------
// It is a *document*, wherever this window says what is open. Kept on purpose, as things that have names:
// Build, Diagnostics, Source Control, Snapshots, and Git's own "repository" where the subject is Git
// (`CloneWindow`, `SourceControl`). The assistant's and Git's own panes still say project: they are not
// part of a first day, and are for a later loop.

const root = join(__dirname, '../..');
const read = (path: string) => readFileSync(join(root, path), 'utf8');

/** Every string in `source` that is a sentence, or could be part of one: it has a space in it. */
function sentencesIn(source: string): string[] {
  const code = source
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '')
    .replace(/\s\/\/ .*$/gm, '')
    .replace(/'"'/g, "' '"); // Rust's one-character quote mark
  return [...code.matchAll(/"((?:[^"\\]|\\[\s\S])*)"|'((?:[^'\\\n]|\\.)*)'|`((?:[^`\\]|\\[\s\S])*)`/g)]
    .map((match) => match[1] ?? match[2] ?? match[3] ?? '')
    .map((text) => text.replace(/\$\{[^}]*\}|\{[^}]*\}/g, '…')) // what is filled in is not ours
    .filter((text) => text.trim().includes(' '));
}

const inComponents = ['App', 'components/Editor', 'components/Sidebar', 'components/QuickOpen', 'components/PdfPane', 'components/StatusBar']
  .map((name) => read(`src/${name}.svelte`))
  .flatMap((source) => {
    const markup = source.replace(/<script[\s\S]*?<\/script>/g, '');
    const script = /<script[^>]*>([\s\S]*?)<\/script>/.exec(source)?.[1] ?? '';
    const attributes = [...markup.matchAll(/(?:aria-label|placeholder|title|alt)="([^"]*)"/g)].map((match) => match[1]!);
    return [ownText(source), ...attributes.map((text) => text.replace(/\{[^}]*\}/g, '…')), ...sentencesIn(script)];
  });

const inRust = ['commands', 'compile', 'project', 'templates', 'synctex', 'watcher']
  .map((name) => read(`src-tauri/src/${name}.rs`).split('#[cfg(test)]')[0]!)
  .flatMap(sentencesIn);

const sentencesSaying = (word: RegExp) =>
  [...inComponents, ...sentencesIn(read('src/lib/controller.svelte.ts')), ...sentencesIn(read('src/lib/ipc.ts')), ...inRust].filter((text) =>
    word.test(text),
  );

it('finds sentences in all of them, so the next guard cannot pass by matching nothing', () => {
  for (const known of [/root \.tex file/, /was not found on this machine|could not be watched/i, /The PDF will appear here/, /Select some text first/, /Choose where to put/]) {
    expect(sentencesSaying(known), String(known)).not.toEqual([]);
  }
});

it('the window says document, never project', () => {
  expect(sentencesSaying(/\bprojects?\b/i)).toEqual([]);
});

// ---- Help (S17.16) ------------------------------------------------------------------------------

it('Help has eight questions, each with an answer', () => {
  expect(words.help).toHaveLength(8);
  const screen = read('src/components/StartScreen.svelte');
  expect(screen).toContain("goToPage('help', 'heading')");
  expect(screen).toContain('<StartHelp />');
  expect(words.help.filter((h) => !h.question.endsWith('?') || h.answer.length < 40)).toEqual([]);
});
