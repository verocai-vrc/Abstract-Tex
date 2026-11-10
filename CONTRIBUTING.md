# Contributing to Abstract-Tex

Thank you for thinking about it. This is a small project run by one maintainer, so this page is
short on ceremony and long on the things that will save you a round trip.

## Before you start

- **Read the two documents that settle arguments.** [`0.1/DESIGN.md`](0.1/DESIGN.md) holds the six
  commitments (plain files are the truth; latency is the feature; no raw log by default; zero
  setup; keyboard first; every AI feature has a non-AI path) and the list of things the project
  will not do (§1.3: no WYSIWYG, no TeX distribution, no cloud service, no replacement for Zotero
  or Git, no mobile). A change that breaks one of them will be declined however well it is built.
- **For anything bigger than a fix, open an issue first.** Say what you want to change and why.
  It is much cheaper to find out that a feature is already planned, or deliberately excluded,
  before you write it. [`0.1/SPRINTS.md`](0.1/SPRINTS.md) is the plan; its last section lists
  cards decided but not yet scheduled.
- **Small fixes are welcome straight away**: a wrong sentence in an error explanation, a rule
  for a LaTeX error the catalogue misses, a document that builds wrongly, a typo.

## Setting up

Follow *Quick start* in the [README](README.md). Then:

```sh
pnpm verify        # the whole gate: cargo fmt --check, cargo test, clippy -D warnings, svelte-check, Vitest
cargo test -p <crate>
pnpm tauri dev
```

`pnpm verify` is what CI runs on Linux, Windows and macOS. Run `cargo fmt --all` before you commit
Rust, or it fails on formatting alone. Some tests are marked `--ignored` because they run the real
Tectonic and need the network on first use:
`cargo test -p abstract-tex-engine -- --ignored`.

## How the code is laid out

[`DEVELOPMENT.md`](DEVELOPMENT.md) has the repository layout and the architecture. Two rules you
will meet immediately:

- **Every crate that can be tested without a window is its own crate** (`abstract-tex-engine`,
  `-reconcile`, `-includes`, `-git`, `-comments`, and so on). New logic belongs there, with its
  tests, not in `src-tauri`, which should only connect it to the window.
- **In the frontend, `src/lib/ipc.ts` is the only file that calls `invoke` or `listen`.**
  Components do not import `@tauri-apps/api`. Shared state lives in `src/lib/*.svelte.ts`, and a
  Vitest test sits beside the module it tests.

## Code style

- **Rust is written to be read by someone learning it.** The maintainer is learning Rust through
  this codebase and reads every line. Name things for what they are (`build_in_flight`, not
  `cur`); the first time a construct appears in a file (a trait, `?`, `Arc<Mutex<_>>`, lifetimes)
  a short comment says what it does and why; prefer a few more lines a reader can follow to a
  clever one-liner; start every module with a `//!` comment saying what it owns and what it must
  never do.
- **Errors:** `thiserror` enums in library crates, `anyhow` only at the application's edge.
- **Svelte 5 with runes** (`$state`, `$derived`, `$effect`); plain CSS with custom properties in
  `src/app.css`, no CSS framework. CodeMirror and pdf.js own their DOM; Svelte mounts a container
  and stops.
- **Text colours must keep 4.5:1 contrast.** `src/lib/contrast.test.ts` checks the theme in both
  palettes.
- **Nothing may reach the network without a person asking.** `src-tauri/tests/no_silent_network.rs`
  lists the few crates that can, and why. Adding to that list is a decision the pull request has
  to defend.

## Tests are part of the change

Write them in the same commit as the code. For a bug fix, write the test that fails first. For
anything a person sees, run the app (`pnpm tauri dev`) and look at it: a passing test suite has
not once been enough to know that a window works. On Linux without a screen,
`scripts/xvfb-app.sh` and `scripts/xvfb-ui.py` run and photograph the real app on a virtual
display; their headers say how.

## Bugs you find on the way

Every bug found while working goes in [`bugs-issues-fixes.md`](bugs-issues-fixes.md) the moment you
find it, not after you fix it, and its entry is updated in place, never deleted. A bug you do not
fix is still worth an entry.

## Pull requests

- Keep a pull request to one change a reviewer can hold in their head. If it grows past roughly
  400 lines of Rust, it is probably two.
- Say what a reader should learn from the diff, not only what it does.
- `pnpm verify` must pass. Say so, and say what you ran by hand that it does not cover.
- Commit messages are `S<sprint>.<loop>: what changed` for work from the plan; for anything else a
  plain imperative sentence is fine, and the maintainer may reword it.

## Reporting a security problem

Please do not open a public issue for something that could harm someone who runs the app (for
example a way to make it write outside a project folder, or to send data it should not). Contact the
maintainer privately instead (the address on their GitHub profile) and give them a reasonable time
to fix it before it is made public.

## Licence

Abstract-Tex is licensed under the [GNU Affero General Public License v3.0](LICENSE). The `texlog`
and `texbib` crates are MIT, so a contribution to one of them is MIT. By opening a pull request you
agree that your contribution is licensed the same way as the code it changes.
