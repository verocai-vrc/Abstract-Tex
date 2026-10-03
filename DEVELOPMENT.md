# Developing Abstract-Tex

The [README](README.md) is the public face of the project: what it does and how to run it. This
file is for people working on the code: where the plans live, how the pieces fit, and how to
build and test it.

## Where things are written down

| File | What it holds |
| --- | --- |
| [`0.1/DESIGN.md`](0.1/DESIGN.md) | Architecture, the six commitments that settle arguments, non-goals, stack rationale, the milestone roadmap, risks and open decisions. Authoritative: code follows it. |
| [`0.1/SPRINTS.md`](0.1/SPRINTS.md) | The sprint plan as numbered loops (`S<sprint>.<loop>`), each with a verification command and a done-when clause, and the recorded outcome of every loop run so far. The live project status is its §3. |
| [`bugs-issues-fixes.md`](bugs-issues-fixes.md) | The bug ledger. Every bug found gets an entry the moment it is found, and the entry is updated in place, never deleted. |
| [`user-suggested-features.md`](user-suggested-features.md) | Feature requests from early testers, not yet planned. |
| [`CLAUDE.md`](CLAUDE.md) | Conventions for anyone working here, human or agent: code style, error handling, commit messages. |
| [`fixtures/corpus/README.md`](fixtures/corpus/README.md) | The golden corpus of eight real documents, what building them found, and the measured build times. |

## Status in detail

Sixteen two-week sprints, each ending in something usable. Every milestone has one exit
criterion that must be demonstrated on a real document before it counts as done
(`DESIGN.md` §7). **v0.3 is the release where this stops being a hobby editor.**

| Version | Sprints | Theme | Exit criterion |
| --- | --- | --- | --- |
| v0.1 | 1–2 | It compiles | Edit a real paper, see the PDF update, on a machine with no TeX |
| v0.2 | 3–4 | It navigates | Keyboard-drive a six-file thesis; click the PDF and land on the source |
| v0.3 | 5–6 | **It explains itself** | A 20-error torture document: every error gets the correct file, line and a plain-language explanation |
| v0.4 | 7–8 | It cites | Assemble a 40-reference paper without opening a browser |
| v0.5 | 9 | It's fast | p95 warm recompile < 1.2 s on a 60-page TikZ/biblatex thesis, gated in CI |
| v0.6 | 10–11 | It syncs *(storage before convenience)* | Write on one machine, sync, clone on a second, lose nothing |
| v0.7 | 12–13 | It assists | No fabricated citation can reach the buffer under adversarial prompting |
| v0.8 | 14–15 | It shares, live | Two authors edit one paragraph across a network; one goes offline and loses nothing |
| v0.9 | 16 | It ships | A stranger installs it on a clean machine and compiles their paper without reading anything |
| v1.0 | — | Public release | Announced |

As of sprint 9, the features of v0.1–v0.4 are in the code, but several of their loops are still
marked `[~]`: the code and its tests are done, and a manual smoke run in the app or an exit demo
is outstanding. [`0.1/SPRINTS.md`](0.1/SPRINTS.md) §3 says which ones and why.

Sixteen sprints is roughly seven and a half months at full time. Solo and part-time, plan for
double. This project is also how its maintainer is learning Rust, so the early sprints ran
slower than that baseline, which is expected. Sprint numbers, not dates, are the unit of
commitment.

## Architecture

The compile loop in the README is the whole application; everything else is a panel attached to
it. `DESIGN.md` §3 describes the loop and §5 the seven subsystems that need real design
(orchestrator, diagnostics, editor, bibliography, assistant, document model, storage).

| Layer | Choice | Why |
| --- | --- | --- |
| Shell | Tauri 2 | Rust core, system WebView; ~10 MB bundle against Electron's ~150 MB |
| Core language | Rust | Process orchestration, filesystem, log and bibliography parsing, Git |
| Interface | TypeScript + Svelte 5 | Compile-time, no virtual DOM; does not fight CodeMirror for the DOM |
| Editor | CodeMirror 6 | Holds a 400-page thesis without stutter; transactional state |
| Language intelligence | TexLab (LSP) | A mature LaTeX language server, also in Rust |
| Engine | Tectonic, bundled | Self-contained; makes "zero setup to first PDF" possible |
| Document model | Yjs + y-codemirror.next | A CRDT from sprint 1, because it cannot be retrofitted cheaply for live collaboration |
| PDF view | pdf.js | Its text layer gives search, selection and SyncTeX coordinate mapping |
| Storage and history *(v0.6)* | libgit2 + GitHub API | Real Git; the remote doubles as the filestore |
| AI *(v0.7)* | Bring your own key | Anthropic Messages API or any OpenAI-compatible endpoint |

The full rationale, including what was rejected and why, is in `DESIGN.md` §4.

## Repository layout

```
0.1/                            design document and sprint plan for the v0.1 line
crates/abstract-tex-engine/     Engine trait, Tectonic and latexmk subprocesses (no Tauri, testable alone)
crates/abstract-tex-reconcile/  text diff → CRDT operations, with property tests
crates/abstract-tex-includes/   the \input/\include graph
crates/abstract-tex-synctex/    .synctex.gz parser, forward and inverse search
crates/abstract-tex-lsp/        TexLab process and JSON-RPC bridge
crates/abstract-tex-git/        the project's Git repository on libgit2: status, commit, push, sync, merge
crates/abstract-tex-github/     GitHub device-flow sign-in, repository creation, the keychain token
crates/abstract-tex-snapshot/   a snapshot of the project on every successful compile, as real Git objects
crates/abstract-tex-latexdiff/  a latexdiff-marked-up document between two Git revisions
crates/abstract-tex-sidecar/    finds the bundled helper binaries
crates/texlog/                  TeX log parser and rule catalog (MIT, published separately)
crates/texbib/                  .bib parser, health checks, acquisition (MIT, published separately)
crates/texwords/                prose word counts and section lookup for a LaTeX file
src-tauri/                      the Tauri app crate `abstract-tex`: project model, orchestrator, watcher, commands
src/                            Svelte 5 frontend (src/lib/ipc.ts is the only place that talks to Rust)
fixtures/                       real documents used by tests and manual smoke scripts
scripts/                        sidecar download scripts and other tooling
```

## Building and testing

Prerequisites and first run are in the README's [Quick start](README.md#quick-start). The
commands you will use day to day:

```sh
pnpm install          # frontend dependencies
pnpm fetch-sidecars   # Tectonic 0.17.0 and TexLab 5.26.0 for this host, into src-tauri/binaries/
pnpm tauri dev        # build the Rust core, start Vite, open the window
scripts/dev.sh         # the same, from a terminal inside the VS Code snap (clears its GTK variables)
pnpm verify           # the gate: cargo test + clippy -D warnings, svelte-check, Vitest
cargo test -p <crate> # one crate
cargo test -p abstract-tex-engine -- --ignored                               # build a fixture with the real engine
cargo test -p abstract-tex-engine --test corpus -- --ignored --nocapture     # build and time the golden corpus
```

Things that trip people up:

- **Fetch the sidecars before any `cargo` command.** `tauri_build` fails the app crate's build if
  the Tectonic or TexLab binary for the host is missing from `src-tauri/binaries/`.
- **Run `pnpm build` once before `cargo build -p abstract-tex` or `cargo test --workspace` on a
  fresh clone.** The app crate embeds `dist/` at compile time and fails if it does not exist.
- **`cargo build --release` is not a release build.** A binary built that way still loads the
  Vite dev server at `localhost:1420` and shows an error page when Vite is not running. Use
  `pnpm tauri build`.
- **`ABSTRACT_TEX_OPEN` needs an absolute path.** A relative one resolves against `src-tauri/`,
  where the app process runs, and silently opens nothing.
- **The engine downloads packages on a document's first build**, so real-engine tests need a
  network the first time. On a machine where Tectonic cannot resolve DNS by itself, run
  `python scripts/dev-proxy.py` and set `HTTPS_PROXY=http://127.0.0.1:3128`.
- **Shell commands run in the build folder, not the project folder.** With shell escape allowed,
  `\write18` (and `minted` through it) runs with the build folder as its working directory, on
  either engine, so its output stays out of the source tree. The cost: a command handed a
  project-relative path — `\write18{cat code/x.py}` — finds nothing. There is no general fix; it
  is a known limitation (ledger, *Won't fix*).
- **Turning shell-escape consent on or off makes the next latexmk build a full one.** The consent
  changes the engine's command line, so a system-engine build starts from scratch once. Expected,
  not a bug (ledger, *Won't fix*).

[`.github/workflows/verify.yml`](.github/workflows/verify.yml) runs on Linux, Windows and macOS
for every push: it fetches the sidecars, builds the frontend, runs `pnpm verify` and the
real-engine fixture build, and builds the app crate.

## Working on the code

- Work is done in loops from `0.1/SPRINTS.md`, one commit per loop, with the message
  `S<sprint>.<loop>: <what changed>`.
- Rust here is written for a reader who is learning it: descriptive names, a short comment the
  first time a construct appears in a file, and a `//!` doc comment on every module. `CLAUDE.md`
  has the full list.
- Any bug you find goes into `bugs-issues-fixes.md` straight away, even if you are in the middle
  of something else.

## Licensing

The application is AGPL-3.0, which prevents a proprietary hosted fork, the specific threat for a
project like this. The reusable libraries, `crates/texlog` and `crates/texbib`, are MIT so the
wider TeX ecosystem can use them, and each carries its own `LICENSE` and `README.md`. Both are
packaged and pass `cargo publish --dry-run`. The actual `cargo publish` needs the maintainer's
crates.io token and has not been run yet.
