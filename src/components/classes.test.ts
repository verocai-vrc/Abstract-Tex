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

it('no class of ours is styled by pdf.js', () => {
  const ours = readdirSync(__dirname)
    .filter((file) => file.endsWith('.svelte'))
    .flatMap((file) => {
      const template = readFileSync(path.join(__dirname, file), 'utf8').replace(/<style>[\s\S]*<\/style>/, '');
      const classes = [...template.matchAll(/class="([^"]*)"/g)].flatMap((match) => match[1]!.split(/\s+/));
      const toggled = [...template.matchAll(/class:([\w-]+)/g)].map((match) => match[1]!);
      return [...classes, ...toggled].filter((name) => theirs.has(name)).map((name) => `${file}: ${name}`);
    });
  expect(ours).toEqual([]);
});
