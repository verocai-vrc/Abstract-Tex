import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri sets TAURI_ENV_* when it drives Vite; outside Tauri (plain `pnpm dev`, vitest) they are absent.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],

  // Relative asset URLs, not root-absolute ones. Vite's default base of '/' emits
  // `/assets/…`, which resolves against the page's origin — and the page's origin is
  // tauri://localhost in a packaged build while assets are looked up elsewhere, so a
  // root-absolute worker or chunk URL points at nothing. This is what kept the pdf.js
  // worker (src/lib/pdf/viewer.ts) from loading and left the PDF pane blank.
  base: './',

  // Tauri prints its own output; keep Vite from wiping the terminal.
  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: {
      // The Rust side rebuilds on its own; Vite must not react to it.
      ignored: ['**/src-tauri/**', '**/target/**', '**/crates/**'],
    },
  },

  // Only variables with these prefixes reach the frontend bundle.
  envPrefix: ['VITE_', 'TAURI_ENV_*'],

  optimizeDeps: {
    // The pdf.js worker is a bundle target, not a dependency to pre-bundle: Vite's dep
    // optimizer rewrites it into .vite/deps and then cannot find it, warning on every
    // start. `?worker&url` in src/lib/pdf/viewer.ts wants the original file.
    exclude: ['pdfjs-dist/build/pdf.worker.min.mjs'],
  },

  build: {
    // WebView2 on Windows is Chromium; WKWebView on macOS needs Safari targets.
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    // Vite 8 minifies with oxc by default; debug builds keep readable output.
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },

  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
