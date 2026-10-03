#!/usr/bin/env node
// Renders each starter template's preview.png from a real build of it (S11.10a).
//
// Builds every template on the bundled engine (the ignored test in
// crates/abstract-tex-engine/tests/templates.rs, which also fails on any diagnostic), then turns
// page 1 of each PDF into templates/<id>/preview.png with `pdftoppm` (poppler-utils). The picture
// is never drawn by hand, so it cannot drift from what the template actually produces.
//
// Usage: node scripts/template-previews.mjs        (needs `pnpm fetch-engine` and pdftoppm)

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, renameSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  if (result.error || result.status !== 0) {
    console.error(`${command} ${args.join(' ')} failed${result.error ? `: ${result.error.message}` : ''}`);
    process.exit(1);
  }
}

run('cargo', ['test', '-p', 'abstract-tex-engine', '--test', 'templates', '--', '--ignored']);

const pdfFolder = join(root, 'target', 'template-pdfs');
for (const file of readdirSync(pdfFolder).filter((name) => name.endsWith('.pdf'))) {
  const id = file.slice(0, -'.pdf'.length);
  const folder = join(root, 'templates', id);
  if (!existsSync(folder)) continue; // a template that has since been removed
  // `-singlefile` writes exactly <prefix>.png, with no page number appended to the name.
  const prefix = join(folder, 'preview');
  run('pdftoppm', ['-png', '-f', '1', '-l', '1', '-r', '50', '-singlefile', join(pdfFolder, file), prefix]);
  console.log(`templates/${id}/preview.png`);
}
