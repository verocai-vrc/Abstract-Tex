#!/usr/bin/env node
// Downloads the TexLab language server for this machine into src-tauri/binaries/ (S3.1).
//
// Usage: node scripts/fetch-texlab.mjs [--triple x86_64-pc-windows-msvc] [--force]

import { fetchSidecar } from './lib/sidecar.mjs';

const VERSION = '5.26.0';

fetchSidecar({
  name: 'texlab',
  version: VERSION,
  assets: {
    'x86_64-pc-windows-msvc': 'texlab-x86_64-windows.zip',
    'aarch64-pc-windows-msvc': 'texlab-aarch64-windows.zip',
    'x86_64-apple-darwin': 'texlab-x86_64-macos.tar.gz',
    'aarch64-apple-darwin': 'texlab-aarch64-macos.tar.gz',
    'x86_64-unknown-linux-gnu': 'texlab-x86_64-linux.tar.gz',
    'aarch64-unknown-linux-gnu': 'texlab-aarch64-linux.tar.gz',
  },
  url: (asset) => `https://github.com/latex-lsp/texlab/releases/download/v${VERSION}/${asset}`,
}).catch((err) => {
  console.error(err);
  process.exit(1);
});
