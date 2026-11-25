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
  expect(screen).toMatch(/:global\(\.start button\),\s*:global\(\.start input\) \{[^}]*min-height: 44px/);
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

// ---- The accessible tree (S17.17) ---------------------------------------------------------------

// WebKit stops calling a `<ul>` a list when it has `list-style: none`, unless its role says so. Seen in the
// tree itself: the recent documents were a "panel".
it('a list without bullets still says it is a list', () => {
  const lists = components.flatMap(({ name, source }) => (source.match(/<[uo]l\b[^>]*>/g) ?? []).map((tag) => `${name} ${tag}`));
  expect(lists.length).toBeGreaterThan(3);
  expect(lists.filter((tag) => !tag.includes('role="list"'))).toEqual([]);
});

// Keys scroll what holds the focus, and Help has nothing to press: without a stop in it, one Tab from its title
// left a keyboard nothing that could move the page.
it('Help can be reached and scrolled with the keyboard alone', () => {
  expect(read('src/components/StartHelp.svelte')).toMatch(/<h1 tabindex="0"/);
  // And where the focus is can be seen there: only a title that Tab never stops at goes without its ring.
  expect(read('src/components/StartScreen.svelte')).toMatch(/h1\[tabindex='-1'\]:focus\)/);
});

// ---- Always in sight (S17.18) -------------------------------------------------------------------

/** The declarations of the rule with exactly this selector, inside `@media media` when one is given. */
function declarations(source: string, selector: string, media = ''): string {
  let css = /<style>([\s\S]*)<\/style>/.exec(source)![1]!.replace(/\/\*[\s\S]*?\*\//g, '');
  if (media !== '') css = css.slice(css.indexOf(media) + media.length);
  return css.split('}').find((rule) => rule.split('{').at(-2)?.trim() === selector)?.split('{').at(-1) ?? '';
}

const startScreenSource = read('src/components/StartScreen.svelte');

// A list scrolled to its end, or enlarged text, has the top of the page out of the window: what was said
// there looked like nothing at all.
it('what the start screen has to say is not in the part that scrolls', () => {
  const main = /<main[\s\S]*<\/main>/.exec(startScreenSource)![0];
  expect(startScreenSource).toContain('role="alert"');
  expect(startScreenSource).toContain('<SavedPdfNotice />');
  expect(main).not.toMatch(/role="(alert|status)"|<SavedPdfNotice/);
  // Why nothing was made, too: *Create* is at the end of a page, and its title, which is asked for then, at the start.
  expect(startScreenSource).toContain('{newProject.error}');
  expect(read('src/components/StartNew.svelte')).not.toContain('{newProject.error}');
});

it('what the Open page says of a choice, and its buttons, are scrolled to', () => {
  expect(read('src/components/StartOpen.svelte')).toMatch(/scrollIntoView\(\{ block: 'nearest' \}\)/);
});

// At 200% in the smallest window, a long title's *Back to …* was higher than the window.
it('in a narrow window the list of pages leaves room for the page', () => {
  const narrow = '@media (max-width: 760px)';
  expect(declarations(startScreenSource, 'nav', narrow)).toMatch(/max-height: \d+vh/);
  expect(declarations(startScreenSource, '.back', narrow)).toMatch(/white-space: nowrap/);
  expect(declarations(startScreenSource, '.start', narrow)).toMatch(/grid-template-rows: max-content auto minmax\(\d+px, 1fr\)/);
});

// The build adds `:-webkit-any()` beside it for older Safaris, which counts as a class and not as what is in
// it: the rule then beats the ones meant to beat it, though only in what is built, and not in development.
it('no style of ours uses :is()', () => {
  const ours = readdirSync(__dirname).filter((name) => name.endsWith('.svelte')).map((name) => `src/components/${name}`);
  expect(['src/app.css', 'src/App.svelte', ...ours].filter((path) => /:is\(/.test(read(path)))).toEqual([]);
});
