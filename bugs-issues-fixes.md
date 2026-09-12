# Bugs, issues, and fixes

A running ledger of every bug found during development, so nothing found is silently lost
between sessions or handed down to a release. An agent or the maintainer appends an entry
the moment a bug is found — before it is fixed, not after — and updates its status in place
once it is. Never delete an entry; a `Fixed` one is history, and a `Won't fix` one is a
decision worth keeping the reasoning for.

Newest entries at the top of their section.

## Status legend

`Open` — found, not yet fixed. `Fixed` — fixed and verified. `Won't fix` — deliberately
left, with the reason. `Wontfix` entries still need a reason a future reader will accept.

## Open

_(none as of 12 September 2026 — see Fixed below for everything closed this session)_

- **`PREAMBLE_OPEN` env var resolves relative to the wrong directory.**
  `src-tauri/src/commands.rs:42` filters `PREAMBLE_OPEN` through `Path::new(p).is_dir()`,
  which resolves a relative path against the Tauri process's working directory
  (`src-tauri/`), not the repository root. `fixtures/paper/SMOKE.md`'s documented invocation
  `PREAMBLE_OPEN=fixtures/paper pnpm tauri dev` therefore opens no project at all, silently.
  An absolute path works. Found: 12 Sep 2026. Not yet fixed — small, but it is in the script
  handed to a new contributor.

- **`cargo build --release` is not the release path.**
  `tauri.conf.json` sets `devUrl` unconditionally, so even a release binary loads
  `localhost:1420` and shows a "can't reach this page" error without Vite running.
  `pnpm tauri build` is the supported route (S2.9). Documentation debt more than a code bug;
  worth a README note before anyone tests a release binary the quick way.

- **No `rustfmt.toml`.** House style runs to ~110 columns; rustfmt defaults to 100. Nothing
  fails today because `pnpm verify` does not run `cargo fmt --check`, but the next
  `cargo fmt` invocation reformats every Rust file in the repo. One-line fix whenever
  someone owns the style decision.

## Fixed

- **`positions.test.ts` asserted the wrong clamp offset for an out-of-range line.** (S3.3a,
  12 Sep 2026) The test `clamps a line number past the end of the document to the last line`
  called `positionToOffset(doc, { line: 50, character: 0 })` on a one-line document and expected
  `doc.line(1).to` (the end of the line) — but `character: 0` clamps to the *start* of the
  clamped line, `doc.line(1).from`, which is what `positionToOffset` correctly returns. Caught
  immediately by `pnpm verify` (the assertion failed, not the code), so nothing shipped; logged
  because CLAUDE.md asks for every bug found during development, test bugs included. Fixed by
  correcting the expected value to `doc.line(1).from`.

- **PDF pane stayed blank after a fully successful build.** (12 Sep 2026)
  Root cause: `#[serde(tag = "status", rename_all = "camelCase")]` on `CompileEvent`
  (`src-tauri/src/compile.rs`) renames the enum's variant *tags* only, not the fields inside
  each variant. Rust sent `pdf_path`, `root_file`, `log_path`, `duration_ms`;
  `src/lib/ipc.ts` declared `pdfPath`, `rootFile`, `logPath`, `durationMs`. Every one of
  those fields reached the frontend as `undefined`. `app.pdfUrl` was never set,
  `PdfViewer.load()` was never called, and nothing failed — so nothing logged an error.
  Two earlier, more plausible-looking theories (the `?url` pdf.js worker import; then
  `new Worker(new URL(..., import.meta.url))`) were both real problems for the *packaged*
  build but were not what was blocking this one; only logging the raw event payload over
  the wire found the actual cause. Fix: add `rename_all_fields = "camelCase"` alongside
  `rename_all` on the enum (both attributes are required together). Applied to
  `CompileEvent` and, defensively, `LspEvent` (`src-tauri/src/lsp.rs` — no live bug there
  today since its fields are single words, but the same trap). Regression tests added in
  `compile.rs` and confirmed to fail when the fix is reverted:
  `finished_serialises_its_fields_in_camel_case`, `started_serialises_root_file_as_camel_case`.
  **Lesson:** 94 Rust tests and 93 Vitest tests passed throughout. Nothing asserted on the
  wire format — the one contract the two languages must agree on and neither side's test
  suite could see by itself.

- **PDF worker failed to load under `tauri dev` (root-absolute asset URLs).** (12 Sep 2026)
  `src/lib/pdf/viewer.ts` imported the pdf.js worker with `?url`, which Vite resolved to a
  root-absolute `/assets/…` path. The page's origin under `tauri dev` is
  `tauri://localhost`; Vite serves assets from `http://localhost:1420`; a cross-origin
  worker load is refused by the browser, with an empty `window.error`. Real fix required
  two changes together: `?worker&url` instead of `?url` in the import (so Vite bundles the
  file as a dedicated worker entry) and `base: './'` in `vite.config.ts` (Vite's default
  `base: '/'` emits root-absolute URLs for *every* asset, and a root-absolute first argument
  to `new URL(..., import.meta.url)` discards the base regardless of import style — verified
  by inspecting the emitted `dist/assets/index-*.js`, not by reading the source spelling).
  Also added `optimizeDeps.exclude` for the worker file, since Vite's dependency
  pre-bundler otherwise rewrites it into `.vite/deps` and then warns it cannot find it.

- **UI crashed on every launch: `effect_update_depth_exceeded`.** (12 Sep 2026)
  `src/components/Editor.svelte`'s editor-rebuild `$effect` assigned to `view`
  (`$state.raw`) both on creation and in its own teardown, while other effects in the same
  component read `view` reactively. An effect that writes state it also depends on re-runs
  itself without bound. Fixed by holding the CodeMirror instance the teardown needs in a
  plain (non-reactive) local, and only ever *writing* — never reading — the reactive `view`
  from inside that effect. 93 Vitest tests and `svelte-check` were green throughout; no test
  mounts `Editor.svelte`.

- **PDF load errors were unreachable once any build had ever succeeded.**
  (12 Sep 2026) `src/components/PdfPane.svelte` rendered `loadError` only inside
  `{#if !app.pdfUrl}`. Once `app.pdfUrl` held any value — even a stale one from a previous
  session — a subsequent load failure showed nothing at all, indistinguishable from "no
  build yet". Fixed by checking `loadError` first, independent of whether a URL exists.

- **Window rendered into roughly the top half of its frame.** (12 Sep 2026)
  `#app` (the Svelte mount point in `index.html`) had no height rule anywhere in `app.css`.
  `html, body` and `.app` were all `height: 100%`, but a percentage height resolves against
  the *parent's* height, and `#app` was `height: auto` — sized to its content, not the
  viewport. Fixed with `#app { height: 100%; }`.

- **`compile.rs` panicked on every build: "there is no reactor running".** (S3.2, 11 Sep 2026)
  Bare `tokio::spawn` was called from the synchronous `compile` Tauri command, which has no
  Tokio context. The verification method used for this file in S2.2/S2.7 (copy into a
  throwaway crate, swapping `tauri::async_runtime::spawn` for `tokio::spawn`) could not see
  this bug, because that swap is exactly what normalises the wrong call away. Fixed by using
  `tauri::async_runtime::spawn` throughout.

- **`bridge::path_to_uri` did not percent-encode paths.** (S3.2, 11 Sep 2026) A path
  containing a space (this repository's own path, `LaTeX Editor`) produced an invalid
  `file://` URI; TexLab rejected it and exited, which surfaced as a language server that
  crash-looped with no clear cause. All unit tests had used space-free paths. Fixed on both
  the Rust and TypeScript sides, with tests for spaces, `#`, `?`, `%`, and non-ASCII.

- **`LspSession::start` killed the language server on any failed handshake.** (S3.2,
  11 Sep 2026) The `?` on `initialize` returned early and dropped the only `Bridge`;
  dropping the last one tells the supervisor to kill the process — so a handshake that
  merely failed also took the server down, and the log named "restarted, then bridge
  dropped" instead of the real cause. Fixed by storing the bridge before the awaited call
  and making `stop()` explicit on the error path.

- **`initial_project` broke on folder paths containing spaces.** (S3.2, 11 Sep 2026)
  `preamble C:\My Thesis` arrives as two argv entries (`["C:\My", "Thesis"]`) unless quoted;
  `nth(1)` took the truncated half. Fixed by trying the joined tail first, then the first
  argument, with five tests including the exact case.

- **Sprint-1 WebView2/Kaspersky failure.** (Resolved between 9–11 Sep 2026, cause unconfirmed)
  `failed to create webview (0x80010108)` blocked every rung-4 smoke test through sprint 2.
  Stopped reproducing on 11 Sep with no code change on our side — likely a Kaspersky Endpoint
  Security policy update or a WebView2 runtime update. Left here because the underlying
  cause was never identified, only that it stopped: if it returns, this entry is the prior
  history.

## Won't fix

_(none yet)_
