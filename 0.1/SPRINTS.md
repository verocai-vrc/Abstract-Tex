# Preamble — Sprint Plan and Agent Development Loops

**Rev A · 9 September 2026 · Companion to [`DESIGN.md`](DESIGN.md)**

`DESIGN.md` says *what* and *why*. This file says *in which order, in what size pieces, and how
each piece proves itself*. It is written to be executed by an AI coding agent working with a
maintainer who is learning Rust through the project, so every unit of work is a **loop**: a
self-contained task with an explicit verification command and a done-when clause that a fresh
agent session can pick up cold.

Sprint numbers, not dates, are the unit of commitment (`DESIGN.md` §7).

---

## 1. The loop

Every piece of work, from a one-hour fix to a two-day subsystem, runs the same seven steps. The
agent follows them literally; the maintainer reviews the diff and the explanation at step 6.

| # | Step | What happens |
|---|---|---|
| 1 | **Orient** | Read `CLAUDE.md`, the loop card, and the `DESIGN.md` sections it cites. Re-read `DESIGN.md` §1.3 (non-goals) if the loop touches the UI. |
| 2 | **Pick** | Take the lowest-numbered loop in the current sprint whose dependencies are done. Never two loops in one branch. |
| 3 | **Build** | Implement in the files the card names. Rust is written for a reader who is learning it (see `CLAUDE.md`). Tests are written in the same loop, not a later one. |
| 4 | **Verify** | Run the card's verification command(s). Green means the loop's *own* tests pass **and** the whole suite still passes (`pnpm verify`). |
| 5 | **Review** | `/code-review` on the diff, or a self-review pass: does it violate any of the six commitments in `DESIGN.md` §2? Does it show a raw log? Does it write anything into the source tree that is not plain `.tex`? |
| 6 | **Record** | Tick the loop in §3 of this file. If a design decision changed, edit `DESIGN.md` first, then the code. Note anything a learner should read about in the commit body. |
| 7 | **Commit** | One commit per loop, message prefixed with the loop id: `S1.5: compile orchestrator with cancel-and-restart`. |

**Loop sizes.** S is under an hour of agent time and one file or test. M is a half-day and a
module. L is a day and a subsystem with its own tests. A loop that grows past L is split, not
finished.

**Verification ladder.** Every loop must clear the rung it names and every rung below it:

1. `cargo test --workspace` and `pnpm test` — unit and property tests, no GUI, no network.
2. `cargo clippy --workspace -- -D warnings` and `pnpm check` — types and lints.
3. Integration: `cargo test -p preamble-engine -- --ignored` runs the real Tectonic on a fixture
   (needs the sidecar fetched and a network on first run).
4. Smoke: `pnpm tauri dev` on `fixtures/paper`, following the manual script in
   `fixtures/paper/SMOKE.md`.
5. Exit demo: the sprint's exit criterion from `DESIGN.md` §7, performed on a real document.

`pnpm verify` runs rungs 1 and 2 on every platform. CI runs the same.

**Definition of done for a loop.** The card's done-when clause is true, verification is green,
no new `TODO` without a loop id next to it, and nothing in the diff violates `DESIGN.md` §2.

**Definition of done for a sprint.** Every loop ticked, the exit demo performed and its outcome
written in §3, and the plan for the next sprint re-read against §1.3.

### 1.1 Loop card format

Agents receive work as a card. Cards for sprints 1–4 are written out in full below; later
sprints carry titles and sizes and are expanded at the start of that sprint, because expanding
them now would only be guessing.

```
Loop      S1.5 · Compile orchestrator · M
Reads     DESIGN.md §3, §5.1 rung 1, §4.1 note 2
Depends   S1.2
Files     src-tauri/src/compile.rs, src-tauri/src/lib.rs
Build     One build in flight per project. A new request cancels the running one
          (kills the child process) and starts over. Emits compile:started,
          compile:finished, compile:failed as Tauri events.
Verify    cargo test -p preamble -- compile
Done when A fake engine that sleeps 5 s is cancelled within 100 ms of a second
          request, and the second request's outcome is the one emitted.
```

---

## 2. Milestone map

| Version | Sprints | Theme | Exit criterion (from `DESIGN.md` §7) |
|---|---|---|---|
| v0.1 | 1–2 | It compiles | Edit a real single-file paper, see the PDF update, on a machine with no TeX. |
| v0.2 | 3–4 | It navigates | Keyboard-drive a six-file thesis; click the PDF and land on the right line of the right file. |
| **v0.3** | **5–6** | **It explains itself** | 20-error torture document: every error → correct file, line, plain-language explanation. No raw log by default. |
| v0.4 | 7–8 | It cites | Assemble a 40-reference paper without a browser. |
| v0.5 | 9 | It's fast | p95 warm recompile < 1.2 s on a 60-page thesis, gated in CI. |
| v0.6 | 10–11 | It syncs | Write, sync, clone on a second machine, continue. Resolve a conflict without seeing `<<<<<<<`. |
| v0.7 | 12–13 | It assists | No fabricated citation reaches the buffer under adversarial prompting. |
| v0.8 | 14–15 | It shares, live | Two authors, one paragraph, one goes offline ten minutes and loses nothing. |
| v0.9 | 16 | It ships | A stranger installs on a clean machine and compiles their own paper. |

**The MVP is v0.1 plus the skeleton of v0.3.** A presentable demo needs the compile loop, a
PDF that updates as you type, and at least one error explained in a sentence. Sprint 2 therefore
borrows one loop from sprint 5 (S2.6, a five-rule catalog) so the first demo already shows the
differentiator rather than a raw log.

---

## 3. Sprints

Tick boxes are the record. An agent updates them in step 6 of the loop. `[~]` means partly done; the outcome paragraph says what remains.

### Sprint 1 — Skeleton that compiles

**Goal.** A Tauri window opens a folder, shows its files, edits `main.tex` in CodeMirror through
a Yjs document, saves on 700 ms idle, runs bundled Tectonic, and shows the PDF in pdf.js.
**Exit demo.** `pnpm tauri dev`, open `fixtures/paper`, change one word, PDF updates.

| ✓ | Loop | Size | Depends | Verify |
|---|---|---|---|---|
| [x] | S1.1 Repository scaffold: Cargo workspace, Tauri 2, Svelte 5, Vite, pnpm, CI matrix stub, licence, `CLAUDE.md` | M | — | `pnpm install && pnpm verify` |
| [x] | S1.2 `preamble-engine` crate: `Engine` trait, `BuildJob`/`BuildOutcome`, Tectonic subprocess with cancel-by-kill, binary discovery (env → sidecar → PATH) | L | S1.1 | `cargo test -p preamble-engine` |
| [x] | S1.3 `scripts/fetch-tectonic.mjs` downloads the 0.17.0 release for the host triple into `src-tauri/binaries/`; `externalBin` wired in `tauri.conf.json` | S | S1.1 | `pnpm fetch-engine && cargo test -p preamble-engine -- --ignored` |
| [x] | S1.4 Project model: open folder, file tree (ignoring `.git`, `.preamble`, build junk), root `.tex` detection, `preamble.toml` read/write | M | S1.1 | `cargo test -p preamble -- project` |
| [x] | S1.5 Compile orchestrator: one build in flight, cancel-and-restart, events | M | S1.2 | `cargo test -p preamble -- compile` |
| [x] | S1.6 File watcher: `notify` debounced, `.preamble/` ignored, self-write suppression by content hash | M | S1.4 | `cargo test -p preamble -- watcher` |
| [x] | S1.7 `preamble-reconcile` crate: text diff → CRDT ops in UTF-16 units, prefix/suffix trimming, `proptest` round-trip | M | S1.1 | `cargo test -p preamble-reconcile` |
| [x] | S1.8 Frontend shell: three panes, file tree, open-folder dialog, status bar, CSS custom-property theme with light/dark | M | S1.1 | `pnpm check && pnpm test` |
| [x] | S1.9 Editor: CodeMirror 6 + `stex` highlighting + `y-codemirror.next` binding + 700 ms debounced save through IPC | L | S1.8 | `pnpm test -- document` |
| [x] | S1.10 PDF pane: pdf.js over the asset protocol, reload on `compile:finished` preserving scroll, last good PDF stays on failure | M | S1.8 | manual, `fixtures/paper/SMOKE.md` §1 |
| [x] | S1.11 `texlog` crate stub: `quick_errors()` pulls `! …` lines and `l.NN` markers so sprint 1 can show *something* better than a log; fixture captured from a real failed build | S | S1.3 | `cargo test -p texlog` |

**Outcome (9 September 2026).** All eleven loops built and green on rungs 1–3: 35 Rust tests
(including the reconciler property tests and a real-Tectonic build of `fixtures/minimal`),
clippy clean, `svelte-check` clean, 11 Vitest tests, production Vite build. `fixtures/paper`
compiles with the bundled engine in 1.3 s warm. Rung 4 (the window) could **not** be
demonstrated on the maintainer's machine: WebView2's sandboxed child processes are killed at
launch for this unsigned host, so Tauri reports `failed to create webview (0x80010108)` before
our code runs. The same binary launched detached from the agent's process tree fails
identically, and Chromium's own log shows its network and GPU services dying on start; the
likely cause is Kaspersky Endpoint Security, which also blocks DNS inside the Tectonic process
(see `scripts/dev-proxy.py`). **Next action for the maintainer:** run `pnpm tauri dev` from a
normal terminal; if the window still fails, add an exception for `target\debug\preamble.exe` in
KES or run the smoke script on another machine. Loops S2.5 (paper fixture and `SMOKE.md`) and
S2.9 (README at the root with a *Building* section) were done early because sprint 1 needed
them; the sidecar path, atomic writes, self-echo suppression and cancel-and-restart all have
tests, so the exit demo is a matter of a working webview, not of missing code.

### Sprint 2 — v0.1 exit, and the first "oh"

**Goal.** External edits reconcile safely, compile feedback is honest, the app is presentable,
and the first five error rules exist so the demo never shows a raw log.
**Exit demo.** `DESIGN.md` §7 v0.1 exit, on this Windows machine which has no TeX installed.

| ✓ | Loop | Size | Depends | Verify |
|---|---|---|---|---|
| [~] | S2.1 External change → diff → CRDT transaction end to end; dirty-buffer conflict bar (*Keep mine* / *Load from disk*), never an automatic merge | M | S1.6, S1.7, S1.9 | vitest for the decision table; smoke §2 |
| [~] | S2.2 Compile feedback: spinner and elapsed time in the status bar, Tectonic package-fetch progress surfaced from stderr, failure count in the drawer, raw log behind one click | M | S1.5, S1.11 | smoke §3 |
| [~] | S2.3 Multi-document editing: open any `.tex`/`.bib` from the tree, tabs, per-file Y.Doc, save-all on compile | M | S1.9 | `pnpm test -- documents` |
| [~] | S2.4 Keyboard: `Ctrl S` save, `Ctrl B`/`F5` compile, `Ctrl O` open folder, `Ctrl P` quick-open file (seed of the palette) | S | S2.3 | smoke §4 |
| [~] | S2.5 Fixtures: `fixtures/paper` (a real 8-page article), `fixtures/broken` (underscore, undefined control sequence, missing brace), `fixtures/paper/SMOKE.md` manual script | S | — | `cargo test -p preamble-engine -- --ignored` |
| [x] | S2.6 First five diagnostic rules in `texlog`: undefined control sequence, missing `$`, missing `}`/runaway argument, undefined reference/citation, file not found — each with a sentence and a fixture (brought forward from sprint 5 so the demo is honest) | M | S1.11 | `cargo test -p texlog` |
| [~] | S2.7 Diagnostics drawer v0: sentence per error, click jumps to line, gutter marker; raw log one click away | M | S2.6 | smoke §5 |
| [~] | S2.8 CI: 3-OS matrix runs `pnpm verify`; Linux installs WebKitGTK deps; engine fetched in CI; `cargo build` of the Tauri app on all three | M | S1.1 | green Actions run |
| [~] | S2.9 Name decision recorded in `DESIGN.md` §10; README moved to repository root with a real *Building* section; installer smoke on Windows via `pnpm tauri build` | S | — | installer launches and opens `fixtures/paper` |

**Outcome (11 September 2026, interim).** Every loop's code is written and green on rungs 1–3
where this machine can run them: 38 Rust tests across the three library crates plus the four
`compile.rs` tests in a Tauri-free copy, clippy clean, `svelte-check` clean, 65 Vitest tests, a
production Vite build, and a real Tectonic build of `fixtures/minimal`. What keeps the sprint at
`[~]` is not code: rung 4 (every smoke section) and the v0.1 exit demo need the webview that
the maintainer's machine could not open in sprint 1 and this Linux machine has no display for;
S2.8 needs a push to see Actions go green; and S2.9's name decision is the maintainer's to make
(`DESIGN.md` §10 still lists *Preamble* as a working name with three alternatives). S2.5 stays
`[~]` for one honest reason: `fixtures/paper` is a real article that compiles with zero
warnings, but it is 4 pages, not the card's 8 — worth lengthening when S9.1 builds the benchmark
corpus and page count starts to matter, not before. **Next action for the maintainer:** run
`pnpm tauri dev` on a machine with a working webview and walk `fixtures/paper/SMOKE.md` §1–§6;
push to GitHub and read the first Actions run; pick a name.

**S2.1 (10 September 2026).** `[~]` because rungs 1–2 are green but rung 4 (smoke §2) still
waits on the webview problem recorded under sprint 1. The decision table and the debounce hold
in `document.ts` were already written; this loop wired them to the controller, which was still
passing a bare `boolean` to `decideExternalChange` and so did not typecheck. Three things a
reader should take from the diff:

1. **The debounce is the merge hazard.** Reading the disk and diffing are both `await`s. If the
   700 ms save timer fires inside that window, or while the conflict bar is on screen, it writes
   the buffer over the file and answers the author's question for them — an automatic merge by
   accident. `reconcileOpenDocument` holds saves across the whole reaction and `resolveConflict`
   releases them. `controller.test.ts` fails if the hold is removed; that was checked.
2. **A deleted file is not an empty one.** `lastSavedText` became `string | null` so that
   `forgetDiskState()` can mean "nothing on disk holds this text". Without it `save()` would
   short-circuit on `snapshot === lastSavedText` and Ctrl S on a vanished file would silently do
   nothing.
3. **Switching files is refused while a bar is up**, rather than flushing a buffer whose fate is
   still an open question. Proper multi-file handling is S2.3.

**S2.2 (10 September 2026).** `[~]` for the same reason as S2.1: rungs 1–2 are green, rung 4
(smoke §3) waits on the webview problem from sprint 1. Three of the card's four items — the
status-bar spinner, the drawer's failure count, and the one-click raw log — already existed from
S1.9/S1.10 wiring; the gap was package-fetch progress, which this loop closes.

1. **Streaming replaces `read_to_end`.** `Tectonic::build` used to buffer the whole of stderr
   and hand it back only after the process exited — correct for the "raw output" view, useless
   for showing anything *while* a cold Tectonic run is fetching packages. `pump_stderr` in
   `crates/preamble-engine/src/tectonic.rs` now reads byte chunks and splits lines itself rather
   than using `AsyncBufReadExt::lines()`, because `lines()` requires valid UTF-8 and stops dead,
   silently, on the first byte that isn't — a real risk in engine stderr. Each line is sent on an
   `Option<ProgressSink>` (a plain `mpsc::UnboundedSender<String>` type alias) the moment it
   arrives, and still collected into the same string `BuildOutcome::stderr` carried before.
2. **A second channel, and an ordering bug caught before it shipped.** `Orchestrator::request`
   forwards progress lines to the frontend through their own `mpsc` channel and task, kept
   separate from the task that awaits the build and emits `Finished`/`Failed`, because the engine
   crate must not know it is talking to Tauri. Two independent tasks calling the same `on_event`
   have no ordering relative to each other by default — an early version of this loop let
   `Finished` reach the frontend before the `Progress` lines that preceded it, only visible as a
   flaky test. The fix is to `.await` the forwarder task's `JoinHandle` inside the build task
   before emitting `Finished`; that is safe because the engine has already dropped its sender by
   the time `build()` returns, so the forwarder is guaranteed to drain and end once polled.
3. **Verified without a webview.** `cargo test -p preamble` cannot build here (no `pkg-config`,
   no root — same limitation the sprint-1 outcome recorded), so `compile.rs`'s real behaviour —
   including the ordering fix above — was checked by copying it, unmodified but for swapping
   `tauri::async_runtime::spawn` for `tokio::spawn`, into a throwaway crate that path-depends on
   the real `preamble-engine` and `texlog`. All four tests and `cargo clippy` passed there before
   the file was trusted. `cargo test -p preamble -- compile` still needs to run on a machine that
   can link Tauri, to be sure that swap was the only thing standing between the two.

**S2.3 (10 September 2026).** `[~]`: rungs 1–2 are green (`pnpm test -- documents` and the rest
of the suite, `svelte-check`), rung 4 (smoke §6, added to `fixtures/paper/SMOKE.md`) still needs
the webview this machine cannot open. Purely a frontend loop — no Rust changed.

1. **A new module, not a bigger `document.ts`.** `src/lib/documents.ts` adds `DocumentManager`:
   a plain, Tauri- and Svelte-free class (same shape as `OpenDocument` itself) that owns a
   `Map<path, OpenDocument>` and the tab order, so opening, closing, and save-all are each one
   testable method rather than logic folded into the controller. `state.svelte.ts` holds a
   reactive *snapshot* of it (`docs`, `openTabs`, `dirtyPaths`); `controller.svelte.ts`'s new
   `syncTabs()` is the one place that copies from the manager into that snapshot, matching the
   file's own rule that only the controller writes to `app`.
2. **`activeDoc` and `dirty` became derived, not assigned.** With several tabs open, "the active
   document" is just "whichever path `activePath` names, looked up in `docs`" — a `$derived`,
   the same pattern the file already used for `projectName`. This deleted more code than it
   added: `openFile` no longer needs to flush-then-recreate a document on every switch, because
   switching tabs no longer disposes anything. That was the actual point of "per-file Y.Doc" —
   not just "don't lose the buffer while the tab is in front," which the CRDT already gave for
   free, but "don't lose it while the tab is in *back*," which required keeping the instance
   alive at all.
3. **Save-all surfaced a real race, not just a naming decision.** The obvious implementation —
   `triggerCompile` awaits `manager.saveAll()`, which calls each dirty tab's ordinary `save()` —
   quadrupled the compile count in a two-tab test instead of leaving it at one. `save()` already
   calls `backend.afterSave`, which is wired to `triggerCompile`; `saveAll` calling it too meant
   every tab's save queued *another* `triggerCompile`, which called `saveAll` again while the
   first was still running, and a tab whose write had not yet landed could be told to save twice.
   Fixed by giving `OpenDocument.save()` a `notify` parameter (default `true`); `saveAll` passes
   `false`, since its caller is about to compile once anyway. `document.test.ts` gained a case
   for `save(false)` directly, not only the effect of it three layers up.
4. **A conflict on a background tab now raises that tab.** `reconcileOpenDocument` used to
   require the changed file to be the *active* document; any other open file's external changes
   went unreconciled entirely (harmless in sprint 1, since only one file was ever open). It now
   looks the path up in the manager regardless of which tab has focus, and if the decision is
   `conflict`, switches to that tab first — asking a question about a file nobody can see would
   not be asking much.

**S2.4 (11 September 2026).** `[~]`: rungs 1–2 green, smoke §4 waits on the webview. Frontend
only. Three things a reader should take from the diff:

1. **One dispatcher, one table.** `Ctrl B` was bound twice — in CodeMirror's keymap and in the
   window handler — and a CodeMirror binding that handles a key calls `preventDefault` but does
   *not* stop propagation, so one keypress inside the editor compiled twice. `Ctrl S` and `F5`,
   meanwhile, only worked when the editor had focus. Now `src/lib/shortcuts.ts` is the table
   (chord → action name, with a label the S4.3 palette will list) and `App.svelte`'s window
   handler is the only code that turns a key into an action; the CodeMirror bindings are gone.
   `shortcutFor` is pure and tested, including that it does not steal `Ctrl Shift S` or `Alt F5`.
2. **`Ctrl P` is the seed of the palette, and it is mostly two pure functions.** `fuzzy.ts` is a
   greedy subsequence matcher with three bonuses (basename, word start, consecutive run) and a
   length tie-breaker — enough that `res` finds `sections/results.tex` above
   `resources/figure.tex` and `main` puts `main.tex` above `main-old.tex` above
   `domain-notes.tex`, all asserted. `paths.ts` grew `listFiles`. `QuickOpen.svelte` is the thin
   part: an input, a ranked `<ul>`, arrows/Enter/Escape, and `aria-activedescendant` so the rows
   need no focus of their own. When nothing is typed, open tabs come first.
3. **`EditorCallbacks` is deleted, not deprecated.** `createEditor(host, doc)` no longer takes
   save/compile callbacks because CodeMirror no longer has any reason to know those actions
   exist. Less plumbing than before the loop, which is the shape a keyboard loop should have.

**S2.7 (11 September 2026).** `[~]`: rungs 1–2 green, smoke §5 (rewritten for what the drawer
now does) waits on the webview. The loop that connects S2.6 to the screen.

1. **Rust: one line changed in meaning, ten in text.** `CompileEvent::Finished.errors:
   Vec<QuickError>` became `diagnostics: Vec<texlog::Diagnostic>` and `read_quick_errors`
   became `read_diagnostics`. Verified the way S2.2 was: `compile.rs` copied unmodified but for
   `tauri::async_runtime::spawn` → `tokio::spawn` into a throwaway crate on the real
   `preamble-engine` and `texlog`; 4 tests and clippy green there. `cargo test -p preamble`
   still needs a machine that can link Tauri.
2. **The drawer renders sentences and nothing else.** `Drawer.svelte` shows title +
   explanation per card, errors before warnings, a dashed border when no rule matched (the
   explanation then quotes TeX, but says so), and *Raw log* is still one click. `errorCount`
   and `warningCount` are `$derived` on the state so the drawer's summary and the status bar
   phrase themselves from the same two numbers: a failed build says `1 error`; a clean build
   with undefined citations says `Built in 1.3s · 1 warning` and does **not** open the drawer,
   because the PDF was produced and shouting would be wrong (DESIGN.md §6).
3. **Gutter dots, and where they are allowed to be.** `src/lib/editor/diagnostics.ts` is a
   `StateField` of `GutterMarker`s replaced by a `StateEffect` — CodeMirror's own lint-gutter
   pattern, spelled out with comments for a first reader — that maps its ranges through edits
   so a dot follows its line as the author types above it. `build()` is exported and tested for
   the three things that matter: one dot per line, error beats warning on the same line, a line
   TeX claims past the end of the file is clamped onto the last line. A diagnostic has a line
   but no *file* until S5.2, so the dots are drawn only on the root file's tab, and
   `jumpToDiagnostic` brings that tab to the front before moving the cursor — the best
   available guess, stated as one in the code rather than hidden.

**S2.8 (11 September 2026).** `[~]`: the workflow is written and its YAML parses; "green
Actions run" needs a push to GitHub, which this session cannot do. There was no `.github/`
directory at all — the S1.1 card's "CI matrix stub" had not been created. What the workflow
does, and why in that order: Linux installs the WebKitGTK/GTK development packages (the other
two runners ship their webview with the OS); `pnpm fetch-engine` runs before any `cargo`
command because `tauri_build` fails the build if the sidecar for the host triple is missing;
`pnpm build` runs before `pnpm verify` because `tauri::generate_context!` embeds `dist/` and
fails if it does not exist — which means `cargo test --workspace` on a fresh clone fails
without it, a trap README.md now names. Then rungs 1–2 (`pnpm verify`), rung 3 (the real-engine
fixture build, with the engine's package cache kept between runs via `TECTONIC_CACHE_DIR`), and
`cargo build -p preamble`. `fail-fast: false` so a Windows-only failure still shows the other
two results. First-run things to expect when it is pushed: the macOS runner is arm64, so it
exercises the `aarch64-apple-darwin` sidecar path nobody has run yet.

**S2.6 (10 September 2026).** `crates/texlog/src/rules.rs`: a `Rule` is a matcher `fn` plus an
explanation `fn`, and `CATALOG` is a `const` slice of them, so S5.5 generalises this rather than
replacing it. `diagnostics(log)` is now the entry point the app should call; `quick_errors` stays
underneath it as the raw scan. Six rule ids cover the card's five errors (undefined reference and
undefined citation are separate ids because the advice differs). 23 tests, clippy clean.

**The finding worth keeping: hand-written log excerpts test the matcher against our idea of TeX,
not against TeX.** Five fixtures were captured from real Tectonic 0.17.0 runs, and three of the
five rules were wrong until they existed:

- `Runaway argument?` is printed with **no leading `!`**, so a scanner keyed on `!` never sees
  it. The message that does arrive is `! File ended while scanning use of \emph .`, and it
  carries **no `l.NN` marker at all** — so that rule names the command instead of a line.
- Tectonic **doubles the bang** on package errors: `! ! LaTeX Error: File \`x.sty' not found..`
- Undefined `\ref`/`\cite` are **warnings, not errors**, with the line number in prose
  (`on input line 4`). `quick_errors` only reads `!` lines, so `rules.rs` carries a deliberately
  narrow second scan for those two forms; the general warning tokenizer is still S5.1.

Deliberately not done here: the paren-stack resolver, so a diagnostic still has a line but no
*file* (S5.2 — the real engineering problem); one-click fixes (S6.2); and wiring `diagnostics()`
through `compile.rs` to the drawer, which is S2.7.

**Environment notes from this session (Linux, not the maintainer's Windows box).**

- `pnpm fetch-engine` was **broken on any machine where `/tmp` is a separate mount** — the
  script unpacked into the OS temp dir and `renameSync`d into the repo, which fails with
  `EXDEV`. Fixed by staging inside `src-tauri/binaries/` so the rename stays on one filesystem
  and keeps the atomicity that was the point of using a rename. This would have hit S2.8 (CI).
- **The repository had no `.gitignore` at all.** Added one; without it the first commit would
  have swallowed `target/`, `node_modules/` and the 26 MB sidecar. The parser fixtures are
  explicitly re-included, since `*.log` is otherwise build junk.
- `cargo test -p preamble` (the Tauri app crate) **cannot build on a machine without the
  WebKitGTK/GTK development packages** and `pkg-config`, which need root. The three library
  crates build and test fine, which is exactly the separation SPRINTS.md §4 chose them for.
- `cargo fmt --check` disagrees with the existing house style (S1 code runs to ~110 columns,
  rustfmt defaults to 100) and there is **no `rustfmt.toml`**. `pnpm verify` does not run fmt,
  so nothing is failing — but the next contributor to run `cargo fmt` will reformat every file.
  A `rustfmt.toml` pinning the intended width is a one-line loop for whoever owns the style.

### Sprint 3 — Language server and SyncTeX

**Exit demo.** Completion for `\ref`/`\cite`, hover, go-to-definition on a two-file project;
`Ctrl click` in the PDF lands on the line, cursor move highlights the PDF.

| ✓ | Loop | Size | Depends |
|---|---|---|---|
| [x] | S3.1 TexLab sidecar fetch script and `externalBin`; Rust owns process lifecycle and stdio (`DESIGN.md` §4.1 note 3) | M | S1.3 |
| [x] | S3.2 JSON-RPC bridge: Rust ↔ TexLab over stdio, Tauri events ↔ TypeScript; request/response correlation, restart on crash | L | S3.1 |
| [x] | S3.3a Protocol types and the completion source: typed `lsp.ts`, `textDocument/completion` as a CodeMirror `autocompletion()` source | M | S3.2 |
| [x] | S3.3b Diagnostics merge: `publishDiagnostics` into the editor's problem markers | M | S3.3a |
| [x] | S3.3c Hover, go-to-definition, document symbols | M | S3.3a |
| [x] | S3.3d Inline squiggles and diagnostic hover: `Decoration.mark` over LSP ranges | M | S3.3b, S3.3c |
| [x] | S3.4 SyncTeX forward: cursor → PDF highlight, parsed from `.synctex.gz` in Rust | M | S1.10 |
| [x] | S3.5 SyncTeX inverse: click in PDF → `file:line`, opening the file if needed | M | S3.4, S2.3 |
| [x] | S3.6 LSP settings passthrough from `preamble.toml` (root file, build dir) | S | S3.2 |

**S3.1 (11 September 2026).** `[x]`. The crate, the fetch script and the `externalBin` entry
were already written; what closed the loop was running its `--ignored` test against the real
TexLab 5.26.0 for the first time, on Windows, which failed twice and for two real reasons:

1. **`format!("file://{path}")` is not a URI on Windows.** A temp dir is `C:\Users\…\.tmpXYZ`,
   so that format string produces `file://C:\Users\…` — backslashes kept, and two slashes where
   a drive letter needs three. TexLab parsed `C:` as the *host*, rejected the request and exited;
   the test saw only `Err(Exited)` with no explanation. The conversion now lives in exactly one
   place, `bridge::path_to_uri`, and the test calls it.
2. **`initialized` is not optional.** The test sent `initialize`, then `shutdown`. TexLab 5.26
   enforces the spec's ordering and answers that with `expected initialized notification, got:
   shutdown`, then exits — so the assertion that failed was the *shutdown* reply, three lines
   below the real mistake. `Bridge::initialize` sends the notification for real callers; the
   process-level test now sends it by hand.

Worth keeping: `Error: disconnected channel` on TexLab's stderr is what it prints when stdin
closes, **not** a failure. It appears on a perfectly good one-shot `printf | texlab` probe, and
cost a detour before that was clear.

**S3.2 (11 September 2026).** `[x]`: both halves of the card are built and green on rungs 1–3.
`crates/preamble-lsp/src/bridge.rs` is the Rust half — correlation and supervision, depending on
neither Tauri nor the editor, so it tests with no window, the same separation §4 chose the library
crates for. `src-tauri/src/lsp.rs` plus four commands are the seam to Tauri, and `src/lib/lsp.ts`
is the frontend client. 90 Vitest tests (25 new) and 27 in `preamble-lsp`. What a reader should
take from the diff:

1. **Correlation is a table of parked `oneshot` senders, and the id goes in *before* the send.**
   `request` allocates an id, inserts its half of a `oneshot` into `Arc<Mutex<HashMap<i64, …>>>`,
   and only then writes the frame — because a fast server can reply before `send` returns, and an
   entry inserted afterwards would race a response that has already arrived. The test that earns
   the module is `answers_go_to_the_right_caller_when_they_arrive_out_of_order`: a `slow` request
   answered after a `quick` one sent later. Anything matching answers by arrival order swaps them.
2. **One `select!` loop owns the process, not two tasks.** Writing and reading could each be their
   own task, but a restart swaps *both* pipes at once, and a writer holding the old stdin would
   write into a dead process. One loop means one place where `running` is replaced. On a crash the
   supervisor clears the pending table first — dropping every `oneshot` sender wakes its receiver
   with `Dropped` — so a caller waiting on the dead server gets an error rather than hanging
   forever. `MAX_RESTARTS` is 5: a server dying repeatedly is broken in a way respawning will not
   fix, and an unbounded loop would burn a core instead of surfacing the problem.
3. **The bridge never interprets a method.** `textDocument/completion` is a string here. Turning
   protocol into CodeMirror behaviour is S3.3's job, in TypeScript (DESIGN.md §4.1).
4. **Version numbers are the frontend's whole job, and the restart is why.** `src/lib/lsp.ts`
   tracks which files the server believes are open and what version each is on, because LSP
   requires every `didChange` to count strictly upward per file. The case that makes it worth a
   module: when the bridge restarts a crashed server, the new process has never heard of any of
   our documents — continuing from version 4 leaves the two permanently out of step, and
   completion then answers from text nobody is looking at. `resync()` re-opens every document at
   version 1 with its *latest* text, and `controller.svelte.ts` calls it on the `restarted` event.
   Tested both in isolation and through the controller with a recording transport.
5. **No TexLab is a degraded mode, not an error.** `lsp_start` failing sets a muted status-bar
   label and nothing else: no notice, no dialog. A test opens the project with the server missing
   and asserts the editor still opens the file, saves on the 700 ms debounce, and compiles —
   DESIGN.md §2 commitment 6 is the kind of rule that only stays true if something checks it.
   `didChange` rides the existing save debounce rather than every keystroke, so commitment 2
   (<16 ms keystrokes) is unaffected.

**Rung 3 reaches the feature now, not just the handshake.**
`the_real_texlab_completes_an_environment_name` runs the sequence the frontend sends —
`initialize`, `didOpen`, `completion` inside a `\begin{` — against the real TexLab 5.26.0 and
asserts `itemize` comes back. A break in the protocol path fails a test rather than waiting for
someone to notice completion is quiet. `cargo test -p preamble-lsp -- --ignored` runs it.

Deliberately not done here: rendering any of this. Completion lists, hover cards, go-to-definition
and `publishDiagnostics` on screen are S3.3, which is why `handleLspEvent` ignores notifications
today. The plumbing is what this loop owed.

**The machine question from sprints 1 and 2 is half answered.** `cargo test -p preamble` — the
Tauri app crate — **builds and passes here**: 18 tests, including the four in `compile.rs` that
S2.2 and S2.7 could only verify by copying the file into a throwaway crate. The cancel-and-restart
test and S2.2's event-ordering fix pass in place, so that `tokio::spawn`-for-`tauri::async_runtime
::spawn` swap was indeed the only thing between the two, as those outcomes hoped. The whole
workspace is clippy-clean with `-D warnings`. What this does **not** answer is the webview: no
`pnpm tauri dev` was attempted, so every smoke section and the v0.1 exit demo still wait.

Two things fixed in passing, both pre-existing: `crates/preamble-engine/src/tectonic.rs` imported
`warn` without using it, which fails `cargo clippy -- -D warnings` and so had been breaking
`pnpm verify` at rung 2; and `src-tauri/binaries/` was empty on this machine, which fails
`tauri_build` before any Rust compiles — `pnpm fetch-sidecars` fixes it, and both sidecars
(Tectonic 0.17.0, TexLab 5.26.0) are now present here.

**The webview opens. (11 September 2026 — the finding that closes sprint 1's open question.)**

`target\debug\preamble.exe` runs on the maintainer's Windows machine: a titled window, WebView2
initialised, `msedgewebview2.exe` alive as a child, `Responding=True`, the full UI rendered —
file tree with `main.tex` badged *root*, CodeMirror with `stex` highlighting, the status bar
reading `Tectonic 0.17.0`. **The sprint 1 diagnosis no longer reproduces**: no
`failed to create webview (0x80010108)`, and Kaspersky is still installed and running. Whatever
it was — a KES policy update, or a WebView2 runtime update to 152.0.4191.66 — it is gone, and the
webview is no longer the thing blocking rung 4.

**`fixtures/paper` compiles inside the app**: `tectonic finished success=true ms=1395`, then
`build finished generation=1 success=true diagnostics=0`, and a 48 KB `main.pdf` in
`.preamble/build`. That is the v0.1 exit demo's engine half, performed by the app rather than
argued for.

Running it found four bugs that no test had caught, three of them in code written this session:

1. **`compile.rs` called bare `tokio::spawn` for the progress forwarder** (line 128), from the
   synchronous `compile` command where there is no Tokio context. Every build panicked with
   *"there is no reactor running"* and took the app down. This is precisely the line the
   copy-into-a-throwaway-crate verification of S2.2 and S2.7 could not see, because that
   procedure *swaps `tauri::async_runtime::spawn` for `tokio::spawn`* — so the one call that was
   already wrong was the one the check normalised away. **A verification that edits the code it
   verifies cannot see bugs in the edit.** Now `tauri::async_runtime::spawn`, like its neighbour.
2. **`bridge::path_to_uri` did not percent-encode.** A path with a space produces an invalid URI;
   TexLab answers `unexpected character at index 37` and closes its output, which arrives on our
   side as a language server that crashed on startup and then crash-looped. Index 37 was the
   space in `LaTeX Editor` — **this repository's own path**, which is why the unit tests passing
   meant so little: they all used space-free paths. Encoding added on both sides
   (`src/lib/lsp.ts` mirrors it), with tests for spaces, `#`, `?`, `%` and non-ASCII.
3. **`LspSession::start` killed the server whenever the handshake failed.** The `?` on
   `initialize` returned early, dropping the only `Bridge`; dropping the last one tells the
   supervisor to kill the process. So a handshake that merely failed also took the server down,
   and the log read *"restarted, then bridge dropped"* instead of naming the cause. The bridge is
   now stored before the await, and `stop()` is explicit on the error path.
4. **`initial_project` broke on paths with spaces.** `preamble C:\My Thesis` arrives as
   `["C:\My", "Thesis"]` unless quoted, and `nth(1)` took the truncated half. `folder_from_args`
   now tries the joined tail first, then the first argument; five tests, including the exact
   case. (Found because the smoke harness hit it, but it is a real bug for any author whose
   folder has a space in it.)

**Still open, and each wants its own loop:**

- **The PDF pane stays blank** though the PDF is built and on disk. `src/lib/pdf/` loads the
  pdf.js worker with `?url`, which resolves to an absolute `/assets/…` path; under `tauri dev`
  the page is served from `tauri://localhost` while the worker URL points at
  `http://localhost:1420`, and a cross-origin worker load is refused. The empty-message
  `window.error` in the log matches. That makes rung 4's *"see the PDF update"* the one part of
  the v0.1 exit demo still unperformed — an S1.10 fix, sized S.
- **The window renders into roughly the top half of its frame** at 1456×939 (visible in the
  screenshots). A CSS height problem, not a Rust one.
- **`cargo build --release` is not the release path.** `tauri.conf.json` sets `devUrl`
  unconditionally, so even a release binary loads `localhost:1420` and shows Edge's
  *can't reach this page* without Vite running. `pnpm tauri build` is the supported route and is
  S2.9's business; worth knowing before anyone tests a release binary the quick way.

Rungs 1–2 stay green throughout: 94 Rust tests, clippy clean with `-D warnings`, `svelte-check`
clean, 93 Vitest.

**S3.3a (12 September 2026).** `[x]`: rungs 1–3 are green; rung 4 (`pnpm tauri dev`, typing
`\begin{` by hand) is still pending — no webview in this environment, the same standing
sprint-3 gate as S3.1 and S3.2. `src/lib/lsp-protocol.ts` gives `lsp.ts`'s four request methods
typed unions instead of `Promise<unknown>`, and `src/lib/editor/completion.ts` turns
`textDocument/completion` into a CodeMirror `autocompletion()` source. 408 files clean on
`svelte-check`, 120/120 Vitest across 11 files, clippy clean. What a reader should take from
the diff:

1. **Hand-written protocol types, not `vscode-languageserver-types`.** `lsp-protocol.ts` defines
   the ~30 fields this app actually reads (`Position`, `CompletionItem`, `Hover`, …) plus
   narrowing helpers (`isCompletionList`, `isHover`) instead of pulling in a dependency whose
   full surface no one here will read. Recorded in §4: a dependency tax on the learner-reader
   outweighs the benefit at this size.
2. **`apply` is a plain string, not a captured-offset function.** `completion.ts`'s
   `lspCompletionSource` sets a completion's `apply` to
   `textEdit.newText ?? insertText ?? label` and lets CodeMirror remap `from`/`to` itself.
   The alternative — a closure capturing the offsets at request time — goes stale the moment an
   intervening edit shifts the document, and would insert into the wrong place. This was a
   round-1 required finding (corruption like `\begin{itemizeem}`) that the fix round closed.
3. **`validFor: /^[A-Za-z*]*$/` keeps the request round-trip out of the keystroke path.** Without
   it, every character typed inside an open completion popup re-queries TexLab and re-flushes
   `lsp.ts`'s `didChange` first — a second required finding from round 1. With it, CodeMirror
   filters the existing list client-side until the pattern breaks.
4. **A per-URI send queue in `lsp.ts` serializes concurrent `didChange` calls.** Completion's
   just-in-time flush and the save debounce's flush are two independent callers writing to the
   same file; unserialized, a slow first send can arrive after a fast second one and leave the
   server holding an older version number than the file it just parsed.
   `lsp.test.ts` reproduces the old code delivering versions `[3, 2]` and the fix delivering
   `[2, 3]` — a non-vacuous regression test, not just an assertion that a promise resolved.
5. **UTF-16 boundaries are tested with an emoji and a CJK line.** `positions.ts` converts between
   CodeMirror's UTF-16 offsets and LSP's UTF-16 `Position`; `positions.test.ts` pins the
   surrogate-pair case an ASCII-only fixture would never catch. A bug in the test itself — a
   wrong clamp assertion for a line past the end of the document — was caught and fixed during
   this loop's own verify run and is logged in `bugs-issues-fixes.md`.

Carried forward, advisory, not blocking this tick: the reviewer flagged that `src-tauri/gen/`
and `Abstract-Tex.code-workspace` are untracked and not gitignored. Neither belongs to this
loop; whichever loop next touches `.gitignore` should add them.

**S3.3b (13 September 2026).** `[x]`: rungs 1–3 are green — `pnpm verify` exit 0, cargo test
workspace green, clippy `-D warnings` clean (no Rust touched this loop), svelte-check 410 files /
0 errors / 0 warnings, Vitest 166/166 across 12 files. Rung 4 stays `[~]` for want of a webview:
`jsdom` is not installed in this repo, so there is no `EditorView` test at any level and
`pnpm tauri dev` was not attempted — the same standing sprint-3 gate as S3.1–S3.3a. `src/lib/
lsp-diagnostics.ts` is the new module, translating TexLab's `publishDiagnostics` into rows the
gutter draws; `src/lib/editor/diagnostics.ts` gained a second `StateField` and the merge that
combines it with the build's own. What a reader should take from the diff:

1. **Two `StateField`s, not one merged list.** `diagnostics.ts` keeps `markers` (per build) and
   `lspMarkers` (per publish) apart because they live on different clocks — a build every few
   seconds, a publish potentially every keystroke burst — and a single field would mean each
   source wiping the other's dots every time it spoke. Only the `markers` callback passed to
   `gutter()` ever looks at both, via `mergeMarkers`.
2. **A tie-break bug the test caught before the app ever ran it.** `mergeMarkers`'s first cut
   reused `build`'s displacement rule (`existing is a warning && incoming is an error`) for the
   pass that is supposed to prefer texlog on an *equal* severity — which quietly hands an exact
   tie to whichever source is read first instead of to the log parser's explained sentence,
   DESIGN.md §5.2's requirement. `mergeMarkers gave the LSP marker the exact-severity tie, not
   texlog` in `bugs-issues-fixes.md` is the record; the fix separates "more severe wins" from
   "equal severity, texlog wins" into two conditions instead of one that tried to do both.
3. **The reviewer mutation-tested rather than trusted the count.** Deleting `+ 1` from
   `toEditorDiagnostic`'s `startLine` (the 0-based-to-1-based line conversion) fails seven tests
   across two files; neutering `equalAndFromLog` in `mergeMarkers` fails the §5.2 tie-break test
   on its own. Both were restored. Green suites answer "does it work"; a mutation answers "would
   a test have told us if it didn't" — the project's own recurring worry, checked rather than
   assumed, for the two lines this loop most needed to trust.
4. **A version counter carries a publish into a `$derived` without the store becoming a rune.**
   `LspDiagnosticStore` (`lsp-diagnostics.ts`) is a plain class so it tests with no Svelte
   runtime; `app.lspDiagnosticsVersion` (`state.svelte.ts`) is the one reactive value that moves
   on every `publish`, and `Editor.svelte` takes it as an argument to `lspDiagnosticsFor` rather
   than reading it and discarding the result — a bare `app.lspDiagnosticsVersion;` statement
   reads like dead code to a later editor and invites deleting the subscription along with it.

One card deviation, self-reported and not a defect: `src/lib/state.svelte.ts` was touched
without being named in the card's `Files` list, because `lspDiagnosticsVersion` has nowhere else
to live. A card gap, not a builder error.

Decided this loop, recorded in §4 below: LSP diagnostics reach the gutter and nothing else —
never `app.compile.diagnostics`, the drawer, or `errorCount`/`warningCount`.

Left advisory by the reviewer, for a later loop to pick up: a drive-letter casing mismatch
between `publish`'s URI (the server's spelling) and `forPath`'s (`pathToUri`'s spelling) can
make a lookup miss with nothing erroring — the third Windows-path-spelling bug in this
subsystem, an argument for normalizing in one place; S3.3c or whichever loop next touches
`lsp-diagnostics.ts` should close it. And `diagnostics.ts`'s `markers` callback re-runs
`mergeMarkers` on every view update, allocating two Map/array/RangeSet triples per keystroke —
almost certainly under the <16 ms budget at realistic diagnostic counts, so not blocking, but
worth memoizing on the two field values whenever that loop lands. Both are in
`bugs-issues-fixes.md`, filed 13 September 2026.

`EditorDiagnostic.from`/`to` carry the original 0-based LSP range but nothing reads them yet —
kept deliberately for S3.3d's squiggles, now split out as its own row in the table above.

**S3.3c (13 September 2026).** `[x]`: rungs 1–3 are green — `pnpm verify` exit 0 (94 Rust tests
unchanged, no Rust touched this loop, clippy `-D warnings` clean), `svelte-check` 416 files / 0
errors / 0 warnings, Vitest 200/200 across 15 files (33 new). Rung 4 (`pnpm tauri dev`,
Ctrl-clicking a `\ref` and hovering an undefined command by hand) is still pending — no webview
in this environment, the same standing sprint-3 gate as S3.1–S3.3b. Three new modules under
`src/lib/editor/` — `hover.ts`, `definition.ts`, `symbols.ts` — plus three new controller
functions (`lspHover`, `lspGoToDefinition`, `lspDocumentSymbols`) and their wiring into
`setup.ts`/`Editor.svelte`. What a reader should take from the diff:

1. **`hoverTooltip`'s source function is exported apart from the extension it builds, because
   the extension gives no other way to call it.** `hoverTooltip(source)`'s return value exposes
   only an `active` state field for reading what is *currently* shown, not `source` itself — so
   `hover.ts` exports `hoverSource(view, pos, request)` as a plain async function and
   `lspHoverSource` is a two-line wrapper around it. `hover.test.ts` calls `hoverSource` directly;
   without the split, testing it at all would mean driving CodeMirror's real hover lifecycle
   (idle timers, pointer events) from Vitest.
2. **Go-to-definition is one `DefinitionRequester`, not a `Promise<Location | null>`, because
   the CodeMirror layer must not know about tabs.** `completion.ts` and `hover.ts` both stop at
   "ask a position, get an answer back" and leave rendering to CodeMirror's own machinery; a
   definition can point at a file that has no tab open yet, and only `controller.svelte.ts` (via
   `DocumentManager`) can open one. So `definition.ts`'s `DefinitionRequester` returns
   `Promise<boolean>` — "was something found" — and `lspGoToDefinition` in the controller does
   the whole job: request, resolve `Location | Location[] | null` down to one location
   (`firstLocation`), turn its `uri` back into a project-relative path with `uriToPath` +
   `toRelative`, and either move the cursor or call `openFile` first.
3. **This environment cannot construct a real `EditorView` at all, which shaped every test in
   this loop, not just one of them.** `vite.config.ts` runs Vitest under `environment: 'node'`
   (recorded as a gap in S3.3b's outcome); `new EditorView(...)` calls `document.createElement`
   internally and throws `document is not defined`. `hover.test.ts` and `definition.test.ts`
   pass structural stand-ins — a bare `{ state: { doc } }` — cast to `EditorView`, since the
   functions under test only ever read `view.state.doc` (and, for the click handler,
   `posAtCoords`); `completion.test.ts` already did the analogous thing for `CompletionContext`.
   A `MouseEvent` is faked the same way in `definition.test.ts`, for the same reason.
4. **`documentSymbol` shipped as data with tests and no UI, on purpose.** The card allows this —
   "a plain data-returning function with tests may be sufficient for this loop" — and S4.2's
   Document map panel is explicitly where a tree view belongs. `symbols.ts`'s `flattenSymbols`
   depth-first-flattens `DocumentSymbol.children` into a list with an explicit `depth` field, so
   S4.2 starts from a working request instead of inventing the walk under UI pressure.

One thing checked and found already safe, not a new fix: `lspGoToDefinition` turns a server URI
back into a path with `uriToPath` + `toRelative`, which is the same drive-letter-casing hazard
flagged advisory in S3.3b's outcome for `lsp-diagnostics.ts`'s raw `Map` lookup — but
`toRelative` (`paths.ts`) already compares path segments case-insensitively, so this call site
does not reproduce that bug. The advisory item itself is unchanged and still open, since fixing
it belongs with `lsp-diagnostics.ts`'s own `Map`, which this loop did not touch.

**S3.3d (13 September 2026).** `[x]`: rungs 1–3 are green, and for the first time this loop
verified them for real rather than through the throwaway-crate workaround — this session runs on
the maintainer's Windows machine, where `cargo test --workspace` (105 tests: 25 + 0 + 6/7 + 18 +
6/8 + 5/6 + 9 + 4 + 23, the fractions being the `--ignored` real-server/real-Tectonic tests this
run skipped) and `cargo clippy --workspace --all-targets -- -D warnings` both build `src-tauri`
and pass in place. `svelte-check` 416 files / 0 errors, Vitest 209/209 across 15 files (9 new).
Rung 4 (`pnpm tauri dev`, hovering a squiggle by hand) is still pending — no webview attempted
this session — the same standing sprint-3 gate as S3.1–S3.3c, though for the first time that gate
is "wasn't tried" rather than "can't be tried here." Only `src/lib/editor/diagnostics.ts` and its
test changed; no other file needed touching, and no Rust changed. What a reader should take from
the diff:

1. **The squiggle is a third `StateField`, not a repaint of the gutter's.** `EditorDiagnostic.
   from`/`to` were written by S3.3b and read by nothing until now — the 0-based LSP range the
   gutter's line-only markers threw away. `squiggles` is a `StateField<DecorationSet>` fed by its
   own `StateEffect` (`setSquiggles`), dispatched alongside `setLspDiagnostics` from the same
   `applyLspDiagnostics` call, in one transaction — they are two views of one publish, not two
   stores that could drift apart for a frame. Build diagnostics never get a squiggle: a texlog
   line claim has no column, so there is nothing to underline, and the card scopes this to LSP
   ranges only.
2. **`Decoration.mark` needs a `RangeSetBuilder`, not `RangeSet.of`, and the difference is an
   ordering contract, not a style choice.** `build`/`buildLsp` above already had working code that
   maps diagnostics into a `RangeSet` via `RangeSet.of(ranges, true)`, which sorts its input for
   you. `RangeSetBuilder.add` does not — it throws if ranges arrive out of order — so
   `buildSquiggles` sorts once before the loop rather than trusting every future caller (or a
   server that publishes diagnostics out of position order) to hand them over pre-sorted. A test
   passes diagnostics in reverse-line order specifically to pin this.
3. **The diagnostic hover is synchronous, and that is the whole reason it needed no `hover.ts`
   wrapper.** `hover.ts`'s `HoverRequester` exists because `textDocument/hover` is a round trip to
   TexLab; a diagnostic's message is already sitting in the `squiggles` field from the last
   publish, so `diagnosticHoverSource` just reads it back with `DecorationSet.between` — no
   promise, no requester type, no controller wiring. `diagnosticAt` is split out from it the same
   way `hoverSource` is split from `lspHoverSource`: a pure function over a `DecorationSet` and a
   position, callable from Vitest without driving `hoverTooltip`'s real idle-timer lifecycle.
4. **The wave is an inline SVG `background-image`, not `text-decoration: wavy`.** CSS's own wavy
   underline has no controllable amplitude or stroke width across engines, and rather than fight
   that, `squiggleTheme` draws a 6×3 SVG tile and repeats it — same `EditorView.baseTheme` pattern
   as the gutter's `theme` and `setup.ts`'s `hoverTheme`, and the same `var(--error)`/`var(--warn)`
   colour pair the gutter dots already use (as literal hex in the SVG, since `url()` values cannot
   reference a CSS custom property).

One `EditorDiagnostic` field this loop leaned on that the drawer/gutter never needed: the
zero-width range case (`from === to`, legal under the spec, and a real shape for a point
diagnostic like "expected a `}` here"). `Decoration.mark` throws on `to <= from`; `buildSquiggles`
widens such a range to one character rather than silently dropping the diagnostic's underline.
Covered by its own test.

The advisory drive-letter-casing bug on `lsp-diagnostics.ts`'s `Map` lookup (logged in
`bugs-issues-fixes.md`, S3.3b) is unchanged by this loop: squiggles read `EditorDiagnostic` rows
that already came out of `LspDiagnosticStore.forPath`, the same lookup the gutter has always used,
so this loop neither closes nor worsens it. Still open, still a one-line fix wherever
`lsp-diagnostics.ts` next gets touched for its own reasons.

**S3.4 (13 September 2026).** `[x]`: rungs 1–3 are green, run for real on this Windows machine
rather than through the Linux-sandbox workaround earlier loops needed — `cargo test --workspace`
(112 tests, one new `--ignored` real-Tectonic test), `cargo clippy --workspace --all-targets -- -D
warnings` clean, `svelte-check` 418 files / 0 errors, `pnpm vitest run` 220/220 (11 new). Rung 4
(`pnpm tauri dev`, moving the cursor and checking the PDF highlights) was **not** driven
interactively this session: the debug build compiles cleanly on its own
(`cargo build -p preamble`), but nobody clicked through the running window to watch a highlight
appear, so that check is still owed to a human at the keyboard.

A new library crate, `crates/preamble-synctex/`, parses `.synctex.gz` and answers both directions
(forward here; inverse is S3.5, sharing the same parsed `SyncTex` value rather than a second
parser). Three things worth a reader's attention:

1. **The fixture is a real Tectonic build, not hand-typed bytes, and that discipline caught two
   real bugs the same day.** `fixtures/multi.synctex.gz` was produced by running the bundled
   Tectonic sidecar against `fixtures/multi.tex` (a two-page, four-paragraph document), the same
   "verification that edits the code cannot see" lesson this project already carries applied to a
   *binary format* rather than a Rust file. It found: (a) `inverse_search`'s naive `min_by` kept
   the *first* record at a tied minimum distance, which was the enclosing paragraph's box, not the
   more specific line a `h` void-box record at the same point actually named — fixed by preferring
   the *last* tied record, since SyncTeX writes outermost-first; (b) `paths_match` compared
   `to_string_lossy()` case-insensitively but not separator-insensitively, so a test path built as
   `dir.join("fixtures/multi.tex")` (a literal forward slash inside one Windows `Path` component)
   silently failed to match SyncTeX's own backslash-separated `Input:` line for the identical
   file. Both are logged in `bugs-issues-fixes.md` with the fixture that exposed them.
2. **The coordinate constant, 65781.76, was verified against a second real implementation, not
   derived from the spec alone.** SyncTeX stores scaled points (1/65536 of a *TeX* point, 72.27
   dpi), but PDF and pdf.js want *PDF* points (72 dpi); the man page never states the combined
   constant outright. `65536.0 * 72.27 / 72.0 = 65781.76` was cross-checked against LaTeX
   Workshop's shipped, maintained `synctexjs.ts`, which divides by the identical literal — recorded
   in `preamble-synctex/src/lib.rs`'s module doc comment so a future reader does not have to
   re-derive it from the spec's prose.
3. **The crate is a flat scan over every record, on purpose, not a box tree.** Real SyncTeX nests
   `[`/`]` and `(`/`)` to describe TeX's box structure, but neither forward nor inverse search asks
   a nesting question — both only need "which record is at/near this point" — so `parse` tracks
   only the enclosing page (`{`/`}`) and pulls a flat `(tag, line, h, v)` out of any content line
   shaped that way, ignoring record-type and nesting entirely. Simpler than a real box-tree parser,
   and a linear scan over a few thousand records is well inside the keystroke budget (DESIGN.md
   §2) without an index — worth revisiting only if a real thesis's SyncTeX file proves otherwise.

The Tauri side is a new `src-tauri/src/synctex.rs` (path resolution: `build_dir/<stem>.synctex.gz`,
the same convention `commands::read_log` already used for `.log`) plus `synctex_forward`/
`synctex_inverse` commands — both wired now since S3.5 needed the inverse command's shape decided
anyway and the card's own dependency line points S3.5 at this loop's output. On the frontend,
`editor/synctex.ts` mirrors `definition.ts`'s "ask, do not act" seam (`Ctrl-Alt-J` reads the
cursor's line and calls a requester); `pdf/viewer.ts` gained `scrollToPosition` (an absolutely
positioned, CSS-faded `<div>` over the target canvas, never drawn into the canvas itself, so a
reload's `replaceChildren` clears it for free) and `onInverseSearch` (a double-click handler,
since a single click is pdf.js's own text-selection gesture and must not be stolen).

**Design call made without an architect, recorded here per the card's instruction:** TexLab's own
`initializationOptions` schema (checked for S3.6, not this loop) has no equivalent field for "the
line an editor's cursor is on," so there was no existing convention to match — `Ctrl-Alt-J` was
chosen fresh, next to `Alt-F12` (go-to-definition) and clear of every binding already in
`setup.ts`'s keymap and `shortcuts.ts`'s window-level table.

**S3.5 (13 September 2026).** `[x]`: rungs 1–3 are green — `cargo test --workspace` (113 tests,
one new), `cargo clippy --workspace --all-targets -- -D warnings` clean, `svelte-check` unchanged
at 0 errors, `pnpm vitest run` 220/220 (the S3.4 commit already added `syncTexInverse`'s tests).
Rung 4 was not driven interactively this session, the same standing gap S3.4 recorded.

Most of this card's plumbing — the `synctex_inverse` command, `preamble-synctex::inverse_search`
sharing S3.4's parsed `SyncTex` rather than a second parser, `syncTexInverse` in the controller,
and `PdfViewer::onInverseSearch`'s double-click handler — was built and tested in the S3.4 commit,
because the card's own dependency line pointed S3.5 at exactly that shape and building the
direction twice, a commit apart, would have meant either reparsing or an awkward half-finished
`SyncTex` API in between. What this loop added:

1. **An end-to-end test through the real committed fixture, not just the two halves separately.**
   `src-tauri/src/synctex.rs` gained `inverse_search_end_to_end_against_the_real_fixture`: forward
   search finds a real point on page 1, inverse search is asked about that exact point, and the
   result is checked against the file and line the fixture is actually built from — the same
   "prove it against real bytes" standard `preamble-synctex`'s own tests already hold to, applied
   one layer up, at the layer that turns a hit back into the project-relative path `openFile`
   needs.
2. **That test found a third real path bug the same day as S3.4's first two.** `Path::new(...)
   .join("../crates/preamble-synctex/fixtures")` keeps the literal `..` component, and
   `paths_match`'s string comparison never resolves it against the fully-resolved path the `.gz`
   itself records — so the test's own path construction, not the library code, was wrong at
   first. Fixed with `std::path::absolute`, the same function `project.rs`'s `Project::open`
   already uses and for the same reason (normalises without `canonicalize`'s `\\?\` prefix).
   Logged in `bugs-issues-fixes.md` alongside S3.4's two.
3. **`InverseResult::file` being `None` is not an error, and `syncTexInverse` treats it that
   way.** A click can resolve to a record whose file lies outside the project (a package's own
   `.sty`, in principle, though not exercised by the current fixture) — there is no tab to open
   for that, so the controller simply returns rather than raising a notice. Only a page with no
   SyncTeX records at all — a stale click after the document changed shape — surfaces a sentence,
   via the `Err` path `synctex_inverse` already returns a clean message for (`synctex.rs`'s
   `open`, from S3.4).

**S3.6 (13 September 2026).** `[x]`: rungs 1–3 are green — `cargo test --workspace` (117 tests,
4 new), `cargo clippy --workspace --all-targets -- -D warnings` clean, `svelte-check` and
`pnpm vitest run` unchanged since this loop touched no frontend file. Rung 4 was not driven
interactively this session, the same standing gap S3.4/S3.5 recorded.

**The card turned out narrower than its own title, and the reason is worth recording rather than
guessing past.** "Root file, build dir" reads as two things to pass; checked against TexLab
5.26.0's actual deserialisation code (`crates/texlab/src/server/options.rs`'s `Options` struct, at
the exact pinned tag — not the README, which mentions a `texlab.rootDirectory` setting that turned
out not to exist at this version, logged as a "won't fix" in `bugs-issues-fixes.md` since there
was nothing on our side to fix), there is only one: TexLab finds its own root document by walking
up for `\begin{document}`, and offers no field to be told which file that is. What it does accept
are three build-output directories — `build.auxDirectory`, `build.logDirectory`,
`build.pdfDirectory` — all relative to the root document's own directory since TexLab 5.0, not the
workspace root. So this loop passes exactly that: the project's `.preamble/build`, as a path
relative to the project folder, through `initializationOptions` on the `initialize` request.

1. **`Bridge::initialize` gained a third parameter rather than a second overload,** because LSP's
   own spec leaves `initializationOptions` as "any" — server-defined — and threading `Option<Value>`
   through one signature keeps every caller (there is exactly one, in `src-tauri/src/lsp.rs`, plus
   two real-server tests in `preamble-lsp/tests/bridge.rs` that pass `None`) explicit about
   whether it has anything to say, rather than silently sending `null` to servers with nothing to
   configure. `crates/preamble-lsp/src/bridge.rs`'s own doc comment is where this is recorded,
   since `preamble-lsp` is the crate a future server integration would read first.
2. **`texlab_settings` is a pure function returning `Option<Value>`, not four lines inlined into
   `start()`.** `None` when the build directory cannot be expressed relative to the project root
   (`pathdiff`'s `strip_prefix` failing) — which cannot happen through this app's own
   `Project::build_dir`, but a function this cheap to test should say "I don't know" rather than
   emit a relative path that starts climbing out of the project with `..`. Four tests pin the
   three field names, the forward-slash normalisation (`\` never survives on Windows, the same
   normalisation `preamble-synctex`'s `paths_match` needed for the same reason this week), and the
   `None` case.
3. **A real gap, named rather than silently accepted:** `texlab_settings` computes the build
   directory relative to `Project::root_dir` (the project folder), not the root `.tex` file's own
   directory, because `LspSession::start` is only ever given the project directory today — the
   two coincide for every project `detect_root` actually finds a root in (`main.tex`, or any
   `.tex` up to three folders deep, always inside the project folder), but would be wrong for a
   root file `set_root_file` places in a subfolder. Fixing it needs the root file's own path
   threaded alongside the directory, which is a bigger change than this S-sized card's scope;
   left as an open question below rather than solved speculatively.

**Open question for the architect:** should a later loop thread the root file's path (not only
the project directory) into `LspSession::start`, so `texlab_settings`'s relative-path computation
is correct for a root file in a subfolder too? Low priority — `detect_root`'s own search order
makes a root file outside the project's top level the less common case — but worth a decision
before `set_root_file` is more prominently exposed in the UI.

### Sprint 4 — v0.2 exit: navigation

**Exit demo.** `DESIGN.md` §7 v0.2 on `fixtures/thesis` (six files).

| ✓ | Loop | Size | Depends |
|---|---|---|---|
| [~] | S4.1 `\input`/`\include` graph in Rust; root detection uses it; watcher compiles on any node change | M | S1.4 |
| [~] | S4.2 Document map panel: sections, figures, tables, labels, TODOs, from LSP symbols plus our own scan | M | S3.3 |
| [~] | S4.3 Command palette `Ctrl K`: actions, files, sections, fuzzy matching, every action registered through one registry | L | S2.4 |
| [~] | S4.4 `fixtures/thesis` six-file skeleton and its smoke script | S | — |
| [~] | S4.5 Focus and typewriter modes | S | S1.9 |
| [~] | S4.6 Maths preview on hover with KaTeX | S | S1.9 |
| [ ] | S4.7 Linux and macOS smoke on CI artifacts; WebKitGTK issues logged as loops | M | S2.8 |

**S4.1 (14 September 2026).** `[~]`: rungs 1–2 are green — `cargo test -p preamble-includes` 19
passed, `cargo clippy -p preamble-includes --all-targets -- -D warnings` clean; `cargo test -p
preamble -- project` still cannot link in this sandbox (no WebKitGTK, no root — the standing gap
since sprint 2), so `project.rs` was verified the S2.2/S2.7 way instead, copied unmodified into a
throwaway crate path-depending on the real `preamble-includes`: 14 passed, clippy clean, an
in-place run on a machine that can link Tauri is still owed; `pnpm test -- paths` 11 passed;
`pnpm verify:web` 0 errors, 225 vitest; the rest of the workspace (`cargo test --workspace
--exclude preamble`, minus the two pre-existing Linux-only failures already logged before this
loop started) stayed green, clippy clean throughout. Rung 4 (`pnpm tauri dev` smoke) is still
pending: no webview in this Linux sandbox — the card's own verify line scopes rung 4 as pending
rather than required for this loop, but the file's own convention keeps the tick at `[~]` until a
webview exists here to run it on. This was also the first session on a brand-new Linux sandbox
with no prior cargo/pnpm cache and, for the first time, no system C compiler at all; the
orchestrating session bootstrapped rustup, Node 22/pnpm 9, and a Zig-backed `cc`/`gcc`/`c++`/`g++`
wrapper under `~/.local/bin`, all user-level and outside this loop's own diff, before any of the
above could run.

A new crate, `crates/preamble-includes`, splits pure text scanning from filesystem walking the way
`preamble-reconcile` already models: `scan.rs`'s `scan_includes(&str) -> Vec<Directive>` never
touches disk; `graph.rs`'s `build_graph(project_dir, root_relative) -> IncludeGraph` does the BFS,
the visited-set dedup by resolved path, and the depth cap. `project.rs`'s `detect_root` now
excludes any `.tex` candidate another file's scan includes before its name/depth tie-break, and
`ProjectInfo` carries `document_files`/`document_files_complete` to the frontend; `paths.ts`'s new
`shouldCompileFor(relative, project)` is what `controller.svelte.ts`'s `handleFsEvent` now gates
recompilation on, in place of `isTexSource`.

1. **Two bugs the fix round caught before either shipped, both about trusting an argument string
   too literally.** `scan.rs`'s `is_literal_argument` accepted `\input{chapters/#1}` — the body of
   a `\newcommand`, never expanded by a scanner that only reads source text — as a real path, so a
   chapter loaded that way silently dropped out of `document_files` with nothing saying so; the fix
   rejects any argument containing `#` as a macro placeholder, turning it `Unparsed`, which is what
   makes the graph correctly report itself incomplete. `graph.rs`'s `resolve_include_argument` used
   `PathBuf::set_extension("tex")` to add the implicit extension LaTeX assumes — but
   `set_extension` *replaces* whatever follows the last `.`, so `\input{data.2024}` resolved to
   `data.tex` and lost `2024` entirely. `append_tex_extension` fixes it via `with_file_name`
   instead, and its doc comment names the trap for a reader who has not hit `set_extension`'s
   surprise yet — worth the maintainer's ten minutes on its own.
2. **The BFS terminates a cycle by dedup, and a pathological chain by depth, and the two are
   deliberately separate limits.** `IncludeGraph`'s visited set is keyed on resolved path, so a
   two-file `\input` cycle stops after each file is scanned exactly once; `MAX_DEPTH = 32` is a
   second, independent backstop, not the cycle's own guard. The reviewer's one gap here — the cap
   drops a branch past depth 32 with no `Unresolved` entry, contradicting the module's own promise
   never to skip silently — is logged rather than fixed this loop.
3. **Root detection resolves against a directory it has not found yet, and the deviation was
   flagged rather than hidden.** `detect_root`'s preliminary include scan (which candidate is
   included by another) has no root to resolve arguments relative to — the root is exactly what is
   undetermined at that point — so it resolves against the project directory instead. The reviewer
   showed this can misfire: two files that `\input` each other are now *both* excluded, and
   `detect_root` returns `None` instead of the old name tie-break picking one. Not fixed here;
   logged as an `Open` advisory with a fix direction (fall back to the un-excluded list rather than
   `None`).

Carried forward as `Open` advisories in `bugs-issues-fixes.md`, none `required` so none blocks this
tick, each pointed at whoever should absorb it:

- `detect_root`'s exclusion has no fallback and ignores depth (`project.rs:220-233`) — no sprint-4
  card owns hardening root detection yet; whoever next touches it should start here.
- The include graph's depth cap drops nodes with no `Unresolved` entry (`graph.rs`) — same file,
  same absence of an owning card.
- `root_relative` reaches `build_graph` unnormalised, so a hand-edited `preamble.toml` with
  `root = "./main.tex"` can desync `shouldCompileFor` from the watcher — same file, same gap.
- Includes reached only through a `.sty`/`.cls`, `\InputIfFileExists`, or `\subimport` stay
  invisible while `is_complete()` still reports `true` — the `.sty`/`.cls` half is a design question
  for the architect to settle, not a bug to fix; the other two are a straightforward `COMMANDS`
  addition for whoever picks the rest of this up.
- `documentFiles.includes` does a case-exact match (`paths.ts:40`), traced upstream to
  `resolve_include_argument` not canonicalising case — owned by whoever next touches that function.
- No controller-level test covers the `isTexSource` → `shouldCompileFor` swap itself — S4.4's
  `fixtures/thesis` six-file skeleton is the natural place to add one once it exists.
- `Project::info()` re-reads every document file on every debounced tree refresh, measured at
  128 ms on a synthetic 200-chapter project — the architect's own card already pointed this at
  sprint 9 (S9.1's benchmark corpus).

Two implementation choices the builder flagged as open questions rather than silent decisions,
neither judged here to need a `DESIGN.md` §10 row: resolving `detect_root`'s preliminary scan
against the project root instead of each file's own directory (recorded in-line at the call site),
and a flat `Unresolved` enum in place of the card's suggested single
`UnresolvedInclude{from,line,reason}` shape, since `Unreadable` has no natural `from`/`line`. Both
are cheap to revisit if the architect disagrees.

**S4.2 (14 September 2026).** `[~]`: built, not yet reviewer-approved (review deferred to a
later batched pass, per the maintainer). `pnpm verify:web` is green — svelte-check 0 errors,
243 vitest tests (18 new). No Rust or `src-tauri` file was touched, so the standing WebKitGTK
link gap does not apply to this loop at all. Rung 4 (click a section in `pnpm tauri dev`, watch
a new one appear while typing) is pending, the same standing no-webview gate as every loop since
S3.1.

`src/lib/outline.ts` is a new pure module: `scanOutline(text)` walks the buffer once for
sections (six levels plus starred forms, brace-balanced single-line titles), figure/table
environments (with a `\caption` lookahead or a bare-word fallback), `\label` keys, and
`%TODO`/`%FIXME` comments (an unescaped-`%` detector keeps `100\%` from being misread as a
comment). `mergeOutline` layers TexLab's `documentSymbol` answer over the scan when the server
is ready — our TODO rows have no server equivalent, so they always survive; the scan is the whole
outline when there is no server. `state.svelte.ts` gained a raw `outline` field; `controller.
svelte.ts`'s `refreshOutline()`/`scheduleOutlineRefresh()` (250 ms debounced off a new
`onTextChange` hook in `document.ts`, fired for every observed change including
`ORIGIN_LOAD`/`ORIGIN_EXTERNAL`) guard a tab-switch race the same way `Editor.svelte`'s per-tab
closures already do — capture `activePath` before the `lspDocumentSymbols` await, compare after,
drop the answer if the tab moved on. `DocumentMap.svelte` renders one `<button>` per row under
`Sidebar.svelte`'s existing file tree, each reachable by Tab/Enter.

**Reviewer pass (16 September 2026).** `pnpm verify:web` green — svelte-check 0 errors, 285
vitest (1 new). The two questions this loop's own outcome flagged for the deferred round are both
settled: `extractBraceGroup`'s brace balancing was checked against nested titles and holds; the
250 ms debounce and the tab-switch guard already had controller-level coverage
(`controller.test.ts`'s "the Document map (S4.2)" block — the debounce test advances fake timers
to 249 ms then 1 more, the switch test reopens a tab and asserts the outline repaints for the
newly active one), so nothing was missing there after all.

What the pass did find, and fixed: **`SECTION_RE` had no word boundary after the command name**,
so `\partial` (common in any document with calculus) matched the `part` alternative with `ial`
left dangling — no `{` follows, so the title comes out empty, and the Document map would have
shown a spurious untitled "Part" row on real math-heavy prose. `\paragraphindent` has the same
problem with `paragraph`. Fixed with a `(?![a-zA-Z])` lookahead between the alternation and the
optional `\*`; regression test and full writeup in `bugs-issues-fixes.md` (Fixed, 16 Sep 2026).
Still `[~]`: rung 4 (`pnpm tauri dev` on `fixtures/thesis`, clicking a row) is the same standing
no-webview gate every frontend loop since S3.1 has carried.

**S4.3 (14 September 2026).** `[~]`: built, not yet reviewer-approved (review deferred to a
later batched pass, same as S4.2). `pnpm verify:web` green — svelte-check 0 errors, 250 vitest
tests (7 new). Frontend-only, no Rust touched. Rung 4 pending, same standing no-webview gate.

`src/lib/commands.ts` is a new plain registry (`Command`, `registerCommand`, `allCommands`,
`searchCommands`) holding only *actions* (save, compile, open folder, go-to-file); files and
outline sections are built dynamically in `CommandPalette.svelte` since they change with the
open project, then ranked through the same `fuzzy.ts` used by the existing `Ctrl P` quick-open.
`Ctrl K` (`Mod-K` in `shortcuts.ts`, alongside the four existing chords, unchanged) opens the
palette; `Ctrl P`/`QuickOpen.svelte` was deliberately kept as its own separate dialog rather than
folded in, to guarantee zero regression risk on that existing chord — the two share `fuzzy.ts`'s
ranking and a newly-extracted `highlightMatch` (factored out of `QuickOpen.svelte`, which is a
file outside the card's own list — a flagged, low-risk deviation) rather than duplicating the
list-highlighting logic.

Two things the builder flagged rather than deciding silently, settled by the deferred reviewer
pass (16 September 2026): touching `QuickOpen.svelte` to extract `highlightMatch` is fine as is —
it removed a real duplication, both dialogs still pass their own suites, and nothing about it was
`Ctrl K`-specific. "Go to file…" is **removed** from the registry: with `Ctrl K` already listing
every project file as its own searchable entries (`fileCommands`), the action added nothing but a
second modal on top of the one the user is already in — selecting it from inside the palette just
closed the palette and reopened `QuickOpen` for a list the palette had just shown. `Ctrl P` itself
is untouched and still opens `QuickOpen` directly, so the standalone shortcut loses nothing.
`pnpm verify:web` stayed green after the removal (no test pinned the registry entry by id or
title). Still `[~]`: rung 4 is the same standing no-webview gate as every frontend loop since
S3.1.

**S4.4 (14 September 2026).** `[~]`: built, not yet reviewer-approved (review deferred to a
later batched pass). `fixtures/thesis/` is a real six-file skeleton — `main.tex`,
`preamble.tex`, and four `sections/*.tex` chapters on a plausible CS topic (gossip-estimated
distributed rate limiting) — that compiles clean with the bundled Tectonic: verified directly
by invoking `src-tauri/binaries/tectonic-x86_64-unknown-linux-gnu -X compile` against it,
exit 0, `main.pdf` produced (9 pages, 42 KiB), zero warnings and zero errors in stderr. `main.tex`
uses four `\include`s (page-break-per-chapter, unlike `fixtures/paper`'s `\input`-only style)
specifically to exercise S4.1's include graph against a different structure than its own test
fixtures used. Each chapter has real multi-paragraph prose, section/subsection structure, at
least one `\label`, and deliberate cross-file `\ref`s (introduction → background, results →
introduction and background, conclusion → results and background) so S3.3c's go-to-definition
and S3.4/S3.5's SyncTeX both have something meaningful to resolve across files; `results.tex`
carries a captioned `table` and `figure` for S4.2's document map to render. `SMOKE.md` is a
manual walkthrough in `fixtures/paper/SMOKE.md`'s style, covering the include graph, document
map, `Ctrl K` palette, and cross-file navigation.

One deviation, resolved rather than left broken: an initial `\tableofcontents` produced an
`Object @page.1 already defined` xdvipdfmx warning — a real collision between hyperref's
per-page anchors and `report`'s titlepage landing on the same physical first page as the first
`\include`d chapter, reproduced independently in a throwaway minimal case to confirm the cause
had nothing to do with the fixture's prose. Fixed with `pageanchor=false` on hyperref's load
(commented in `preamble.tex`) rather than dropping the table of contents from a still-open
question — a real thesis normally has one, but the card's `Done when` clause did not require it
and a definitely-clean compile was preferred over matching that convention exactly. Worth an
architect nod on whether to add it back once the anchor interaction is well enough understood
to keep.

**Deferred reviewer pass (16 September 2026).** The fixture's own cross-references were checked
against the actual chapter files (every `\ref`/`\label` pair `SMOKE.md` §1/§2/§4 names does exist
and does sit under the section the script claims), and one real mismatch was found and fixed:
**`SMOKE.md` §3 step 3 described a search the application cannot do.** It asked the reader to
type `convergence` while `sections/conclusion.tex` was the active tab and expected
`Convergence after a traffic shift` (a section that lives in `sections/results.tex`) to surface
from "the document map's own section list" — but `CommandPalette.svelte`'s `sectionCommands`
(S4.3) is deliberately scoped to `app.outline`, which is only ever the *active* file's outline
(S4.2's own design note: "a palette is for getting somewhere fast, not for browsing every section
of every file in the project at once"). As written, a maintainer running this step by hand would
watch it fail. Fixed by reordering the walkthrough to open `sections/results.tex` first, so the
section it searches for is the active file's own — same demonstration, accurate to what the code
actually does. No `.tex` file changed; the fixture itself already compiles clean (verified at
build time in this loop's own outcome above). Still `[~]`: rung 4 (walking the script by hand)
still needs a real webview.

**S4.5 (14 September 2026, outcome backfilled 16 September 2026).** `[~]`: this row's own
outcome paragraph was never written at the time — a scribing gap in the loop that built it; the
commit message (`85fdfca`) carries what should have landed here. `pnpm verify:web` was green at
the time: 0 errors, 263 vitest (13 new); no Rust touched. `src/lib/editor/focus.ts` dims every
paragraph but the one under the cursor (`currentParagraphRange` is the pure, testable core —
walks `docText` with `indexOf`/`lastIndexOf` rather than materialising a line array, so the cost
tracks the size of the current paragraph, not the document); `src/lib/editor/typewriter.ts` keeps
the cursor's line vertically centred (`centeredScrollTarget` is the same kind of pure core, the
DOM-touching `centerCursor` wrapped thinly around it). Both extensions sit behind their own
`Compartment` in `setup.ts`; `Editor.svelte` — one file, since only one `EditorView` is ever live
at a time, rebuilt on every tab switch (confirmed while reviewing this loop, since a
`Compartment` shared across simultaneously-live views would have been a real bug) — applies a
toggle to the live view with two small `$effect`s.

**Deferred reviewer pass (16 September 2026): APPROVE, no required findings.** Checked
specifically because a shared module-level `Compartment` is a plausible way for S2.3's multi-tab
architecture to leak state between tabs: confirmed `Editor.svelte` destroys and recreates the one
`EditorView` on every tab switch (the comment at its `$effect` says as much), so the compartment
is never live in two configurations at once. One advisory, not required: `buildDecorations` in
`focus.ts` walks every line of the document on every keystroke and every cursor move to rebuild
the dimming set, the same shape as the gutter's `mergeMarkers` (S3.3b) and `Project::info()`
(S4.1) — almost certainly under the <16 ms budget at thesis-length documents, since the per-line
work is a single `RangeSetBuilder.add`, but the same "worth memoizing if it ever measures
otherwise" note applies. Logged in `bugs-issues-fixes.md`.

**S4.6 (16 September 2026).** `[~]`: rungs 1–2 are green — `pnpm verify:web` (svelte-check 431
files / 0 errors, Vitest 21 files / 283 tests, 20 new), no Rust changed so the standing
WebKitGTK link gap does not apply; the library-crate half of `pnpm verify`'s rung 1 stayed
green too, apart from the two pre-existing, environment-bound failures S4.1 and this session
both already carry (the `preamble-lsp` pipe deadlock and `preamble-synctex`'s Windows-path
fixture, the latter newly ledgered this loop). Rung 4 (`pnpm tauri dev` on `fixtures/thesis`,
hovering `$C$`/`$r$`/`$k \in \{2,4,8\}$`, then a deliberately broken `$\frac{a}{b$`) is still
pending — no Tauri link on this Linux box, the same standing gate every frontend-only loop
since S3.1 has carried. Reviewed: APPROVE, no required findings.

`src/lib/editor/math-preview.ts` is a second, independent `hoverTooltip` alongside S3.3c's
`lspHoverSource`, built the way `hover.ts` and `focus.ts` already model this codebase's split
between pure logic and the CodeMirror shell around it.

1. **The scanner is bounded by the paragraph, not the document, for the same reason
   `focus.ts`'s dimming is.** `mathAtOffset` calls `currentParagraphRange` before it looks at a
   single character, so a hover near the end of a long chapter costs a few lines of scanning, not
   the whole file — the latency commitment (§2.2) is kept by never doing document-sized work on a
   300 ms idle timer, not by making that work fast.
2. **`renderMath` returns a `{html}|{error}` union instead of throwing, and that shape is the
   whole reason the "never a raw log" promise holds here.** A `katex.ParseError` becomes one
   short sentence off `e.message`; an unresolved `\newcommand` lands in the same branch. The
   architect's decision below is what makes that acceptable rather than a gap: KaTeX gets no
   macro table in v0.2, so a preamble command shows KaTeX's own "undefined control sequence"
   line, not a stack trace — cheap now, revisited once Sprint 6's rule catalog already parses
   preambles.
3. **A real bug in the scanner, caught by review, not by the 20 tests that shipped with it.**
   None of the card's cases put a bare `$` inside a `%` comment next to real maths in the same
   paragraph; the reviewer's scenario — `% price is $5` followed by `The value $x$ is` — shows
   the unstripped `%` shifting every pairing after it, so hovering `x` finds nothing and hovering
   "The value" renders `5\nThe value` as if it were a formula. Left `Open` rather than fixed in
   this pass; whoever next touches `math-preview.ts` should blank each unescaped `%` to end of
   line before scanning, the same trick the scanner already uses for `\%`.
4. **`multline` needed a KaTeX-specific patch, not a card-level one.** KaTeX has no `multline`
   environment at all ("No such environment"); the card's own rule keeps the `\begin`/`\end`
   wrapper verbatim for every environment, so `mathAtOffset` stays generic and the substitution
   (`multline`→`gather`) lives only in `renderMath`, the one place that already knows it is
   talking to KaTeX specifically.

Two more advisories from the reviewer, neither fixed this loop: `katex`'s default
`strict: 'warn'` logs a console warning on every hover over a non-ASCII glyph (e.g. `$é$`) —
not user-visible, but `strict: 'ignore'` would quiet it, worth folding into whichever loop next
touches `renderMath`. And an offset exactly on a second opener in `$a$$b$` previews the first
span, not the second — cosmetic, left alone unless it bites. Rung 4 itself (two `hoverTooltip`s
stacking, real KaTeX fonts under the CSP) is still owed and is the natural first check for
whoever runs this loop's card on a machine with a webview.

### Sprint 5 — The log parser

**Exit demo.** Twenty captured logs resolve to the right `file:line`, every one.

| ✓ | Loop | Size | Depends |
|---|---|---|---|
| [x] | S5.1 Tokenizer: unwrap 79-column lines, classify `!`, `l.NN`, warnings, `(`/`)` file events | L | S1.11 |
| [x] | S5.2 Paren-stack resolver: track the open file through interleaved output; fixtures for the known pathological cases | L | S5.1 |
| [x] | S5.3 Fixture harness: `crates/texlog/fixtures/<name>/{main.log,expected.json}`; a test per fixture, generated from the directory | M | S5.2 |
| [x] | S5.4 Twenty fixtures captured from real documents, including the torture document | M | S5.3 |
| [x] | S5.5 Rule engine: matcher trait, explanation, optional fix; catalog as data, not code | M | S5.2 |

**Outcome (16 September 2026).** All five loops ticked and green — 66 `texlog` lib tests, 15
generated fixture tests, `cargo clippy -p texlog --all-targets -- -D warnings` clean, rest of the
workspace unaffected. **The exit demo as written — "twenty captured logs resolve to the right
`file:line`"** — cannot be performed, and this is a real gap rather than an oversight: no card in
this sprint's own table wires `tokenizer`/`resolver` (S5.1/S5.2) into `rules::diagnostics`
(S2.6's rule catalog, now S5.5's trait-based one). `Diagnostic` carries a `line` from TeX's own
`l.NN` claim — the same approximate number sprint 2 shipped with — and no `file` field at all.
S5.4's fixtures README names this explicitly (`lib.rs`'s own module doc has said as much since
S5.1), and this was a design call made without an architect back in S5.4, not one hidden here:
wiring the resolver into the rule catalog looked like it could grow past any one of these five
cards' own scope, so it was left for whichever loop rebuilds `rules.rs` on top of `tokenizer`/
`resolver` for real, which no sprint 5 or sprint 6 card currently names. **Open question for the
architect:** sprint 6's own table (`S6.1`, "Rules 6–40") assumes the six existing rules' shape
carries forward unchanged; if `file:line` resolution is meant to land before the twenty-error
torture document (S6.4), a card for that wiring needs adding — to sprint 6, or as a corrective
sprint-5 loop first. Twenty real fixtures (S5.4) and a resolver that already answers `file_at`
correctly (S5.2, including three files deep, S5.4's `nested-include`) are both ready and waiting
for it; nothing further needs capturing first.

**S5.1 (16 September 2026).** `[x]`: `cargo test -p texlog` 44 passed (20 new), `cargo clippy -p
texlog --all-targets -- -D warnings` clean, rest of the workspace still builds
(`cargo build --workspace --exclude preamble`). A pure library-crate loop with no webview
dependency at all, so there is no rung-4 gate to carry here, unlike every frontend loop since
S3.1 — `crates/texlog/src/tokenizer.rs` is a new module, additive only: `rules.rs`'s existing
`quick_errors`/`diagnostics` path (what the app calls today) is untouched, so nothing already
shipped changed behaviour this loop.

New captured fixture: `crates/texlog/fixtures/wrapped-file-open`, an `\input` of a file under a
deliberately long directory name, chosen to force the real bug DESIGN.md §5.2 names ("messages
wrap at 79 columns mid-word") rather than assume its shape. Real Tectonic output confirmed two
things worth a reader's attention:

1. **The wrap rule is per physical line, not per logical line, and the difference only shows up
   on a message long enough to wrap twice.** The obvious first implementation checked whether the
   *accumulated* logical line so far was exactly 79 characters before deciding to keep joining —
   which is wrong, and would silently truncate a message that wraps three or more times, because
   after the first join the running total is never 79 again. TeX counts columns from zero on
   every physical line it writes, so the correct check is on each raw physical line's own length.
   Caught by a test built for exactly this shape
   (`keeps_joining_across_a_message_long_enough_to_wrap_twice`) before it reached the fixture,
   not by the fixture itself — the real capture only exercises a single join.
2. **A coincidentally-79-character line and a genuine wrap are indistinguishable by this rule,
   and that is stated rather than hidden.** The fixture also captured atbegshi-ltx's own
   two-physical-line banner (73 then 27 characters) sitting right next to the real wrap (79 then
   10) — proof the heuristic does not fire on an ordinary short multi-line message, but not proof
   it never will on some other package's banner that happens to hit exactly 79. `unwrap_lines`'s
   own doc comment names the failure mode instead of pretending the heuristic is exact.

`(`/`)` classification is deliberately shallow: [`ParenEvent`] reports every paren's position and,
for a `(`, the candidate token after it, with no attempt to decide whether it is a real file
boundary or an incidental parenthetical (`(Unicode)`, `(HO)`) — a test pins exactly this
ambiguity (`an_incidental_parenthetical_is_reported_the_same_way_as_a_real_file_open`) as the
reason that decision is S5.2's own loop, with its own fixtures for the pathological cases the
card asks for, rather than guessed at here. Warning classification generalises the narrow scan
`rules.rs` has carried since S2.6 (its own doc comment names this exact gap: "the general warning
tokenizer is S5.1") to any `LaTeX Warning:`, `LaTeX Font Warning:`, or `Package <name> Warning:`
banner, by requiring `Warning: ` to appear within the first 40 characters of the line rather than
matching two hand-picked phrases.

Not done here, on purpose: wiring `tokenizer` into `rules.rs`/`diagnostics` (that rebuild is
S5.2's, once the paren stack exists to resolve a file against); one-click fixes (S6.2); and the
`expected.json` fixture harness (S5.3) `wrapped-file-open` does not yet have, since it is not a
rule-catalog fixture and carries no diagnostic to assert on yet.

**S5.2 (16 September 2026).** `[x]`: `cargo test -p texlog` 61 passed (17 new), `cargo clippy -p
texlog --all-targets -- -D warnings` clean, rest of the workspace still builds. `crates/texlog/
src/resolver.rs` walks `tokenizer`'s classified lines and treats `(`/`)` as a stack push/pop,
exposing `open_files` (the stack after each logical line) and `file_at` (the top of it at a given
line — the direct answer to "what file was this diagnostic in").

**The card asked for "fixtures for the known pathological cases," so three were captured before
any classification rule was written, the same discipline S2.6 and S5.1 both leaned on — and each
one overturned a rule that looked reasonable on paper:**

1. **Position does not distinguish a real file boundary from an incidental parenthetical.** The
   first attempt required a `(` to sit at the start of a line or right after another paren before
   trusting it. `fixtures/space-in-path/main.log` (an `\input` under a directory with a space in
   its name — this project's own working directory has one) opens `article.cls` mid-line, straight
   after a version string. Worse, an incidental one can sit at column 0: the same log's
   `(rerunfilecheck)             Checksum: ...` is a package echoing its own name at the very start
   of a line, not a file. Position was dropped entirely.
2. **A file extension is not reliably present either.** `fixtures/bare-input-no-extension/main.log`
   captures `\input{plainchapter}` (no extension given at the call site) being echoed as
   `(plainchapter)` — no extension at all, and lexically identical in shape to `(rerunfilecheck)`.
   There is no text-only rule that tells these apart; the only way to be sure is to check whether
   `plainchapter` is a real path in the project, which this crate is chartered to never do (it
   never reads a file). Stated as a real, accepted limitation in `resolver.rs`'s own doc comment,
   pinned by a test (`the_captured_bare_extensionless_input_resolves_to_its_parent_instead`) rather
   than hidden: a diagnostic inside a bare extensionless `\input` resolves to its parent file
   instead of itself, until a caller with the real file tree (the app, not this crate) can improve
   on it.
3. **What does work: a `/` anywhere, or a `.` followed by 1-4 letters running to the true end of
   the candidate.** Between them these two signals cover every real file these three fixtures (plus
   the five from S2.6/S5.1) load, and neither ever fires on an incidental parenthetical also
   captured (`(HO)`, `(DPC)`, `(Unicode)`) or on `fixtures/overfull-hbox/main.log`'s `Overfull
   \hbox (48.75pt too wide)` — a decimal number has a `.` too, but not immediately before the
   closing paren, so it is not mistaken for a `.pt` extension. Parens are additionally only walked
   on `LineKind::Text` lines, so a warning's own prose quoting a real filename in passing (rare, but
   possible) is never read as a file boundary either.

A leaf file that opens and closes again within one line (every `.sty`/`.clo` load in these
fixtures does exactly this) is invisible to `open_files`'s once-per-line snapshot, since it is
already popped again by the time that line's snapshot is taken — this is correct for the resolver's
actual job (a diagnostic is never on the same line as the file-open/close pair itself), but made
the `space-in-path` fixture's own test need the lower-level `apply` function directly rather than
`open_files`, which its own doc comment now explains.

Not done here, on purpose: wiring this into `rules.rs`/`diagnostics` (`rules.rs` still runs on
`quick_errors`, untouched); the `expected.json` fixture harness (S5.3, which the three fixtures
captured this loop do not yet have, matching `wrapped-file-open`'s own precedent); and any
project-aware disambiguation of the extensionless-`\input` case, which needs the app's own file
tree and so belongs above this crate's boundary, not inside it.

**S5.3 (16 September 2026).** `[x]`: `cargo test -p texlog` 66 passed (5 new), `cargo clippy -p
texlog --all-targets -- -D warnings` clean, rest of the workspace still builds and tests clean
except the two pre-existing, already-logged Linux-only failures (`preamble-lsp`'s pipe deadlock,
`preamble-synctex`'s checkout-path fixture — neither touched by this loop, both confirmed
unchanged in `bugs-issues-fixes.md`). No rung-4 gate: a pure library-crate loop, same as S5.1/S5.2.

`crates/texlog/build.rs` is new — the first build script in this workspace — and
`crates/texlog/tests/fixtures.rs` is the first integration test directory for this crate. What a
reader should take from the diff:

1. **The card's "generated from the directory" is literal, not figurative.** `build.rs` walks
   `fixtures/` at compile time and writes one `#[test] fn fixture_<name>()` per directory that
   carries both `main.log` and `expected.json` into a file under `OUT_DIR`, which
   `tests/fixtures.rs` pulls in with `include!`. Adding S5.4's twenty fixtures is meant to be
   dropping in two files per fixture, no Rust edited — checked by hand this loop: writing a
   fixture's `expected.json` wrong and re-running `cargo test` shows a normal named test failure
   (`fixture_missing_package`) with a diff, not a compile error or a silently-skipped case.
2. **The failure message writes its own fix.** `run_fixture` compares parsed `serde_json::Value`s
   rather than raw text — so `expected.json`'s key order and whitespace never matter — and a
   mismatch's panic pretty-prints exactly what the parser produced, ready to paste in as the new
   `expected.json`. That is how the five fixtures already captured for `rules.rs`
   (`undefined-control-sequence`, `broken-underscore`, `unbalanced-braces`, `missing-package`,
   `undefined-reference`) got their `expected.json` this loop: each started as `[]`, and the
   panic's own suggested JSON became the real file.
3. **Only fixtures with an `expected.json` are in scope, on purpose.** `wrapped-file-open`,
   `space-in-path`, `bare-input-no-extension`, and `overfull-hbox` exercise `tokenizer.rs` and
   `resolver.rs` (S5.1/S5.2), not `rules::diagnostics` — running the rule catalog against them
   would assert nothing meaningful, so `build.rs` skips any directory missing the file rather than
   erroring, and their own hand-written tests are untouched.
4. **`serde_json` is a dev-dependency only.** It is already a pinned workspace dependency (used by
   `src-tauri` and `preamble-lsp`), so this adds no new crate to the project, and it never reaches
   `texlog`'s own production code — the crate's "never read a file" rule (`lib.rs`) is about what
   ships, and a test harness reading fixtures off disk is the same thing every fixture test in this
   crate has always done with `include_str!`.

**S5.4 (16 September 2026).** `[x]`: `cargo test -p texlog` 77 passed (11 new: 15 generated
fixture tests, up from 5, plus one new hand-written resolver test), `cargo clippy -p texlog
--all-targets -- -D warnings` clean, rest of the workspace still builds. No rung-4 gate, same as
every loop in this sprint. Eleven new real Tectonic 0.17.0 captures bring
`crates/texlog/fixtures/` to twenty directories, the sprint's own exit-demo number; ten got an
`expected.json` through the S5.3 harness, one (`nested-include`) got a hand-written `resolver.rs`
test instead, matching the precedent the four S5.1/S5.2 tokenizer/resolver-only fixtures already
set. `crates/texlog/fixtures/README.md`'s table has the full list and what each one exercises.

1. **A real discovery that reshapes what "the torture document" can mean.** Every attempt at
   capturing two different `!` errors in one real log produced the same shape: Tectonic halts the
   run at the *first* one, full stop — confirmed across every erroring fixture captured this loop,
   not just asserted from one example. A classic engine's batch mode would keep going and log
   several; this bundled one does not. Warnings are unaffected (`torture`, this loop's fixture,
   holds two before the halting error), but S6.4's own card — "twenty-error torture document" —
   cannot mean twenty `!` errors surviving in a single compile. Recorded in the fixtures README
   rather than silently discovered and forgotten; S6.4 will need twenty separate compiles, or a
   document that is mostly warnings, whichever the architect prefers when that loop starts.
2. **Three of the ten new rule-catalog fixtures land in the long tail on purpose.**
   `misplaced-alignment-tab`, `undefined-environment`, and `tikz-unknown-key` are real, common
   LaTeX mistakes with no rule in the catalog yet (DESIGN.md §5.2 names alignment tabs explicitly;
   the other two are not on that list at all but are exactly the kind of thing a real author
   hits). Each gets `rule: null` in its `expected.json` — a captured pin of today's honest
   fallback, not a bug, and a ready-made worklist for S6.1's "rules 6–40."
3. **`tikz-unknown-key` is the first real capture of DESIGN.md §5.2's own wrapping problem inside
   the rule catalog's path, not just `tokenizer.rs`'s.** The pgfkeys error message is long enough
   to wrap at column 79 mid-word — `...and I am go` / `ing to ignore it...` — and `rules.rs` still
   runs on `quick_errors`, which (unlike `tokenizer::unwrap_lines`, S5.1) never undoes that wrap.
   So today's real `raw_message` for this fixture is the truncated half-word, and its
   `expected.json` pins that as current, honest behaviour — the concrete case that will need
   re-generating (via the same harness) once `rules.rs` is finally rebuilt on `tokenizer`/
   `resolver`, which `lib.rs`'s own module doc says has not happened yet.
4. **`nested-include` is a positive result, and the one real capture worth writing a dedicated
   assertion for.** `main.tex` → `\input{outer}` (no extension, so — like `bare-input-no-extension`
   — not recognised as a file) → `\input{chapters/inner}` (a `/`, so recognised despite also
   lacking an extension) → an undefined control sequence, three files deep. `resolver.rs`'s new
   test confirms `file_at` correctly returns `chapters/inner`, skipping straight past the
   unrecognised `outer` wrapper in the middle — real evidence that an intermediate file the
   resolver cannot name does not break resolution of the real file nested inside it, which no
   existing fixture (all one level deep) could have shown either way.
5. **`missing-closing-brace` exercises a fallback branch no fixture had reached before.** `$x^{2$`
   produces a real `! Missing } inserted.` whose message contains no `\command` for
   `trailing_command` to find, so `explain_unbalanced_braces` falls to its generic "a brace was
   never closed" wording — previously only reached by a hand-written log, never a real one.
6. **`diagnostics()` groups by scan, not by document order, and `torture` is the first real log
   with enough diagnostics for that to be visible.** Its citation warning appears before its
   reference warning in the source, but *after* the halting error in `diagnostics()`'s own output,
   because `diagnostics` runs `quick_errors` (the `!` scan) to completion before
   `undefined_reference_warnings` (the warning scan) even starts. Existing, deliberate, unchanged
   by this loop — `errors_and_warnings_from_one_log_come_back_together` already pinned the
   two-diagnostic case — but worth naming here since a three-diagnostic real capture is the first
   place it is genuinely visible rather than incidental.

**S5.5 (16 September 2026).** `[x]`: `cargo test -p texlog` 81 passed (66 lib, 4 new: two
fix-detection tests, a no-fix case, and a sweep asserting the other five rules stay `fix: None`;
plus 15 unchanged generated fixture tests), `cargo clippy -p texlog --all-targets -- -D warnings`
clean, rest of the workspace unaffected. `pnpm check` 0 errors, `pnpm vitest run` 284/284
(`src/lib/ipc.ts`'s `Diagnostic` mirror gained the new field, which meant fixing two test
fixtures that had gone stale the moment `fix` became a real, always-present key). No rung-4 gate:
a pure library-crate loop, same as every other loop this sprint. Net +180 lines in
`crates/texlog/src/rules.rs`.

1. **`Rule` is now a trait, `dyn`-dispatched, for a reason S5.1–S5.4 already made concrete rather
   than a hypothetical one.** The six rules in this catalog all need real logic (`explain_missing_
   dollar` picks between "subscript" and "superscript"; `explain_file_not_found` quotes a name)
   — but S6.1's next thirty-four will not all be that shape, and a trait is what lets a future
   rule that is pure data (a fixed prefix, a fixed sentence) sit in the same `CATALOG` slice as
   these six without forcing it through fn-pointer fields it does not need. `FnRule` is the one
   implementation this loop actually adds — a rename of the old `Rule` struct, now implementing
   the new trait by delegating to its own fields — so all six existing rules' behaviour is
   unchanged; every pre-existing test in `rules.rs` passed without modification.
2. **`&dyn Rule` needed no `Box`, no `Vec`, and no runtime allocation.** `CATALOG` stays a `const
   &[&dyn Rule]`, each entry a `&FnRule { .. }` literal — Rust promotes a constant struct literal
   borrowed inside a `const` initializer to `'static` storage automatically ("rvalue static
   promotion"), the same mechanism that already let the old `CATALOG` hold plain `Rule` values
   with no allocation. `&dyn Rule` is a *trait object*: a fat pointer of data plus a vtable of the
   trait's methods, which is what lets `CATALOG` hold different `Rule`-implementing types later,
   not that it does yet.
3. **`Fix` only has one real implementation, and the other five rules were checked, not assumed,
   to have none worth offering.** `fix_missing_dollar` — "Escape as `\_`" — is the one case in
   DESIGN.md §5.2's own worked example, and the only one this crate can compute at all: it never
   reads the `.tex` source (`lib.rs`'s own rule), so a `find`/`replace` pair on a literal known
   character is the limit of what it can describe without a byte offset it does not have.
   `explain_missing_dollar` and `fix_missing_dollar` now share one `detect_math_symbol` helper so
   the explanation and the fix can never name different symbols for the same error — previously
   two separate `if`/`else` chains that happened to agree. `only_missing_dollar_offers_a_fix_in_
   this_catalog` is a real test, not a comment asserting it: all five other rules, run through
   `explain`, are asserted to return `fix: None`.
4. **A field addition, not a new feature, rippled into two other layers, and both were caught by
   `pnpm check` rather than discovered at review.** `Option<Fix>` serialises to a `fix` key that is
   always present (`null` or an object, never omitted), so `src/lib/ipc.ts`'s `Diagnostic`
   mirror — which `src-tauri/src/compile.rs` already sends this struct through unchanged since
   S2.7 — went stale the moment the Rust field existed; left alone, the frontend's own type would
   have quietly lied about the shape of its own IPC payload. Fixed by adding `fix: Fix | null` to
   the interface and a matching `Fix` interface beside it, and updating the two test fixtures
   (`controller.test.ts`, `editor/diagnostics.test.ts`) `svelte-check` flagged as now missing a
   required field. Nothing in the UI reads `fix` yet — applying one is S6.2's job.

### Sprint 6 — v0.3 exit: the rule catalog

| ✓ | Loop | Size | Depends |
|---|---|---|---|
| [ ] | S6.1 Rules 6–40: the list in `DESIGN.md` §5.2, one fixture each | L | S5.5 |
| [ ] | S6.2 One-click fixes for the ten unambiguous cases, applied through the CRDT, undoable | M | S6.1 |
| [ ] | S6.3 Drawer v1: grouping, severity, filter, "raw log" always one click away | M | S2.7 |
| [ ] | S6.4 Torture document with twenty errors; exit demo recorded | S | S6.1 |
| [ ] | S6.5 `texlog` published as its own MIT crate | S | S6.1 |

### Sprint 7–8 — v0.4 bibliography

S7.1 `bib` parser crate (BibTeX and BibLaTeX, comments and `@string` preserved) · S7.2 `.bib`
watcher and project-wide index · S7.3 `\cite` completion with author/year/title · S7.4 DOI
content negotiation · S7.5 arXiv and ISBN · S7.6 paste-to-cite with deduplication · S8.1 Zotero
detection on port 23119 · S8.2 Better BibTeX collection linking · S8.3 health checks · S8.4
forty-reference exit demo · S8.5 `bib` published MIT.

### Sprint 9 — v0.5 speed

S9.1 benchmark corpus (eight documents, `DESIGN.md` §8) · S9.2 `.aux` hash convergence ·
S9.3 precompiled preamble with hash invalidation · S9.4 system TeX Live / MiKTeX detection and
per-project engine switching · S9.5 CI performance gate that fails the build.

### Sprint 10–11 — v0.6 sync

S10.1 snapshot-on-compile to a hidden ref (**first three days, before anything else**) · S10.2
libgit2 panel · S10.3 GitHub device flow to keychain · S10.4 repository creation, private by
default, explicit public confirmation · S11.1 one-action Sync with a sentence · S11.2 conflicts
as two paragraphs · S11.3 LFS prompt and oversize catch · S11.4 `latexdiff` review · S11.5
two-machine exit demo; GitLab and bare-remote CI test.

### Sprint 12–13 — v0.7 assistant

S12.1 provider abstraction (Anthropic Messages, OpenAI-compatible) with key in keychain · S12.2
**citation-fabrication guard with adversarial tests, written first** · S12.3 selection-scoped
diff-first actions with per-hunk accept · S13.1 whole-document context with cache breakpoint ·
S13.2 model fallback for unmatched errors, cached by log signature · S13.3 payload inspector ·
S13.4 no-network test: zero outbound requests with no key.

### Sprint 14–15 — v0.8 live

S14.1 y-websocket relay binary · S14.2 awareness, cursors · S14.3 comments on relative
positions · S15.1 reconnection and merge-on-rejoin · S15.2 session end commits · S15.3
two-author exit demo · decisions: code signing, relay hosting (`DESIGN.md` §10).

### Sprint 16 — v0.9 ship

S16.1 signed installers and updater · S16.2 companion GitHub Action · S16.3 opt-in crash
reporting · S16.4 accessibility pass · S16.5 docs, contributor guide, templates · S16.6 stranger
test.

---

## 4. Decisions taken at sprint 1

These close the sprint-1 items in `DESIGN.md` §10 and record layout choices the design left
open. Change them here and in `DESIGN.md` before changing code.

| Decision | Choice | Why |
|---|---|---|
| Frontend framework | **Svelte 5** (runes) | The design's recommendation; it does not compete with CodeMirror or pdf.js for DOM ownership. |
| Licence | **AGPL-3.0-only** for the app; `texlog` and `bib` crates **MIT** when extracted | As proposed in §10. `LICENSE` at the root; per-crate `license` fields. |
| Repository layout | Cargo workspace at the root: `src-tauri/` (app crate `preamble`), `crates/preamble-engine`, `crates/preamble-reconcile`, `crates/texlog` | Library crates test without a GUI, which is what makes agent loops fast; `texlog` starts as a crate so extraction at S6.5 is a rename, not a refactor. |
| Engine invocation | Subprocess, never the Tectonic crate | `DESIGN.md` §4.1 note 2. Cancel is `kill`. |
| Engine binary | Tauri `externalBin` sidecar, fetched by script, gitignored | A 30 MB binary does not belong in Git. CI fetches it. |
| PDF transport | Tauri asset protocol, scope widened at runtime to `.preamble/build` | `DESIGN.md` §4.1 note 1. |
| CRDT op indices | UTF-16 code units | Y.Text and JavaScript strings count in UTF-16; Rust strings do not. The reconciler converts so the frontend never has to. |
| File writes | Atomic: write `name.tex.tmp`, then rename | A crash mid-write can never leave a truncated manuscript (`DESIGN.md` §9, row 1). |
| Self-echo suppression | Content hash of the last write, not a time window | A time window races with slow disks; a hash cannot. |
| LSP type definitions | Hand-written subset in `src/lib/lsp-protocol.ts`, not `vscode-languageserver-types` | ~30 fields are used; a dependency tax on the learner-reader outweighs the benefit. |
| Where LSP diagnostics surface | **Gutter only** — `publishDiagnostics` never enters `app.compile.diagnostics`, the drawer, or `errorCount`/`warningCount` | Two cadences in one counter makes the status bar flicker per keystroke (§2 commitment 2, and §6's "never shout when nothing is wrong"). The drawer's contract under §5.2 is an *explained* sentence from the rule catalog; a raw TexLab string in that list is a raw log wearing a card, against commitment 3. |
| Maths preview and user macros | **KaTeX renders with no macro table in v0.2; a preamble `\newcommand` shows KaTeX's "undefined control sequence" line** | Resolving `\newcommand` needs the include graph (S4.1) walked to the preamble and a mini-parser; that is its own loop. Showing the honest message keeps §2 commitment 3 (never a raw log) and the popover cheap. Revisit when the rule catalog (Sprint 6) already parses preambles. |

---

## 5. What an agent must never do

Copied from `DESIGN.md` so it is in the file agents read first.

- Never show a raw log as the default. Show a sentence; keep the log one click away.
- Never write anything but plain `.tex`/`.bib`/`preamble.toml` into the source tree. Everything
  else goes under `.preamble/`.
- Never merge a dirty buffer with a changed file. Ask.
- Never add WYSIWYG rendering, a hosted service, a filestore, or a mobile layout. §1.3.
- Never make a feature depend on an API key or the network. Every AI path has a non-AI path.
- Never let a `\cite` key the project's `.bib` does not contain reach the buffer from a model.
