#!/usr/bin/env node
// Downloads the Tectonic release for this machine into src-tauri/binaries/ (S1.3).
//
// Usage: node scripts/fetch-tectonic.mjs [--triple x86_64-pc-windows-msvc] [--force]

import { fetchSidecar } from './lib/sidecar.mjs';

const VERSION = '0.17.0';

fetchSidecar({
  name: 'tectonic',
  version: VERSION,
  // Linux uses the musl build: it is statically linked, so it runs on any glibc version a user has.
  assets: {
    'x86_64-pc-windows-msvc': `tectonic-${VERSION}-x86_64-pc-windows-msvc.zip`,
    'x86_64-apple-darwin': `tectonic-${VERSION}-x86_64-apple-darwin.tar.gz`,
    'aarch64-apple-darwin': `tectonic-${VERSION}-aarch64-apple-darwin.tar.gz`,
    'x86_64-unknown-linux-gnu': `tectonic-${VERSION}-x86_64-unknown-linux-musl.tar.gz`,
    'aarch64-unknown-linux-gnu': `tectonic-${VERSION}-aarch64-unknown-linux-musl.tar.gz`,
  },
  url: (asset) => `https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40${VERSION}/${asset}`,
}).catch((err) => {
  console.error(err);
  process.exit(1);
});
