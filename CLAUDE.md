# Abstract-Tex — agent conventions

Local-first LaTeX editor. Tauri 2 + Rust core, Svelte 5 + CodeMirror 6 + Yjs + pdf.js frontend,
Tectonic as a bundled subprocess. Read `0.1/DESIGN.md` for architecture and non-goals, and
`0.1/SPRINTS.md` for the loop you are working on. Both are authoritative; code follows them.

## Layout

```
0.1/                            design doc, sprint plan (docs for the v0.1 line)
Cargo.toml                      workspace root
crates/abstract-tex-engine/     Engine trait + Tectonic subprocess        (no Tauri, testable alone)
crates/abstract-tex-reconcile/  text diff → CRDT ops, proptest            (no Tauri, testable alone)
crates/abstract-tex-includes/   \input/\include graph
crates/abstract-tex-synctex/    .synctex.gz parser, forward/inverse search
crates/abstract-tex-lsp/        TexLab process + JSON-RPC bridge
crates/abstract-tex-sidecar/    finds bundled helper binaries
crates/texlog/                  TeX log parser + rule catalog (MIT, published separately)
crates/texbib/                  .bib parser, health checks, acquisition (MIT, published separately)
src-tauri/                      the Tauri app crate `abstract-tex`: project model, orchestrator, watcher, commands
src/                            Svelte 5 frontend (lib/ipc.ts is the only place that calls invoke)
fixtures/                       real documents used by tests and manual smoke scripts
scripts/                        fetch-tectonic.mjs and other tooling
```

## Commands

```
pnpm install                 # once
pnpm fetch-engine            # downloads Tectonic for this host into src-tauri/binaries/
pnpm verify                  # cargo test + clippy + svelte-check + vitest  (the loop gate)
pnpm tauri dev               # run the app
cargo test -p <crate>        # one crate
cargo test -p abstract-tex-engine -- --ignored   # real Tectonic build of fixtures/minimal (network on first run)
```

## Working solo

No subagents for this project. The main agent does it all: architecture and design calls
(what used to be `architect`), implementation (`builder`), and review (`reviewer`) — all in
one session, to keep the context window small and avoid re-deriving context across spawns.
Scribing (updating `SPRINTS.md`, writing the commit message) is mandatory at the end of a
session rather than a dedicated role — never skip it.

If a task genuinely calls for a different profile (e.g. a second opinion, a fresh read on a
design question), ask for it explicitly in a separate session rather than spawning a subagent
mid-session.

On this Windows machine cargo lives at `%USERPROFILE%\.cargo\bin`; new shells may need it on PATH.

## Rust is written for a learner

The maintainer is learning Rust through this codebase and will read every line. That changes
what good code means here:

- Name things for what they are, not for brevity. `build_in_flight`, not `cur`.
- The first time a construct appears in a file (a trait, `?`, `Arc<Mutex<_>>`, `select!`,
  lifetimes, `async_trait`), a short comment says what it does and why it was chosen over the
  simpler thing. Later uses get no comment. Match comment density to novelty.
- Prefer a few more lines that a reader can follow over a clever one-liner.
- Every module starts with a doc comment (`//!`) saying what it owns and what it must never do.
- Errors: `thiserror` enums in library crates, `anyhow` only at the app edge. Explain the split
  once, in `crates/abstract-tex-engine/src/lib.rs`.
- Keep diffs followable. A loop is one commit; if a commit would exceed ~400 lines of Rust, the
  loop was too large.

## Rules that settle arguments (from DESIGN.md §2)

1. Plain files are the truth. `.tex` on disk is authoritative; the CRDT is session-only.
2. Latency is the feature: <16 ms keystroke, <2 s p95 warm recompile.
3. Never show a raw log by default.
4. Zero setup to first PDF.
5. Keyboard first.
6. Every AI feature has a non-AI path.

Non-goals (DESIGN.md §1.3): no WYSIWYG, no TeX distribution, no cloud service, no Zotero/Git
replacement, no semantic PDF library, no mobile.

## Frontend conventions

- Svelte 5 runes (`$state`, `$derived`, `$effect`); shared state in `src/lib/*.svelte.ts`.
- `src/lib/ipc.ts` owns every `invoke`/`listen`; components never import `@tauri-apps/api`.
- Plain CSS with custom properties in `src/app.css`; no CSS framework.
- CodeMirror and pdf.js own their DOM subtrees. Svelte mounts a container and stops.
- Vitest tests live beside the module: `foo.ts` → `foo.test.ts`.

## Bug ledger

Every bug found during development — while building, reviewing, or just running the app —
gets an entry in `bugs-issues-fixes.md` at the repository root, the moment it is found, not
after it is fixed. This is how a found problem survives to be fixed instead of being
forgotten between sessions or, worse, shipped. Update the entry's status in place once
resolved; never delete one. A `Won't fix` still needs a reason a future reader would accept.
This applies to every agent role, not only the builder — a reviewer or the maintainer
finding something while doing something else still logs it there before moving on.

## Commit messages

`S<sprint>.<loop>: <what changed>` on the first line, then a body that says what a reader should
learn from the diff, if anything. No attribution lines.
