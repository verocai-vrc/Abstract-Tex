// pdf.js's stylesheet is for a whole page of pdf.js's own, and it is loaded into ours. A class of ours that
// it styles by itself is drawn its way: the files pane was, in its colours, whatever the palette (S17.12).
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { expect, it } from 'vitest';

const sheet = readFileSync(path.resolve('node_modules/pdfjs-dist/web/pdf_viewer.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
/** Every class that is all there is to a selector, such as `.dialog{`. */
const theirs = new Set([...sheet.matchAll(/(?:^|[},;{])\s*\.([\w-]+)\s*(?=[,{])/g)].map((match) => match[1]!));

it('finds pdf.js’s classes, so this guard cannot pass by matching nothing', () => {
  expect([...theirs]).toEqual(expect.arrayContaining(['dialog', 'sidebar', 'textLayer']));
});

/** Every class a component's markup gives: written out, toggled, or chosen in an expression. */
function classesIn(source: string): string[] {
  const template = source.replace(/<style>[\s\S]*<\/style>/, '');
  const written = [...template.matchAll(/class="([^"]*)"/g)].map((match) => match[1]!);
  const toggled = [...template.matchAll(/class:([\w-]+)/g)].map((match) => match[1]!);
  // Every string in the expression, also one that is only compared with: reading too much does no harm.
  const chosen = [...template.matchAll(/class=\{((?:[^{}]|\{[^{}]*\})*)\}/g)].flatMap((match) =>
    [...match[1]!.matchAll(/'([^']*)'|"([^"]*)"|`([^`]*)`/g)].map((m) => (m[1] ?? m[2] ?? m[3])!.replace(/\$\{[^}]*\}/g, ' ')),
  );
  return [...written, ...toggled, ...chosen].flatMap((text) => text.split(/\s+/)).filter(Boolean);
}

it('reads a class however it is given', () => {
  const source = `<p class="a {b ? 'c' : ''}" class:d={e}></p><p class={f ? 'g' : "h"}></p><p class={\`i\${j}\`}></p><style>.k {}</style>`;
  expect(classesIn(source)).toEqual(expect.arrayContaining(['a', 'd', 'g', 'h', 'i']));
  expect(classesIn(source)).not.toContain('k');
});

it('no class of ours is styled by pdf.js', () => {
  const files = [path.resolve('src/App.svelte'), ...readdirSync(__dirname).filter((file) => file.endsWith('.svelte')).map((file) => path.join(__dirname, file))];
  const ours = files.flatMap((file) =>
    classesIn(readFileSync(file, 'utf8')).filter((name) => theirs.has(name)).map((name) => `${path.basename(file)}: ${name}`),
  );
  expect(ours).toEqual([]);
});
