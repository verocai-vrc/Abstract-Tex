#!/usr/bin/env node
// Downloads the Tectonic release for this machine into src-tauri/binaries/, named the way
// Tauri's `externalBin` expects: tectonic-<rust target triple>[.exe].
//
// Usage: node scripts/fetch-tectonic.mjs [--triple x86_64-pc-windows-msvc] [--force]
//
// Why a script and not a git-tracked binary: each build is ~30 MB and there are five of them.
// CI and developers run this once; the result is gitignored.

import { createWriteStream, existsSync, mkdirSync, renameSync, chmodSync, rmSync, readdirSync, statSync } from 'node:fs';
import { mkdtempSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';
import { execFileSync } from 'node:child_process';

const VERSION = '0.17.0';
const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT_DIR = join(REPO_ROOT, 'src-tauri', 'binaries');

// Maps a Rust target triple to the asset name Tectonic publishes for it.
// Linux uses the musl build: it is statically linked, so it runs on any glibc version a user has.
const ASSETS = {
  'x86_64-pc-windows-msvc': `tectonic-${VERSION}-x86_64-pc-windows-msvc.zip`,
  'x86_64-apple-darwin': `tectonic-${VERSION}-x86_64-apple-darwin.tar.gz`,
  'aarch64-apple-darwin': `tectonic-${VERSION}-aarch64-apple-darwin.tar.gz`,
  'x86_64-unknown-linux-gnu': `tectonic-${VERSION}-x86_64-unknown-linux-musl.tar.gz`,
  'aarch64-unknown-linux-gnu': `tectonic-${VERSION}-aarch64-unknown-linux-musl.tar.gz`,
};

function hostTriple() {
  // Prefer what rustc reports: Tauri names the sidecar after the *Rust* host triple.
  try {
    const out = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
    const m = out.match(/^host:\s*(\S+)/m);
    if (m) return m[1];
  } catch {
    /* rustc not on PATH; fall through to a guess */
  }
  const arch = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
  if (process.platform === 'win32') return `${arch}-pc-windows-msvc`;
  if (process.platform === 'darwin') return `${arch}-apple-darwin`;
  return `${arch}-unknown-linux-gnu`;
}

function argValue(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

async function main() {
  const triple = argValue('--triple') ?? hostTriple();
  const asset = ASSETS[triple];
  if (!asset) {
    console.error(`No Tectonic ${VERSION} build is published for ${triple}.`);
    process.exit(1);
  }

  const exe = triple.includes('windows') ? '.exe' : '';
  const dest = join(OUT_DIR, `tectonic-${triple}${exe}`);
  if (existsSync(dest) && !process.argv.includes('--force')) {
    console.log(`Already present: ${dest}`);
    return;
  }

  const url = `https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40${VERSION}/${asset}`;
  console.log(`Downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok || !res.body) {
    console.error(`Download failed: ${res.status} ${res.statusText}`);
    process.exit(1);
  }

  // Unpack *inside* the destination directory, not in the OS temp dir. The final step is a
  // rename, which is atomic — so a half-extracted binary can never appear at `dest` — but only
  // within one filesystem. On Linux /tmp is very often a separate tmpfs mount, and there the
  // rename fails with EXDEV instead. Staging alongside the destination keeps the atomicity.
  mkdirSync(OUT_DIR, { recursive: true });
  const work = mkdtempSync(join(OUT_DIR, '.download-'));
  const archive = join(work, asset);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(archive));

  // `tar` ships with Windows 10+, macOS and every Linux. On Windows we name the system one
  // explicitly: a Git Bash shell puts GNU tar first on PATH, and GNU tar cannot read zip files,
  // whereas the bsdtar in System32 can.
  const tar = process.platform === 'win32' ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar';
  execFileSync(tar, ['-xf', archive, '-C', work], { stdio: 'inherit' });

  const found = findFile(work, `tectonic${exe}`);
  if (!found) {
    console.error(`Archive did not contain tectonic${exe}`);
    process.exit(1);
  }

  renameSync(found, dest);
  if (!exe) chmodSync(dest, 0o755);
  rmSync(work, { recursive: true, force: true });

  const version = execFileSync(dest, ['--version'], { encoding: 'utf8' }).trim();
  console.log(`Installed ${version} → ${dest}`);
}

function findFile(dir, name) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) {
      const inner = findFile(p, name);
      if (inner) return inner;
    } else if (entry === name) {
      return p;
    }
  }
  return null;
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
