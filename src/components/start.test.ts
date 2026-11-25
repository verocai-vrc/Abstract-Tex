// Guards over the start screen's markup, not renders (as `dialogs.test.ts`): rules a template can
// quietly stop obeying. Every sentence lives in `words.ts`, so that it can be translated; none of them
// uses a word this audience should not need; nothing is too small to read or to press (S17.4).
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { words } from '../lib/words';

const components = readdirSync(__dirname)
  .filter((name) => /^(Start\w+|OpeningNotice|SavedPdfNotice|FirstPdf)\.svelte$/.test(name))
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
    expect(tooSmall(source)).toEqual([]);
  });
});

/** Each style rule of a component, as its selector and its declarations. */
function rulesOf(source: string): { selector: string; body: string }[] {
  const css = (/<style>([\s\S]*)<\/style>/.exec(source)?.[1] ?? '').replace(/\/\*[\s\S]*?\*\//g, '');
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((match) => ({ selector: match[1]!.trim().replace(/\s+/g, ' '), body: match[2]! }));
}

/** What a component's own rules make too small to read or to press. */
function tooSmall(source: string): string[] {
  return rulesOf(source).flatMap(({ selector, body }) => {
    const declared = [...body.matchAll(/([\w-]+):\s*([^;]+)/g)].map((match) => [match[1]!, match[2]!] as const);
    // Its label is what is pressed, and that has the 44 px (StartLook, and here below).
    const isTheRoundOfAChoice = selector === 'label input';
    return declared
      .filter(([property, value]) => {
        const px = /([\d.]+)px/.exec(value);
        if (property === 'font-size' || (property === 'font' && value.trim() !== 'inherit')) return !px || Number(px[1]) < 16;
        if (!/^(min-|max-)?height$/.test(property) || isTheRoundOfAChoice) return false;
        return px !== null && Number(px[1]) < 44;
      })
      .map(([property, value]) => `${selector} { ${property}: ${value} }`);
  });
}

it('finds what is too small, however it is written', () => {
  const style = (css: string) => `<style>${css}</style>`;
  for (const css of ['p { font-size: 13px; }', 'p { font: 13px/1.4 serif; }', 'p { font-size: 0.9em; }', 'button { height: 30px; }', 'button { max-height: 40px; }', 'button { min-height: 43px; }'])
    expect(tooSmall(style(css)), css).toHaveLength(1);
  expect(tooSmall(style('button { font: inherit; min-height: 44px; } p { font-size: 16px; line-height: 2px; }'))).toEqual([]);
});

/** Whether the rule with this selector makes it as high as a finger, and its text as large as it has to be. */
function isLargeEnough(source: string, selector: string): boolean {
  const bodies = rulesOf(source).filter((rule) => rule.selector.split(', ').includes(selector)).map((rule) => rule.body);
  const most = (property: string) => Math.max(0, ...bodies.flatMap((body) => [...body.matchAll(new RegExp(`(?:^|\\s)${property}: ([\\d.]+)px`, 'g'))].map((m) => Number(m[1]))));
  return most('min-height') >= 44 && most('font-size') >= 16;
}

// The start screen's is one rule for all of its pages; the others are drawn over the editor too, whose buttons
// are small, and have to say it themselves.
it.each([
  ['StartScreen', ':global(.start button)'],
  ['StartScreen', ':global(.start input)'],
  ['OpeningNotice', 'button'],
  ['SavedPdfNotice', 'button'],
  ['FirstPdf', '.again'],
])('what is pressed in %s is at least 44 px high, with text of 16 px: %s', (name, selector) => {
  const source = components.find((c) => c.name === `${name}.svelte`)!.source;
  expect(isLargeEnough(source, selector)).toBe(true);
  expect(isLargeEnough(source.replace(/min-height: \d+px;/g, ''), selector)).toBe(false);
});
it('a choice on Text and colours is pressed on its label, which is 44 px high', () => {
  expect(rulesOf(read('src/components/StartLook.svelte')).find((rule) => rule.selector === 'label')!.body).toMatch(/min-height: 44px/);
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

// ---- Where the focus goes when what was pressed is gone (S17.18) --------------------------------

it('what is pressed and then gone hands the focus on', () => {
  const home = read('src/components/StartHome.svelte');
  expect(home, 'Clear this list').toMatch(/isAskingToClear = true;\s*await tick\(\);\s*keepButton\?\.focus\(\)/);
  expect(home, 'No, keep it').toMatch(/isAskingToClear = false;\s*await tick\(\);\s*clearButton\?\.focus\(\)/);
  expect(home, 'the question is said with the answer that has the focus').toMatch(/bind:this=\{keepButton\} aria-describedby="start-clear-question"/);
  expect(home, 'the last one off the list, and the list with it').toMatch(/else moveFocus\('heading'\)/);
  expect(home, 'a row found missing').toMatch(/\$effect\.pre\([\s\S]*startScreen\.presence[\s\S]*querySelector\('button'\)\?\.focus\(\)/);
  expect(read('src/components/StartOpen.svelte'), 'Choose again…').toMatch(/folderButton\?\.focus\(\);\s*void chooseFolder\(\)/);
  expect(read('src/components/SavedPdfNotice.svelte'), '×').toContain('onclick={closeSavedPdf}');
  expect(read('src/components/FirstPdf.svelte'), 'Try again').toMatch(/closest<HTMLElement>\('\.pdf-column'\)\?\.focus\(\);\s*void triggerCompile\(\)/);
});

// ---- Said, shown, and not there (S17.18) --------------------------------------------------------

it('a search that finds nothing says so to a screen reader', () => {
  for (const [file, sentence] of [['StartHome', 'noRecentMatches'], ['StartNew', 'noTemplateMatches']]) {
    const region = new RegExp(`<div role="status">\\s*\\{#if [^}]*\\}\\s*<p>\\{words\\.${sentence}`);
    expect(read(`src/components/${file}.svelte`), file).toMatch(region);
  }
});

// Find it… on Home looks at a disk too, and the disk may be slow.
it('what is being looked at is said on whichever page it was asked on', () => {
  expect(startScreenSource).toContain('words.looking(startScreen.looking)');
  expect(read('src/components/StartOpen.svelte')).not.toContain('words.looking');
});

it('nothing of the document can be reached while the start screen is in front of it', () => {
  const covered = 'inert={app.launching || startScreen.visible || app.opening !== null}';
  expect(read('src/App.svelte')).toContain(covered);
  // In the window and not in the shell, and not on a backdrop either.
  expect(read('src/components/SessionCommitOffer.svelte')).toContain(covered);
});

// The system's own few colours (Windows): a tint, a shadow and a border's colour are all gone there.
it('what is chosen can be seen when the system chooses the colours', () => {
  const forced = '@media (forced-colors: active)';
  expect(declarations(startScreenSource, 'nav button[aria-current]', forced)).toMatch(/border: 3px solid Highlight/);
  const chosen = ".card.chosen,\n    .kind[aria-pressed='true']";
  expect(declarations(read('src/components/StartNew.svelte'), chosen, forced)).toMatch(/border: 3px solid Highlight/);
});

// ---- The word for it, in the panes a first document starts in (S17.18) --------------------------

/** As `sentencesIn`, but one-word strings too: "Compiling…" was the first thing the PDF pane said. */
function textsOf(path: string): string {
  const source = read(path);
  const script = /<script[^>]*>([\s\S]*?)<\/script>/.exec(source)?.[1] ?? (path.endsWith('.ts') ? source : '');
  const code = script.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '').replace(/\s\/\/ .*$/gm, '');
  const strings = [...code.matchAll(/"([^"\n]*)"|'([^'\n]*)'|`([^`]*)`/g)].map((m) => m[1] ?? m[2] ?? m[3] ?? '');
  // What is imported is not shown, nor is what is only a name.
  return [...strings.filter((text) => !/^[./\w-]*$/.test(text) || /^[A-Z]/.test(text)), path.endsWith('.ts') ? '' : ownText(source)].join('\n');
}

it('the PDF pane and the status bar make a PDF, and compile nothing', () => {
  const shown = ['components/PdfPane.svelte', 'components/FirstPdf.svelte', 'components/StatusBar.svelte', 'lib/announce.ts'].map((path) => textsOf(`src/${path}`));
  expect(shown[0]).toContain('The PDF will appear here'); // and so does not pass by reading nothing
  expect(shown[2]).toContain('Built in');
  expect(shown[3]).toContain('Could not build');
  expect(shown.join('\n')).not.toMatch(/compil/i);
});

it('a long name on Home is broken, not written past the edge of the page', () => {
  const home = read('src/components/StartHome.svelte');
  for (const selector of ['.title', '.about', '.row .muted']) expect(declarations(home, selector), selector).toMatch(/overflow-wrap: anywhere/);
});
