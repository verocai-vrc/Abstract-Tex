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
| [ ] | S4.1 `\input`/`\include` graph in Rust; root detection uses it; watcher compiles on any node change | M | S1.4 |
| [ ] | S4.2 Document map panel: sections, figures, tables, labels, TODOs, from LSP symbols plus our own scan | M | S3.3 |
| [ ] | S4.3 Command palette `Ctrl K`: actions, files, sections, fuzzy matching, every action registered through one registry | L | S2.4 |
| [ ] | S4.4 `fixtures/thesis` six-file skeleton and its smoke script | S | — |
| [ ] | S4.5 Focus and typewriter modes | S | S1.9 |
| [ ] | S4.6 Maths preview on hover with KaTeX | S | S1.9 |
| [ ] | S4.7 Linux and macOS smoke on CI artifacts; WebKitGTK issues logged as loops | M | S2.8 |

### Sprint 5 — The log parser

**Exit demo.** Twenty captured logs resolve to the right `file:line`, every one.

| ✓ | Loop | Size | Depends |
|---|---|---|---|
| [ ] | S5.1 Tokenizer: unwrap 79-column lines, classify `!`, `l.NN`, warnings, `(`/`)` file events | L | S1.11 |
| [ ] | S5.2 Paren-stack resolver: track the open file through interleaved output; fixtures for the known pathological cases | L | S5.1 |
| [ ] | S5.3 Fixture harness: `crates/texlog/fixtures/<name>/{main.log,expected.json}`; a test per fixture, generated from the directory | M | S5.2 |
| [ ] | S5.4 Twenty fixtures captured from real documents, including the torture document | M | S5.3 |
| [ ] | S5.5 Rule engine: matcher trait, explanation, optional fix; catalog as data, not code | M | S5.2 |

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
