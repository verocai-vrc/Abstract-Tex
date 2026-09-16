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

- **Focus mode rebuilds every line's decoration on every keystroke and cursor move.**
  (S4.5, deferred reviewer pass, 16 Sep 2026) `src/lib/editor/focus.ts`'s `buildDecorations`
  iterates every line in the document on every `ViewUpdate` where the doc changed or the
  selection moved, to rebuild which lines are dimmed. Almost certainly under the <16 ms keystroke
  budget (DESIGN.md §2) at thesis-length documents — each line costs one `RangeSetBuilder.add` —
  but the same shape as two findings already logged here (`mergeMarkers` re-running per view
  update, S3.3b; `Project::info()` re-reading every file per refresh, S4.1), so worth the same
  "memoize if it ever measures otherwise" note rather than assuming it is fine forever. Not
  required; no sprint-4 card owns performance work.

- **`detect_root`'s "no other file includes it" exclusion has no fallback and ignores depth.**
  (S4.1, reviewer, 14 Sep 2026) `src-tauri/src/project.rs:220-233` removes every candidate that
  the preliminary include scan says another file includes, then picks the first survivor —
  nothing runs if the exclusion empties the list, and nothing prefers a shallower file among the
  ones that remain. Two scenarios: (1) two `\documentclass` files that each `\input` the other —
  both get excluded, `candidates` is empty, `detect_root` returns `None` where before this loop
  it would have returned one of them by the name tie-break; no root means nothing compiles at
  all. (2) `paper.tex` (the real root) plus `old/submission.tex` (a depth-2 draft) containing
  `\input{paper}` — `paper.tex` is now "included by another file" and gets excluded, so
  `old/submission.tex` wins even though it is shallower-vs-deeper reasoning would normally never
  pick it. The project-root-as-base-dir resolution deviation this loop's outcome recorded (see
  its own report) makes false-positive exclusions like this easier, since an argument that would
  not actually resolve to that target once the real root's directory is known still gets counted
  here. Needs: a fallback when exclusion empties `candidates` (fall back to the un-excluded list
  rather than `None`), and depth should still out-rank "not excluded" the way it does among
  never-excluded candidates today.

- **The include graph's depth cap drops nodes without recording anything.**
  (S4.1, reviewer, 14 Sep 2026) `crates/preamble-includes/src/graph.rs`'s `if depth >= MAX_DEPTH
  { continue; }` (the check right after popping the BFS queue) stops expanding a branch with no
  `Unresolved` entry and no other trace. A chain of 41 files nested one `\input` inside another
  yields 33 nodes (`MAX_DEPTH` is 32) and `is_complete() == true` — silently contradicting the
  module's own `//!` promise to never skip silently. Vanishingly unlikely in a real thesis (the
  comment at the `MAX_DEPTH` constant says as much), but the contract violation is real. Fix
  needs either a new `Unresolved` variant (`DepthLimitReached` or similar) pushed when the cap
  stops a branch, or the module doc's promise narrowed to say what it actually covers.

- **`build_graph`'s `root_relative` is not normalised before becoming the root node's path.**
  (S4.1, reviewer, 14 Sep 2026) `crates/preamble-includes/src/graph.rs:78-82` calls
  `to_forward_slashes(root_relative)` directly on whatever `Project::root_file()` returned. A
  hand-edited `preamble.toml` with `root = "./main.tex"` passes `Project::root_file`'s own
  `is_file()` check (`./main.tex` and `main.tex` name the same file on disk) and reaches
  `build_graph` unnormalised, producing the node path `./main.tex` — which the frontend's
  `documentFiles` list then carries verbatim. `shouldCompileFor('main.tex')` compares against
  that literal string and misses, so the root file's own external edits (a `git checkout`, a
  save from another editor) stop triggering a recompile. Fix belongs wherever paths first enter
  the graph — either `resolve_include_argument`'s `normalize_relative` applied to `root_relative`
  too, or `Project::root_file`/`set_root_file` normalising `./`-prefixed and similar spellings
  before they are ever stored.

- **Includes reached only through a `.sty`/`.cls`, `\InputIfFileExists`, or `\subimport` are
  invisible *and* leave `is_complete() == true`.** (S4.1, reviewer, 14 Sep 2026) Verified:
  `mystyle.sty` containing `\input{macros}`, pulled in via `\usepackage{mystyle}`, gives
  `nodes = ["main.tex"]`, `complete = true` — `macros.tex` never appears and nothing says the
  graph might be missing something. Two separate gaps produce the same symptom: (1) the graph
  never reads a `.sty`/`.cls` file's content even when one is reachable, so an `\input` inside a
  style file is never scanned at all (`\usepackage` is correctly out of scope per the card, but
  the file it names is never followed either, unlike a `\documentclass` chapter's own `\input`s);
  (2) `\InputIfFileExists` and `\subimport` are not in `scan.rs`'s `COMMANDS` list, so a directive
  using either one produces no `Directive` at all — not even `Unparsed` — meaning it is invisible
  to `unresolved` too, unlike `\import` (which the card named explicitly and this loop wired up
  to always report `Unparsed`). Fix for (2) is straightforward — add both names to `COMMANDS`
  with the same "always unparsed" treatment `\import` gets. Fix for (1) is a bigger design
  question (does the graph need to walk `.sty`/`.cls` files at all, given `\usepackage` itself is
  explicitly out of scope) that the architect should settle before anyone builds it.

- **`documentFiles.includes(relative)` is a case-exact string match.**
  (S4.1, reviewer, 14 Sep 2026) `src/lib/paths.ts:40`'s `shouldCompileFor` compares `relative`
  against the graph's `documentFiles` list with plain `===`-style `Array.includes`. The root
  cause is upstream, in how a node's `path` is built: `resolve_include_argument`
  (`crates/preamble-includes/src/graph.rs`) never canonicalises case, so `\input{Sections/Intro}`
  resolving against a real `sections/intro.tex` on a case-insensitive filesystem (Windows,
  default macOS) produces the node path `Sections/Intro.tex` — the graph correctly finds the file
  exists (`is_file()` is case-insensitive there too) but records the directive's own spelling, not
  the on-disk one. `shouldCompileFor('sections/intro.tex')` (the path the file watcher and tree
  actually use) then misses that entry and returns `false`, so external edits to that file stop
  recompiling. Fix likely belongs in `resolve_include_argument`: once a literal or `literal.tex`
  candidate is confirmed to exist, read the real on-disk casing back (e.g. via the directory
  listing `list_tree` already produces) rather than trusting the argument's spelling.

- **No test covers the S4.1 swap from `isTexSource` to `shouldCompileFor` in the controller.**
  (S4.1, reviewer, 14 Sep 2026) `src/lib/controller.svelte.ts:589`'s `handleFsEvent` now gates
  recompilation on `shouldCompileFor(relative, project)` instead of `isTexSource(relative)`, but
  every `controller.test.ts` scenario that reaches this line uses `main.tex`, which is (correctly)
  in the shared test fixture's `documentFiles`. Reverting the line back to `isTexSource(relative)`
  leaves all 225 vitest tests green — the behavioural difference this loop exists to add (a
  `.tex` file the graph knows is not included should *not* trigger a recompile) has unit coverage
  in `paths.test.ts` for the pure function, but no coverage at the controller/`handleFsEvent`
  integration level. Needs a `controller.test.ts` case with a second, non-included `.tex` tab or
  fixture file and an assertion that changing it does not call `compile`.

- **`Project::info()` re-reads every document file on every debounced tree refresh.**
  (S4.1, reviewer, 14 Sep 2026) `src-tauri/src/project.rs:169` calls `preamble_includes::
  build_graph`, which does a fresh `fs::read_to_string` of every node, on every `refresh_tree`
  Tauri command — fired on the 250 ms debounce in `controller.svelte.ts`'s
  `scheduleTreeRefresh` after *any* filesystem event, and `refresh_tree` is a synchronous command
  (blocks the main Tauri thread while it runs). Reviewer measured 128 ms on a synthetic
  200-chapter, 4 MB project — comfortably past the <16 ms keystroke budget if it ever runs on the
  UI thread during typing, though `refresh_tree` is not on that path today. The architect's own
  risk note on this loop's card already flagged this as "trivial at thesis size, worth a note for
  sprint 9" — this entry is that note, with a measurement attached.

- **`frames_keep_their_boundaries_under_load` deadlocks on Linux's 64 KB pipe buffer.**
  `crates/preamble-lsp/tests/process.rs` sends all 50 test frames (~125 KB total) before
  reading any reply back, and `Running::send`/`Running::recv`
  (`crates/preamble-lsp/src/server.rs:91-103`) are direct, unbuffered `write_all`/read calls
  on the child's piped stdin/stdout with no background pump task. Once the OS pipe buffer
  fills in either direction the write blocks; `fake_lsp_echo.rs`'s stand-in server uses
  blocking, synchronous `std::io` and echoes every frame the instant it reads one, so once
  its own stdout pipe back to us fills (we are still inside the `send` loop, not yet
  reading) its write blocks, it stops draining stdin, and our own `stdin.write_all().await`
  then never completes either — a classic bidirectional pipe deadlock. Reproduced by running
  `cargo test --workspace --exclude preamble` on a fresh Ubuntu 26.04 sandbox with no prior
  cargo cache: the test prints Rust's own "has been running for over 60 seconds" warning and
  never returns; killed manually after ~7 minutes. Not observed on the Windows sessions this
  project has run on so far (S3.2/S3.3d recorded this exact suite passing there), which
  suggests Windows's anonymous pipes tolerate more in-flight data before blocking — an
  environment difference, not a fix. Real fix is either read-while-writing in the test (pump
  `recv` concurrently with `send`, e.g. via `tokio::join!` or interleaving one send per read)
  or making `Running::send`/`recv` route through a buffering task the way the LSP bridge
  proper does — the test as written assumes an OS pipe with no meaningful capacity limit,
  which Linux does not give it. Found running the full workspace suite for the first time on
  Linux, 14 Sep 2026.

- **LSP diagnostic lookups can miss on a drive-letter casing mismatch.**
  `src/lib/lsp-diagnostics.ts` keys its map by the server's URI spelling on write
  (`publish`) but by our own (`pathToUri`) on read (`forPath`). If the folder picker returns
  `c:\Proj` (lowercase drive, which the Windows picker does produce) and TexLab round-trips
  through `Url::from_file_path` after canonicalizing, it publishes
  `file:///C:/Proj/main.tex` while `forPath` builds `file:///c:/Proj/main.tex` — the lookup
  misses, zero dots ever appear, and nothing errors. Latent, not observed: the reviewer
  confirmed `pathToUri` is case-preserving and that our percent-encoding matches the Rust
  `url` crate exactly, so this only bites when the two drive spellings actually differ. Fix
  is one line — normalize the URI (lowercase the drive letter) on both `publish` and
  `forPath`. Found by the reviewer, S3.3b, 13 Sep 2026. This is the third Windows-path-
  spelling bug in this subsystem, after the two logged in the S3.1/S3.2 outcomes above — an
  argument for the normalization living in one place rather than at each call site.

- **The gutter's `markers` callback re-merges on every view update.**
  `src/lib/editor/diagnostics.ts` runs `mergeMarkers` on every CodeMirror view update,
  allocating two Map/array/RangeSet triples per keystroke on the typing path (DESIGN.md §2
  commitment 2, <16 ms). Almost certainly under budget at realistic diagnostic counts, so
  not blocking — logged as the one place in this diff that allocates per key, for a future
  loop to memoize on the two field values (`markers`, `lspMarkers`) instead of the view
  update. Found by the reviewer, S3.3b, 13 Sep 2026.

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

- **`the_real_fixture_parses_and_both_searches_answer` fails on any checkout that is not the
  original author's Windows path.** (S4.6, builder, 16 Sep 2026) `crates/preamble-synctex/
  fixtures/multi.synctex.gz` was generated by a real engine run, so its `Input:1:` line is the
  absolute path of the machine that produced it: `C:\Users\arthur\Desktop\LaTeX Editor\
  Abstract Tex\Abstract-Tex\crates\preamble-synctex\fixtures\multi.tex`. The test
  (`crates/preamble-synctex/src/lib.rs:420`) asks `forward_search` for
  `CARGO_MANIFEST_DIR/fixtures/multi.tex`, which on this Linux box is `/home/arthur/ABSTRACT
  TEX/CODE/Abstract-Tex/...`; `paths_match` normalises case and separators but not the prefix,
  so `tag_for_file` finds nothing and the test panics with "line 3 should be on page 1". It
  passes only where the repo sits at that exact Windows path. Reproduced on clean HEAD
  (`85fdfca`) with S4.6's changes stashed, so it predates this loop; not caused by anything in
  S4.6, which touched no Rust. Fix candidates: have the test look the tag up by the
  fixture's own `Input:` path (the `files` table is already parsed) instead of by
  `CARGO_MANIFEST_DIR`, or compare by trailing components (`fixtures/multi.tex`) in the test
  only — `paths_match` itself should stay exact, since a real project can have two files with
  the same tail. Found running `cargo test --workspace --exclude preamble` for S4.6's gate.

- **Maths preview: a `$` inside a `%` comment shifts `$` pairing for the rest of the
  paragraph.** (S4.6, reviewer, 16 Sep 2026) `mathAtOffset` in
  `src/lib/editor/math-preview.ts` (the scan starting at line 97) counts every unescaped `$` in
  the paragraph, comments included. The card accepted "a `$` in a comment" as a false positive,
  but the effect is worse than one spurious popover: the stray `$` pairs with the *next* real
  opener, so every span after it in the paragraph is off by one. Scenario: paragraph
  `% price is $5\nThe value $x$ is` — hovering `x` (offset 26) returns null, and hovering
  "The value" previews `5\nThe value` as a formula. One-line fix, deferred out of S4.6 by the
  reviewer: before scanning, blank out each unescaped `%` to end of line with spaces (keeps
  offsets intact; `\%` is already stepped over by the scanner's escape rule). Two minor
  advisories from the same review, to take with it: (1) `renderMath` leaves KaTeX's default
  `strict: 'warn'`, which `console.warn`s on every non-ASCII glyph in the expression — pass
  `strict: 'ignore'`; (2) in `$a$$b$` an offset exactly equal to the first span's end previews
  `a`, not `b`, because the inclusive `<= spanEnd` test wins before the next opener is tried —
  cosmetic.

## Fixed

- **The Document map's section scanner had no word boundary after a sectioning command name, so
  `\partial` and `\paragraphindent` were misread as an empty-titled "Part"/"Paragraph" row.**
  (S4.2, deferred reviewer pass, 16 Sep 2026) `src/lib/outline.ts`'s `SECTION_RE` matched
  `\\(subsubsection|subsection|chapter|section|paragraph|part)(\*)?` — a LaTeX control word is the
  whole run of letters after the backslash, but the regex only checked a prefix, so `\partial`
  matched the `part` alternative with `ial` left dangling (and no `{` following, so the title came
  out empty) and `\paragraphindent` matched `paragraph` the same way. `\partial` in particular is
  common in any math-heavy document (`$\partial x/\partial t$`), so this would have spammed the
  Document map on a real paper. Fixed by adding a negative lookahead, `(?![a-zA-Z])`, between the
  command-name alternation and the optional `\*`. Regression test:
  `outline.test.ts`'s "does not mistake \partial or \paragraphindent for a sectioning command".

- **A macro-body `\input` with a parameter placeholder resolved as if `#1` were a literal path
  component.** (S4.1, reviewer, 14 Sep 2026) `crates/preamble-includes/src/scan.rs`'s
  `is_literal_argument` checked for a backslash-letter control sequence but not for `#`. Since
  the scanner reads raw source text rather than expanding macros, a definition like
  `\newcommand{\loadchapter}[1]{\input{chapters/#1}}` scans exactly as if `\input{chapters/#1}`
  had been written at top level; the argument `chapters/#1` has no backslash, so it was accepted
  as literal and resolved to the node `chapters/#1.tex` (`exists: false`), while the real
  `chapters/intro.tex` that a `\loadchapter{intro}` invocation actually meant never appeared in
  `documentFiles` — and `is_complete()` still reported `true`, so `shouldCompileFor` did not fall
  back to compiling every `.tex` either. Net effect: an external edit to any chapter loaded this
  way silently stopped triggering a recompile. Fixed by making `is_literal_argument` reject any
  argument containing `#`, which now makes such a directive `Unparsed` and correctly marks the
  graph incomplete. Regression tests: `scan::tests::a_macro_parameter_placeholder_is_unparsed`
  and `graph::tests::a_macro_bodys_input_with_a_parameter_placeholder_marks_the_graph_incomplete`.

- **Appending `.tex` to an argument with a dot in it replaced text instead of appending.**
  (S4.1, reviewer, 14 Sep 2026) `crates/preamble-includes/src/graph.rs`'s
  `resolve_include_argument` used `PathBuf::set_extension("tex")` to add the implicit `.tex`
  LaTeX itself assumes. Rust's `set_extension` *replaces* whatever follows the last `.` in the
  file name, which it treats as "the extension" regardless of whether it looks like one — so
  `\input{data.2024}` (with `data.2024.tex` really on disk) resolved to the node `data.tex`,
  `exists: false`, losing `2024` entirely; the real file never appeared in `documentFiles`; the
  graph still reported `is_complete() == true`. Fixed with a new `append_tex_extension` helper
  that builds the new file name as a string and reattaches it via `with_file_name`, appending
  rather than replacing. Regression test:
  `graph::tests::resolve_include_argument_appends_tex_without_swallowing_an_existing_dotted_suffix`.

- **`src-tauri/src/synctex.rs`'s own end-to-end test compared an unresolved `..` path
  against SyncTeX's fully-resolved one.** (S3.5, 13 Sep 2026) `Path::new(CARGO_MANIFEST_DIR)
  .join("../crates/preamble-synctex/fixtures")` keeps the literal `..` component; `preamble-
  synctex::paths_match` compares paths as normalised strings, which never resolves a `..`
  against a real path built without one — so the test's own path never matched the fixture's
  `Input:` line, which records the fully-resolved path Tectonic saw when it built the fixture.
  Fixed by wrapping the join in `std::path::absolute` (the same function `Project::open` in
  `project.rs` already uses, and for the same stated reason: it normalises without touching the
  filesystem, unlike `canonicalize`, which would additionally emit Windows's `\\?\` prefix).

- **`mergeMarkers` gave the LSP marker the exact-severity tie, not texlog.** (S3.3b,
  13 Sep 2026) The merge reused `build`'s displacement condition —
  `existing.severity === 'warning' && incoming.severity === 'error'` — for the second pass over
  the texlog set. That condition is right for two markers from *one* source, where either may
  displace the other only by being more severe, but it silently folds two different rules
  together when the sources differ: a texlog error arriving over a sitting LSP error is not
  "more severe", so it never displaced it, and the LSP row won the tie. The card, and
  DESIGN.md §5.2 behind it, require the opposite — the log parser's title is the *explained*
  sentence written by the rule catalog, so on an equal severity it is the one the author should
  see. Caught by the test written for exactly this rule, before any of it ran in the app. Fixed
  by separating the two rules in `mergeMarkers`: severity decides first, and source is the
  tie-break, with the texlog pass allowed to overwrite an equal-severity LSP marker.

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

- **`preamble-synctex` inverse search picked the enclosing box, not the specific line, on an
  exact-distance tie.** (S3.4, 13 Sep 2026) A box body (`(tag,line:h,v:...`) and the void
  marker (`h tag,line:h,v:...`) for its first line are frequently recorded at the exact same
  `(h, v)` in real SyncTeX output — confirmed against `fixtures/multi.synctex.gz`, a real
  Tectonic build, not a hand-written excerpt. `Iterator::min_by` keeps the *first* minimum it
  sees, which was the enclosing paragraph's record, not the more specific line the click was
  actually nearest to. Fixed by keeping the *last* record on an exact tie (`distance <=
  best_distance`), since SyncTeX writes records in outermost-first order, so a later record
  at the same point is the more specific one. Caught by a test built from the real fixture;
  a hand-typed fixture without this exact coincidence would not have found it.

- **`preamble-synctex` path comparison did not normalise path separators.** (S3.4, 13 Sep
  2026) `paths_match` compared `Path::to_string_lossy()` output case-insensitively, but not
  separator-insensitively: `PathBuf::from(dir).join("fixtures/multi.tex")` embeds a literal
  forward slash inside one component on Windows, which never gets rewritten to `\`, so it
  failed to match SyncTeX's own backslash-separated `Input:` line for the identical file.
  Fixed by replacing `\` with `/` on both sides before comparing. Found by the crate's own
  test against the real fixture — the exact "verification that edits the code cannot see"
  risk this project already tracks, in the opposite direction: here the *test's own path
  construction* was the fabricated part.

## Won't fix

- **TexLab's own README claims a `texlab.rootDirectory` setting that does not exist in the
  pinned 5.26.0 binary.** (S3.6, 13 Sep 2026) The card asked for "root file, build dir"
  passthrough; the README (`tug.ctan.org/support/texlab/README.md`) says "you may need to
  set the `texlab.rootDirectory` option for some multi-folder projects," which reads as
  exactly what the card wants. Checked against the actual deserialisation code at the pinned
  tag (`crates/texlab/src/server/options.rs`'s `Options` struct, fetched from
  `raw.githubusercontent.com/latex-lsp/texlab/v5.26.0/...`) rather than trusted from prose:
  no `root_directory` field exists anywhere on `Options` or `BuildOptions` at this version.
  Won't fix, because there is nothing to fix — TexLab genuinely has no such field to send at
  this version, so `texlab_settings` (`src-tauri/src/lsp.rs`) only passes the three build
  subdirectories. Documented in the S3.6 outcome and the function's own doc comment so a
  future reader who finds the same README passage does not repeat the same detour.
