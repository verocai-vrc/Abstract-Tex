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
| [ ] | S2.2 Compile feedback: spinner and elapsed time in the status bar, Tectonic package-fetch progress surfaced from stderr, failure count in the drawer, raw log behind one click | M | S1.5, S1.11 | smoke §3 |
| [ ] | S2.3 Multi-document editing: open any `.tex`/`.bib` from the tree, tabs, per-file Y.Doc, save-all on compile | M | S1.9 | `pnpm test -- documents` |
| [ ] | S2.4 Keyboard: `Ctrl S` save, `Ctrl B`/`F5` compile, `Ctrl O` open folder, `Ctrl P` quick-open file (seed of the palette) | S | S2.3 | smoke §4 |
| [~] | S2.5 Fixtures: `fixtures/paper` (a real 8-page article), `fixtures/broken` (underscore, undefined control sequence, missing brace), `fixtures/paper/SMOKE.md` manual script | S | — | `cargo test -p preamble-engine -- --ignored` |
| [x] | S2.6 First five diagnostic rules in `texlog`: undefined control sequence, missing `$`, missing `}`/runaway argument, undefined reference/citation, file not found — each with a sentence and a fixture (brought forward from sprint 5 so the demo is honest) | M | S1.11 | `cargo test -p texlog` |
| [ ] | S2.7 Diagnostics drawer v0: sentence per error, click jumps to line, gutter marker; raw log one click away | M | S2.6 | smoke §5 |
| [ ] | S2.8 CI: 3-OS matrix runs `pnpm verify`; Linux installs WebKitGTK deps; engine fetched in CI; `cargo build` of the Tauri app on all three | M | S1.1 | green Actions run |
| [~] | S2.9 Name decision recorded in `DESIGN.md` §10; README moved to repository root with a real *Building* section; installer smoke on Windows via `pnpm tauri build` | S | — | installer launches and opens `fixtures/paper` |

**Outcome.** *(in progress)*

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
| [ ] | S3.1 TexLab sidecar fetch script and `externalBin`; Rust owns process lifecycle and stdio (`DESIGN.md` §4.1 note 3) | M | S1.3 |
| [ ] | S3.2 JSON-RPC bridge: Rust ↔ TexLab over stdio, Tauri events ↔ TypeScript; request/response correlation, restart on crash | L | S3.1 |
| [ ] | S3.3 CodeMirror LSP adapter: completion, hover, diagnostics, go-to-definition, document symbols | L | S3.2 |
| [ ] | S3.4 SyncTeX forward: cursor → PDF highlight, parsed from `.synctex.gz` in Rust | M | S1.10 |
| [ ] | S3.5 SyncTeX inverse: click in PDF → `file:line`, opening the file if needed | M | S3.4, S2.3 |
| [ ] | S3.6 LSP settings passthrough from `preamble.toml` (root file, build dir) | S | S3.2 |

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
