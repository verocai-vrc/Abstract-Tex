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

- **`credentials()` may offer a rejected token for ever.** (3 Oct 2026, suspected while building
  S11.5a) libgit2 calls a credential callback again after the server refuses what it returned, until
  the callback answers with an error. `abstract_tex_git::credentials` returns the same token every
  time it is asked, so a revoked or wrong token would not produce "authentication failed" but a
  fetch, push or clone that never finishes. **Not confirmed**: every test remote here is a local
  path, which never asks for a credential at all. What settles it: S11.8's smart-HTTP bare remote
  with a deliberately wrong token. The fix, if it loops, is a flag so the token is offered once.
  **Open.**

- **A comparison's "Raw output" is the engine's stderr, not its `main.log`.** (3 Oct 2026, S11.4d)
  The drawer's comparison section (A7: "raw log one click away") can only show what the
  `compile-diff` `finished` event carries, which is `stderr`. `read_log` reads the *live* build's
  log, and nothing reads a log by path, so a comparison that fails inside TeX shows the engine's
  last lines and not the full transcript. The diagnostics cards (from `texlog`) are unaffected.
  What closes it: a Rust command that reads `<comparison folder>/build/main.log`, or `logPath`
  from the event passed to a generic read — a small card, noted here rather than widened into
  S11.4d (which has no Rust in it). **Open.**

- **Leaving a comparison does not cancel its build.** (3 Oct 2026, S11.4d) *Back to live PDF*, the
  toolbar's × and `Esc` make the frontend ignore the comparison's answer and events, but no
  command asks the diff lane to stop, so a build in flight finishes unseen (and the next
  comparison cancels it, as S11.4c's lane always does). Harmless except for the CPU a thirty-second
  compile of a thesis takes. Closing it needs a `cancel_comparison` command. **Open**, low priority.

- **S9.9's draft PDF was probably never loadable in the PDF pane: it sits outside the only folder
  the asset protocol was allowed to serve.** (2 Oct 2026, found reading `open_project` while
  building S11.4c) `open_project` allowed `.abstract-tex/build/` and nothing else, and
  `tauri.conf.json`'s own scope is empty. A draft is written to `.abstract-tex/draft/build/main.pdf`,
  so the pane's `asset://` request for it would be refused. The status bar would still say
  "showing a draft", over a PDF that never arrived. Never seen, because S9.9's rung 4 was never
  walked, and every test of the draft stops at the event. S11.4c's comparison PDF, under
  `.abstract-tex/latexdiff/`, would have hit the same wall.
  **Fix landed in S11.4c (2 Oct 2026):** the scope is now the whole `.abstract-tex/` folder
  (`Project::state_dir`), which holds nothing the author wrote. **Still Open** until a draft is
  actually seen on screen: the Windows smoke campaign after S11.4d checks it (`fixtures/thesis`,
  edit a chapter, watch for the draft).

- **A server-side push rejection (`GitError::PushRejected`) has never actually been triggered by a
  test.** (1 Oct 2026, S11.1a) `push`'s `push_update_reference` callback is where libgit2 reports
  a refusal a real server makes — a GitHub branch protection rule, a pre-receive hook — as opposed
  to a stale local branch, which libgit2 catches itself before sending anything and which *is*
  tested. Confirmed by writing a `pre-receive` hook into the local bare repository every other test
  here uses, and that hook was never invoked: libgit2's local transport does not run server-side
  hooks the way a real `git-receive-pack` subprocess would, so nothing short of a real GitHub
  repository can exercise this path. Believed correct from the libgit2 source, not yet proven.
  What closes it: a rung-3 test against a real repository once the OAuth app exists (same
  blocker as the entry below), pushing to a branch with protection turned on.
  **Re-dated (2 Oct 2026, design interview B1):** the OAuth app is registered at the start of
  S11.8, so this closes there at the earliest.

- **Nobody has registered a GitHub OAuth app, so sign-in cannot be finished by code alone.** (29
  Sep 2026, S10.4a) The device flow needs a client id from an OAuth app registered on GitHub —
  public, not secret, which is why it ships in the binary — and Abstract-Tex has none. The crate
  reads it from `ABSTRACT_TEX_GITHUB_CLIENT_ID` at build time (overridable at runtime) and says
  "signing in to GitHub is not configured in this build" when it is absent, so nothing crashes
  and nothing pretends. What is left for the maintainer, once: create the OAuth app at
  github.com/settings/developers with *Device flow* enabled, put its client id in the release
  build's environment, and record it in `DESIGN.md` §10 with the other ship-time decisions. Until
  then S10.4b's panel can be exercised only against the fake GitHub in
  `crates/abstract-tex-github/tests/`, and the exit demo for v0.6 cannot be performed.
  **Deferred (2 Oct 2026, design interview B1):** registered at the start of S11.8 (the HTTP-remote
  CI and exit-demo card); owner and name are decided then. Until it exists nothing can push to an
  authenticated remote from the app — the device-flow token is the only credential
  `abstract-tex-git` offers. `DESIGN.md` §10 carries the row.

- **The graph's word counts are rebuilt a whole page at a time, and nothing caches them.** (29
  Sep 2026, found measuring S10.3c) `log` computes each row's `word_delta` from a tree diff and a
  prose scan of every changed `.tex` file, so one page of 200 rows costs ~111 ms on this machine —
  measured in `crates/abstract-tex-git/tests/against_real_git.rs`, over a 300-commit repository
  whose files grow to 20 KB. S10.3c already removed the bad case (the panel re-reads the graph
  only when `HEAD` moves, not on every save), so the cost is paid on opening a project, on each
  commit, and on *Show more* — each of them once, asynchronously, with nothing on screen waiting
  for it. It is logged because it scales with the page, not with what changed: a commit that
  touched one comma still re-prices all 200 rows, and an author with a 2 MB chapter would feel
  it. The fix, if it is ever needed, is a `commit id → delta` map in `AppState` — a commit's own
  delta can never change — which was deliberately not written yet because the `HEAD` comparison
  made it unnecessary and a cache is state that can go stale.

- **Nothing measures a system engine's speed: `latexmk` builds report zero passes and always
  `full`.** (29 Sep 2026, noted while writing S9.12) `BuildSteps { single_passes: 0, full: true }`
  is what `latexmk.rs` returns for every build, because latexmk decides its own rerun loop and
  does not say how many passes it ran. That is honest, but it means S9.2's timing harness and
  S9.5's `THRESHOLD_MS` performance gate — DESIGN.md §8's "fail the build rather than warn" —
  cover the bundled Tectonic only. A system engine could get arbitrarily slower between releases
  with nothing to catch it, and the warm/cold distinction the whole of sprint 9 is built on is
  invisible there. Not a regression and not urgent (Tectonic is the default, rule 4), but the
  gate's own doc comment claims "one ceiling per corpus document" without saying "on one engine".
  Worth a card if sprint 10 leaves room: either parse latexmk's `Run number N of rule` lines,
  which it prints with `-verbose`, or gate on wall-clock alone for system engines.
  **Decided (2 Oct 2026, design interview E6):** a wall-clock-only ceiling for system engines in CI
  where one is installed, with S9.5's doc comment narrowed in the same card (*Unplaced cards*,
  `SPRINTS.md`).

- **`latexminted` 0.6.0, which minted v3 needs, crashes on this machine's Python 3.14.** (29 Sep
  2026, S9.12) `latexminted --version` dies in `argparse` with `ArgParser.__init__() got an
  unexpected keyword argument 'color'` — Python 3.14 changed `add_parser`'s signature. minted
  then reports its generic "minted v3+ executable is not installed or is not added to PATH; or
  MiKTeX is being used with -aux-directory or -output-directory without setting a
  TEXMF_OUTPUT_DIRECTORY environment variable", which sent S9.8's investigation down the MiKTeX
  path for an hour. Not ours to fix — upstream `latexminted`, or this machine's TeX Live — but
  worth the entry twice over: it is why S9.12 verified the shell-escape fix with a plain
  `\write18` fixture rather than with minted, and it is why the corpus's `minted` document
  cannot be built here under `latexmk` even though the bundled Tectonic (whose bundle carries
  minted v2, which shells straight out to `pygmentize`) builds it fine.

- **`reqwest` cannot resolve DNS from inside a Rust-compiled process on this machine, though
  `curl.exe` resolves the identical hostname instantly in the same shell.** (S7.5, builder, 21
  Sep 2026, found running the `#[ignore]`d live-network tests for all three `texbib::acquire`
  sources) `cargo test -p texbib --features acquire -- --ignored` fails all three real-network
  tests — `doi::tests::the_real_doi_org_resolves_a_known_doi` (S7.4, unmodified by this loop),
  and this loop's own `arxiv::tests::the_real_arxiv_resolves_a_known_paper` and
  `isbn::tests::the_real_openlibrary_resolves_a_known_isbn` — every one with the same
  `reqwest::Error { kind: Request, source: ConnectError("dns error", Os { code: 11001, ... }) }`
  once printed with `{:?}` instead of the `Display` impl `DoiError`/`ArxivError`/`IsbnError`
  collapse it into (a one-off standalone binary using `reqwest` directly, built and deleted for
  this diagnosis, is what surfaced the real `os error 11001` — "Este host não é conhecido" /
  "this host is not known" — underneath the generic "error sending request" string the crate's
  own tests would otherwise report). In the same shell, at the same moment,
  `curl.exe https://doi.org/...` and `curl.exe https://export.arxiv.org/...` both resolve and
  connect immediately — proving this is not an offline machine, a real DNS outage, or anything
  wrong with the three fixture-based (non-network) test suites, which are all green. Confirmed
  this predates and is unrelated to S7.5's own diff: `doi.rs`'s live test is untouched since
  S7.4 and fails identically. The shape — a security product resolving or blocking DNS
  differently per originating process rather than per machine — matches the sprint-1
  WebView2/Kaspersky entry already in this file (`Sprint-1 WebView2/Kaspersky failure`, under
  Fixed) and the DNS-inside-Tectonic note that entry itself references
  (`scripts/dev-proxy.py`), so a security suite intercepting or filtering DNS for compiled `.exe`
  binaries specifically, and passing `curl.exe` through, is the leading theory — not confirmed,
  since nothing about that process's policy is visible from here. Does not block this loop: the
  card's own `Verify` line is `cargo test -p texbib --features acquire` with no `--ignored`, and
  all 73 non-network lib tests plus clippy (with and without the feature) and both `cargo doc`
  builds are clean. Whoever next hits this — likely S7.6, or S8.3's health checks, both real
  network callers — should check the maintainer's security-suite configuration (an exception for
  `target\debug\*.exe`/`target\release\*.exe`, matching the existing WebView2 exception) before
  assuming a code bug.
  **Still open, wider than recorded (28 Sep 2026, Sprint 9 planning):** the bundled Tectonic is
  hit too. `cargo test -p abstract-tex-engine --test torture -- --ignored` fails at step 1 with
  `File 'graphicx.sty' not found` — the engine cannot reach its package bundle
  (`relay.fullyjustified.net`, `data1b.fullyjustified.net`) — and passes in 21 s with
  `HTTPS_PROXY=http://127.0.0.1:3128` pointed at `scripts/dev-proxy.py`, which resolves DNS from
  Python. So on this machine every real-engine run that needs a package not yet cached must go
  through the proxy; the app itself would show an author the same missing-package error. Worth
  the maintainer checking the security suite's per-process DNS policy, since the app's
  zero-setup promise (DESIGN.md §2 rule 4) depends on the engine reaching the network once.
  **Next step decided (2 Oct 2026, design interview C7):** the maintainer adds the security-suite
  exception for `target\debug\*.exe` and `target\release\*.exe` on the Windows machine, during the
  smoke campaign that follows S11.4d, and records here whether it cures both `reqwest` and the
  bundled Tectonic. If a stranger's security suite does the same, v0.9's stranger test finds it.

- **`fragile-command-in-moving-argument`'s one-click fix is only offered when the offending line
  is short: TeX truncates the `l.NN` context from the left, and the command name goes with it.**
  (S6.4, builder, 17 Sep 2026) The rule (S6.1) and its fix (S6.2) recover the command from the end
  of the context line with `trailing_command`, and the fixture that proved them —
  `footnote-in-moving-arg`, `\section{A title\footnote{a note}}` — is short enough that TeX prints
  the whole line. TeX's `half_error_line` is 50: when more than that precedes the error point, the
  context is printed as `...` plus its tail, and the torture document's realistic
  `\section{Footnotes in titles\footnote{Which never works without protection.}}` arrives as
  `l.1 ...ote{Which never works without protection.}}` — no `\footnote` left to find. The
  explanation falls back honestly ("Try `\protect` right before whichever command is causing
  this") and no fix is offered, which is correct behaviour for what the log says, but it means the
  fix is absent for most real section titles, which are longer than fifty characters. Found by
  step 12 of the torture walk (`crates/preamble-engine/tests/torture.rs`), whose table records
  `fix: None` for this chapter with the reason. Not fixable inside `texlog` (the source line is
  the only place the command survives, and the crate never reads one); a frontend-side
  completion — search the diagnosed source line for a fragile command when the rule matched but
  named none — would be the honest place, and needs its own loop.
  **Decided (2 Oct 2026, design interview E7):** a frontend card — when the rule matched but named
  no command, search the diagnosed source line for one (*Unplaced cards*, `SPRINTS.md`).

- **A diagnostic resolved inside a package file gets a file heading and a jump that cannot
  succeed.** (S6.3, builder, 17 Sep 2026) `babel-unknown-language`'s real fixture resolves to
  `file: "babel.sty", line: 4260` — correct as a statement about where TeX was when it raised the
  error, but `babel.sty` is not a file in the project, so the drawer (S6.3) heads the group
  `babel.sty`, the card says `line 4260`, clicking it produces `Could not open babel.sty`, and no
  tab ever shows a gutter dot for it. The mistake the author can act on is the
  `\usepackage[nosuchlanguage]{babel}` line in their own preamble — the *project* file nearest the
  top of the resolver's stack at that moment, which `texlog` knows (`resolver::open_files` has the
  whole stack) but `Diagnostic` does not carry: `file` is only the innermost entry. A fix wants
  either a second field (the innermost *project* file — but `texlog` never reads the file tree, so
  it cannot tell `babel.sty` from `chapter.tex` except by extension, a heuristic) or the frontend
  walking a stack the IPC does not currently ship. Not decided here; the drawer is at least honest
  about what it has. The same shape will hit `font-not-found` (`fontspec.sty`) and any other
  `\PackageError`. Found during S6.3's own review, not fixed there — it changes `texlog`'s public
  `Diagnostic`, a Rust change with fixture regeneration, past an M frontend loop's scope.
  **Decided (2 Oct 2026, design interview E1):** `Diagnostic` carries the whole open-file stack over
  IPC, and the frontend picks the first file the project has — it already holds the include graph,
  so no extension heuristic. This changes `texlog`'s public type, so it lands before `texlog` is
  published (C8). *Unplaced cards*, `SPRINTS.md`.

- **BibTeX's own error output never reaches `main.log`, so `texlog` cannot see it at all —
  corrects the "biblatex needs biber, not yet bundled" framing from the previous S6.1 commit.**
  (S6.1, builder, 17 Sep 2026) Tested directly: this project's bundled Tectonic *does* invoke
  BibTeX automatically (confirmed by a deliberately malformed `.bib` entry — Tectonic's own stderr
  reports `errors were issued by BibTeX, but were ignored`), so the earlier assumption that a
  missing `biber` binary was the blocker was incomplete. The real blocker is structural: BibTeX's
  diagnostic text — `Illegal end of database file`, the warnings about empty fields, the error
  count — is written entirely to `main.blg`, a file `texlog::diagnostics` is never given (`lib.rs`'s
  own rule: this crate only ever sees the text handed to it, never reads a file itself). `biblatex`
  would face the identical problem one layer up, through `biber`'s own `.blg`. A bibliography-error
  rule needs either a second entry point that also takes `.blg` content, or `diagnostics` gaining an
  optional second parameter — a real design question, not a one-line fix, and not decided here.
  Not required; no sprint-6 or sprint-7/8 (bibliography) card owns it yet.
  **Narrowed** (29 Sep 2026, looking at the corpus thesis's `.blg` files during S9.10). Two
  things changed since this was written. First, most of what BibTeX would report now reaches the
  author another way: S7–S8's bibliography health checks parse the `.bib` themselves (an
  unreadable file, missing fields, undefined and never-cited keys, duplicate DOIs), so what is
  left that only BibTeX can say is small: a `.bst` style that cannot be found, and style-specific
  warnings. Second, a trap for whoever builds this: on any document with `\include`, Tectonic runs
  BibTeX on every chapter's `.aux`, and each chapter's `.blg` holds three *errors* ("I found no
  \citation commands", "no \bibdata", "no \bibstyle") that are not the author's. That is where
  the thesis's "errors were issued by BibTeX, but were ignored" comes from on every build. A
  `.blg` rule must read only the root's `.blg`, or drop exactly those three. Still Open, at low
  priority.
  **Deferred, low priority (2 Oct 2026, design interview E3):** when built, it reads only the root's
  `.blg`, which sidesteps the `\include` trap above.

- **`unwrap_lines` does not undo a `\PackageError` message's own multi-line continuation, only a
  plain 79-column hard wrap.** (S6.1, builder, 17 Sep 2026) LaTeX's `\PackageError`/`\GenericError`
  machinery prints a long message across several physical lines, each continuation prefixed
  `(packagename)` and re-indented — a different mechanism from the transcript writer's own
  column-79 hard wrap that `tokenizer.rs`'s `unwrap_lines` (S5.1) undoes. Two real captures show
  two different failure shapes: `font-not-found/main.log`'s first physical line is 75 characters
  (not 79), so it never joins with its `(fontspec)                found.` continuation at all —
  `raw_message` ends at "...cannot be". `babel-unknown-language/main.log`'s first line genuinely
  is 79 characters and joins correctly with a one-character continuation ("t", completing
  "misspelled it") — but that one-character line breaks `unwrap_lines`'s own chain (its length is
  not 79), so the *next* `\PackageError` continuation line, itself coincidentally 79 characters,
  starts a brand new logical line instead of joining the error message; `raw_message` ends at
  "...misspelled it" with the sentence's second half silently dropped. Neither is a blocking bug —
  both rules' matchers use a short, stable prefix from the *first* logical line, and neither
  explanation depends on the truncated tail — but any future rule whose matcher or explanation
  needs a `\PackageError` message's full text will hit this. A fix would need to recognise the
  `(packagename)` continuation-line shape specifically, which `unwrap_lines`'s own doc comment does
  not attempt today. Not required; no card owns it.

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
  **Decided (2 Oct 2026, design interview E4):** one sweep card fixes this together with the other
  S4.1 include-graph gaps (*Unplaced cards*, `SPRINTS.md`).

- **The include graph's depth cap drops nodes without recording anything.**
  (S4.1, reviewer, 14 Sep 2026) `crates/preamble-includes/src/graph.rs`'s `if depth >= MAX_DEPTH
  { continue; }` (the check right after popping the BFS queue) stops expanding a branch with no
  `Unresolved` entry and no other trace. A chain of 41 files nested one `\input` inside another
  yields 33 nodes (`MAX_DEPTH` is 32) and `is_complete() == true` — silently contradicting the
  module's own `//!` promise to never skip silently. Vanishingly unlikely in a real thesis (the
  comment at the `MAX_DEPTH` constant says as much), but the contract violation is real. Fix
  needs either a new `Unresolved` variant (`DepthLimitReached` or similar) pushed when the cap
  stops a branch, or the module doc's promise narrowed to say what it actually covers.
  **Decided (2 Oct 2026, design interview E4):** one sweep card fixes this together with the other
  S4.1 include-graph gaps (*Unplaced cards*, `SPRINTS.md`).

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
  **Decided (2 Oct 2026, design interview E4):** one sweep card fixes this together with the other
  S4.1 include-graph gaps (*Unplaced cards*, `SPRINTS.md`).

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
  **Decided (2 Oct 2026, design interview E4):** `\InputIfFileExists` and `\subimport` are in the
  include-graph sweep card; following a `.sty`/`.cls` file's own `\input`s stays out of scope, as
  `\usepackage` already is, and the module doc is narrowed to say so.

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
  **Decided (2 Oct 2026, design interview E4):** one sweep card fixes this together with the other
  S4.1 include-graph gaps (*Unplaced cards*, `SPRINTS.md`).

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
  **Scheduled (2 Oct 2026, design interview E8):** the ledger sweep, first thing in sprint 12,
  each fix with its test.

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
  **Scheduled (2 Oct 2026, design interview E8):** the ledger sweep, first thing in sprint 12,
  each fix with its test.

- **No `rustfmt.toml`.** House style runs to ~110 columns; rustfmt defaults to 100. Nothing
  fails today because `pnpm verify` does not run `cargo fmt --check`, but the next
  `cargo fmt` invocation reformats every Rust file in the repo. One-line fix whenever
  someone owns the style decision.
  Still open (28 Sep 2026): measured before adding one — `cargo fmt --check` with
  `max_width = 110` still reports 382 diffs, so a one-line config does not match the house style
  either. The choice is between a one-off whole-repo `cargo fmt` commit (and adding
  `cargo fmt --check` to `pnpm verify`) or no formatter; the maintainer's call.
  **Decided (2 Oct 2026, design interview C5):** `rustfmt.toml` with `max_width = 110`, one
  whole-repository `cargo fmt` commit between S11.8 and sprint 12, then `cargo fmt --check` joins
  `pnpm verify:rust`. Closes when that commit lands.

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
  **Scheduled (2 Oct 2026, design interview E8):** the ledger sweep, first thing in sprint 12,
  each fix with its test.

## Fixed

- **The GitHub token was offered to whatever remote asked for a password.**
  (3 Oct 2026, found while designing the Clone window, which accepts any URL) `credentials()` in
  `abstract-tex-git` handed the stored OAuth token to any HTTPS remote that asked, so a project
  whose `origin` is GitLab, a university server or a pasted lookalike host would have received
  the author's GitHub token on `fetch`, `push`, `sync` or `clone`. It was latent while only
  GitHub remotes were created, and real the moment B7's "generic remotes" or a clone-by-URL
  existed. **Status: Fixed (S11.5b).** The callback now checks the URL (`is_github_https`: `https` and the exact
  host `github.com`, after any `user@`) and answers "no credential" for every other host.
  Tested as a pure function with the lookalikes (`github.com.evil.example`, `evilgithub.com`,
  `github.com@evil.example`, plain `http`). **Not yet seen on a real network**; a GitLab remote
  now simply has no credential to offer, which a generic-remote card (not placed) would add.

- **`cc` on this Linux machine is `zig cc`, which rejects the target triple `cc-rs` passes, so
  `src-tauri` cannot build.** (29 Sep 2026, S9.12) `~/.local/bin/cc` execs `zig cc`, and
  `--target=x86_64-unknown-linux-gnu` — which `cc-rs` adds to every compile — fails with `unable
  to parse target query 'x86_64-unknown-linux-gnu': UnknownOperatingSystem`; zig spells it
  `x86_64-linux-gnu`. This kills `ring`, and with it `cargo build -p abstract-tex`, `cargo test
  --workspace` and `pnpm verify`. Not a bug in this project and nothing in the repo changed, so
  it is here as a note for the next session rather than a fix: run the gate with `CC` pointing at
  a wrapper that rewrites the triple. A real fix is to install a normal clang or gcc, or to put
  `[target.x86_64-unknown-linux-gnu] linker`/`CC` in the environment properly; the maintainer's
  other machine is unaffected.
  **There is a real gcc on this machine, found in S10.3a** (29 Sep 2026): `/usr/bin/gcc` exists
  and works; `~/.local/bin/cc` merely shadows it on `PATH`. So the whole gate runs with
  `CC=/usr/bin/gcc CXX=/usr/bin/g++ pnpm verify` and needs no wrapper and no
  `-fno-sanitize=undefined` — both zig defaults below stop applying, because zig is not involved.
  That is the recommended incantation for a session on this machine until `PATH` is fixed.
  **A second `zig cc` default, found in S10.1** (29 Sep 2026): it turns UndefinedBehaviorSanitizer
  on in debug builds and *traps*. libgit2's bundled `sha1dc` does unaligned 32-bit loads — UB by
  the letter of C, fine on x86, and present in every libgit2 build everywhere — so every `git2`
  commit died with `SIGABRT` and a stack ending in `sha1_compression_states`. The wrapper also
  passes `-fno-sanitize=undefined` now. Worth knowing because the symptom looks exactly like a
  bug in our own code: a clean compile, then an abort inside a C dependency on the first write.
  **Fixed (2 Oct 2026, design interview C6):** the four wrappers (`cc`, `c++`, `gcc`, `g++` —
  one-line `exec zig cc` scripts from 14 Sep) moved from `~/.local/bin` to `~/.local/zig-wrappers/`,
  out of `PATH`; nothing was deleted, so moving them back undoes it. Verified: `which cc` is
  `/usr/bin/cc`, and a clean rebuild of `libgit2-sys` (C, statically linked) succeeds with no `CC=`
  prefix.

- **`PREAMBLE_OPEN` env var resolves relative to the wrong directory.**
  `src-tauri/src/commands.rs:42` filters `PREAMBLE_OPEN` through `Path::new(p).is_dir()`,
  which resolves a relative path against the Tauri process's working directory
  (`src-tauri/`), not the repository root. `fixtures/paper/SMOKE.md`'s documented invocation
  `PREAMBLE_OPEN=fixtures/paper pnpm tauri dev` therefore opens no project at all, silently.
  An absolute path works. Found: 12 Sep 2026. Not yet fixed — small, but it is in the script
  handed to a new contributor.
  Still open (28 Sep 2026, now `ABSTRACT_TEX_OPEN` after the rename): `README.md` and
  `DEVELOPMENT.md` give the absolute-path form and say why; every `fixtures/*/SMOKE.md` still
  gives the relative one that opens nothing.
  **Fixed (2 Oct 2026, design interview D7)** as the documentation debt it was: every
  `fixtures/*/SMOKE.md` now gives `"$PWD/fixtures/<name>"` (bash) and `"$PWD\fixtures\<name>"`
  (PowerShell). The code still resolves a relative path against `src-tauri/`, which
  `DEVELOPMENT.md` already says; an absolute path is the documented form.

- **`anyhow::Context::context()` silently hid Git LFS's own error message from the author.**
  (2 Oct 2026, found while writing S11.3c's `lfs::run`) The first draft ran `git lfs track` and,
  on a non-zero exit, built `bail!("{stderr}")` — the real, actionable message — then wrapped the
  call with `.context("tracking the file with Git LFS")`. `anyhow::Error`'s `Display` (what
  `commands.rs`'s `to_message` sends the frontend, since it takes `impl Display`) only ever shows
  the outermost context, never the source it wraps; the sentence that would have reached the
  author was the generic one, and git-lfs's own reason — the one piece of information the banner
  exists to relay — was silently discarded one layer down. Caught by a test built against
  `fake-git-lfs-track-fails` (a compiled stand-in, not a mock, the same pattern
  `abstract-tex-lsp`'s `tests/process.rs` already uses for a subprocess that must fail in a
  specific way): it asserted the surfaced message contained the fake's stderr, and it did not.
  **Fixed** by building one complete sentence in `run` itself — action and stderr together — so
  there is only ever one frame for `Display` to show, and reserving `.context()` for the rarer
  spawn-failure path, where losing detail matters less.

- **`git2::DiffFile::size()` reads 0 on a tree-to-tree diff, silently.** (2 Oct 2026, found while
  writing S11.3a's `oversized_blobs`) The first draft read `change.new_file().size()` straight off
  a `diff_tree_to_tree` delta, on the assumption libgit2 already has a blob's size from the tree
  entry it is diffing — it does not: `file_size` in libgit2's `iterator.c` is only ever populated
  from a workdir `stat()` (`iter->entry.file_size = entry->st.st_size`), and a `git_tree_entry`
  carries no size field at all, only a mode and an id. Every delta silently reported size 0, which
  a test with a tiny limit caught immediately — the fixture file never tripped the check. **Fixed**
  by reading `Odb::read_header(id)` instead, the binding for `git_odb_read_header`, which asks the
  object database for the object's real length from its header without inflating the full blob.
  Left as a comment on `oversized_blobs` so nothing else in this crate repeats the same assumption.

- **`Commit & Push` and `Commit & Sync` could erase the commit message the moment it was refused.**
  (1 Oct 2026, found by `controller.test.ts` while writing S11.1b) The first draft of `commitThen`
  called `refreshGitStatus()` unconditionally, after the `if (committed !== null)` block rather
  than inside it — so it ran on a refusal too, and `refreshGitStatus` rebuilds the suggested
  message whenever `messageIsSuggested` is still true, which it is until the author's first
  keystroke in the box. A refused commit (no identity, nothing staged) left the author's own
  sentence sitting in the box one line of code away from being silently replaced by a suggestion
  built from the change they had just failed to commit. Caught immediately: a test asserted the
  words survive a refusal, the same promise `commitStaged` already keeps, and they did not.
  **Fixed** by moving the refresh inside the success branch, next to the other two things that
  already only happen on success (clearing the box, re-arming the suggestion).

- **A Source Control refusal appeared and then vanished before it could be read.** (29 Sep 2026,
  found by S10.5a's own tests; the bug was introduced in S10.3a) `git.error` held both a *verb's*
  refusal — "nothing is staged", "Git does not know who you are", "this folder is already inside
  a repository" — and a *read's* failure, and `refreshGitStatus` cleared it on every successful
  read. Since every verb is followed by a refresh, awaited or arriving as `git:status-changed`
  within 120 ms, the sentence explaining why nothing happened was wiped a moment after appearing.
  Nobody saw it while clicking, because the panel still looked right; it showed up as an assertion
  in `controller.test.ts`. **Fixed** by splitting the slots: `git.error` is a verb's refusal,
  cleared when the next verb *starts* and by nothing else, and `git.readError` is a read's
  failure, cleared when a read succeeds. Both are pinned by
  `a refusal outlives the refresh that follows it`.

- **Confirmed: under latexmk, `\write18` (what shell escape runs through) obeys the process's own
  working directory, never `-output-directory`, so it writes into the source tree.** (29 Sep 2026,
  S9.8; confirmed 29 Sep after the maintainer installed TeX Live) `latexmk.rs` runs every build
  with the project folder as `cwd`, only telling `pdflatex` where to put its *outputs*
  (`-outdir`). Reproduced directly, no minted needed: `\immediate\write18{pwd > marker}` under
  `pdflatex -output-directory=<build> -shell-escape`, run from the project folder, writes `marker`
  beside the `.tex`, not into the build folder. minted's own cache would land the same way.
  Tectonic does not have this problem — `-Z shell-escape-cwd=<dir>` sets the shell commands'
  directory independently of the engine's own cwd, which is what S9.8 relies on.
  **A fix shape is tested and works for a plain TeX Live pdfTeX toolchain:** run the process with
  `cwd` = the build folder instead of the project folder, and add the project folder to
  `TEXINPUTS` (kpathsea does not search the main file's own folder automatically once cwd moves
  away from it — confirmed separately: `\input{sections/sub}` failed to resolve, with or without
  an absolute path to the root file, until `TEXINPUTS` named the project folder). Not applied to
  `latexmk.rs` yet: this changes the working directory of *every* latexmk build, not only
  shell-escape ones, and minted's own error text on this machine ("MiKTeX is being used with
  `-aux-directory`... without setting a `TEXMF_OUTPUT_DIRECTORY` environment variable") suggests
  MiKTeX may need a different mechanism than `TEXINPUTS` — unverified, no Windows/MiKTeX machine
  here. Card S9.12 in SPRINTS.md picks this up properly, with both distributions and `\include`
  covered, rather than merging a cwd change proven on one platform. `\inputminted` with a relative
  path is the same root cause and closes with the same fix; still open on its own until then.
  **Fixed** (29 Sep 2026, S9.12): with shell escape on, `latexmk.rs` runs the engine in the build
  folder instead of the project, and puts the project folder in front of `TEXINPUTS`, `BIBINPUTS`
  and `BSTINPUTS` — three variables, not one, because kpathsea has one per kind of file and a
  missing one fails silently. `fixtures/shell-escape/` and `crates/abstract-tex-engine/tests/
  latexmk.rs` pin it against the real TeX Live: the `\write18` marker lands in the build folder,
  the source tree comes back byte-for-byte identical, and `\input`, `\include`, a `.sty` beside
  the manuscript, `\includegraphics` through `\graphicspath` and BibTeX's `.bib` all still
  resolve. Builds *without* shell escape do not move at all — there is nothing for them to buy.
  Two things this entry guessed at, now settled: `TEXMF_OUTPUT_DIRECTORY` is **not** MiKTeX-only
  (`latexrestricted` 0.6.2's `tex_openout_roots`, which is what minted v3 uses on every
  distribution, reads it with a plain `os.getenv` and has no distribution branch), and it is set
  on shell-escape builds; and `\inputminted` on a relative path is *not* closed by this fix — see
  the next entry, which is now its own thing rather than a footnote to this one. MiKTeX itself
  stays unverified: there is still no MiKTeX machine here.

- **Every diagnostic from a system engine missed its tab: the log says `./main.tex` where the
  project says `main.tex`.** (29 Sep 2026, found in S9.12 while checking what the moved working
  folder does to a log) `texlog` hands file names on exactly as TeX printed them, and
  `diagnosticTarget` (`src/lib/drawer.ts`) matches them against the include graph's
  project-relative paths. Tectonic writes `main.tex` and matches; `pdflatex` writes `./main.tex`
  and never did, so since S9.4 every diagnostic from a `latexmk` build fell back to the root file
  in the drawer and the gutter, with nothing to say it had. **Fixed** in the same loop:
  `as_the_project_spells_it` in `compile.rs` — the first place that knows where the project is —
  strips a leading `./` and, for shell-escape builds, the project folder's own absolute prefix. A
  name outside the project (`/usr/share/texlive/…/report.cls`) is left exactly as the log had it.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **Latent: the LSP bridge does not read while it writes.** (found with the entry below, 28 Sep
  2026) `bridge::supervise` is one `select!` loop, and inside its outbound branch it awaits
  `running.send(&body)` to completion, so nothing drains TexLab's stdout during a write. That can
  only deadlock if TexLab, blocked writing to a full stdout, also stops reading its stdin while a
  message larger than the pipe (64 KB on Linux) is being sent to it — a whole-file `didOpen` or
  `didChange` on a long chapter. Not observed; whether TexLab 5.26's I/O threads can block that
  way is unconfirmed. Recorded so it is checked, not rediscovered: the fix would be separate
  reader and writer tasks, as TexLab itself has.
  **Fixed** (29 Sep 2026, S9.11). Confirmed first, with a stand-in rather than TexLab: taught
  to write a 320 KB burst with blocking writes (which is how TexLab's I/O library behaves, since
  its threads hand messages to each other unbuffered), `fake-lsp-rpc` and the old bridge hung
  for good on the second 50 KB `didChange`; the new test timed out every time. The supervisor
  now never writes: each process gets its own writer task (`spawn_writer`, fed by a channel) and
  the loop stays free to read. The same test passes 20 runs in 20, and the real-TexLab tests
  still pass.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **A cancelled warm build makes the next build a full one: saving during a 3 s thesis build
  costs the next save ~12 s.** (29 Sep 2026, found designing S9.9) `Tectonic::build` removes
  `.abstract-tex-warm` before anything runs and writes it back only after a success
  (`tectonic.rs`, S9.2), so a build the orchestrator cancels — every save that lands while a
  build is running — leaves no marker, and the next build starts cold: every BibTeX run and
  every rerun, 12–25 s on the corpus thesis instead of one 3.3 s pass. That is safe (S9.2's
  point 3: never trust an `.aux` a half-run pass may have rewritten) but it turns the ordinary
  typing rhythm into the slowest path there is, and it also means no draft (S9.9) for that
  build, since `draft::prepare` borrows the same marker. A likely fix: a pass cancelled
  *before TeX wrote anything* could restore the marker, or the orchestrator could let a warm
  single pass finish instead of cancelling it, as its cost is bounded. Not fixed in S9.9, which
  does not change when builds are cancelled.
  **Fixed** (29 Sep 2026, S9.10): a warm build takes an in-memory checkpoint of the build
  folder's intermediates and, if cancelled, writes them back and then the marker. The build after
  a cancel is now one 3.5 s pass on the thesis. Measuring it corrected this entry's premise:
  Tectonic writes a pass's intermediates only as the pass ends, so a kill almost never
  half-writes an `.aux`. The marker was what was lost, and the checkpoint is what makes putting
  it back safe without relying on that.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **A draft's SyncTeX names its root `…/.abstract-tex/draft/../../main.tex`, so neither search
  direction would match the root file while a draft was on screen.** (29 Sep 2026, found in
  S9.9's spike before any app code was written) TeX records a file by the path it was asked to
  open, and the draft's wrapper reaches the root by a path relative to its own folder (S9.7).
  Chapter files come out clean — they are `\include`d relative to the project — so only the
  root's own pages (title, contents, bibliography) were affected: forward search from `main.tex`
  found nothing, and inverse search returned a project-relative path that climbed out and back
  in. **Fixed** (29 Sep 2026, S9.9): `abstract-tex-synctex` resolves `.`/`..` segments in every
  `Input:` path as text when it parses; pinned by a unit test and by the ignored real-engine test
  `a_click_in_a_real_thesis_draft_lands_on_the_chapter_line`.

- **The README had gone stale since sprint 1, and two of its instructions did not work.**
  (28 Sep 2026, found rewriting it as a public project page) It still said "Status: sprint 1";
  its *Building* section ran `pnpm fetch-engine` alone, which leaves TexLab out of
  `src-tauri/binaries/`, so `tauri_build` fails on a fresh clone (CI runs `pnpm fetch-sidecars`,
  which is why CI never noticed); it gave `ABSTRACT_TEX_OPEN=fixtures/paper`, the relative form
  the `PREAMBLE_OPEN` entry under *Open* shows opens nothing; it said `texlog` "is the `texlog`
  crate on crates.io", which it is not yet (S6.5 `[~]`); and it described the v0.7 assistant as
  if it existed. **Fixed** (28 Sep 2026): `README.md` rewritten from what the code and
  `0.1/SPRINTS.md` show today, with `pnpm fetch-sidecars`, an absolute launch path, the crates
  described as ready but unpublished, and planned work only in its *Versions* table. The
  development plan, stack rationale and build traps moved to `DEVELOPMENT.md`.

- **A warm single pass never wrote a PDF: `--pass tex` stops at the `.xdv`.** (S9.7 spike,
  28 Sep 2026, found timing the thesis on Linux) S9.2's warm step runs Tectonic with `--pass tex`,
  which runs the TeX engine and nothing after it — no `xdvipdfmx` — so it leaves `main.xdv` in
  the build folder and the *previous* build's `main.pdf` untouched. `Tectonic::build` then
  reports success with that stale PDF, and the preview pane shows the last full build's pages
  for every warm edit after it: the one outcome the compile loop exists to prevent. Checked by
  timestamp: after three warm passes on the corpus thesis, `main.pdf` was still the cold build's
  and `main.xdv` the latest pass's. `--outfmt pdf` beside `--pass tex` changes nothing, and
  Tectonic refuses an `.xdv` as input, so the conversion cannot be run on its own. Nothing
  tested for it: the real-engine warm test counted `undefined` in the log, which a TeX pass
  alone gets right, and S9.2's timings (3.3 s thesis, 0.69 s paper) therefore never included
  making the PDF. Fixed by running the warm pass as Tectonic's default pass with `--reruns 0` — one TeX
  pass, BibTeX only if the `.aux` names a database, then `xdvipdfmx` — and by asserting in the
  real-engine warm test that the PDF is rewritten (checked to fail on `--pass tex`). Cost of the
  PDF, now counted: thesis 3.2 → 3.4 s, conference paper 0.62 s, every corpus edit still one pass.

- **`frames_keep_their_boundaries_under_load` hung forever on Linux.** (28 Sep 2026, first
  `pnpm verify` on a Linux machine that can link everything) The test sent fifty frames (~125 KB)
  before reading any reply. The single-threaded echo stand-in writes each reply as it reads, so
  once its stdout pipe (64 KB on Linux) was full it blocked, stopped reading stdin, that pipe
  filled too, and the test's `send` never returned — a classic two-pipe deadlock that Windows'
  pipe buffering hid. A test bug, not a transport bug: fixed by sending and draining ten frames
  at a time, which stays under one pipe's worth and still has several frames in flight. The
  bridge's own version of the pattern is the latent entry under Open.

- **Focus mode rebuilds every line's decoration on every keystroke and cursor move.**
  (S4.5, deferred reviewer pass, 16 Sep 2026) `src/lib/editor/focus.ts`'s `buildDecorations`
  iterates every line in the document on every `ViewUpdate` where the doc changed or the
  selection moved, to rebuild which lines are dimmed. Almost certainly under the <16 ms keystroke
  budget (DESIGN.md §2) at thesis-length documents — each line costs one `RangeSetBuilder.add` —
  but the same shape as two findings already logged here (`mergeMarkers` re-running per view
  update, S3.3b; `Project::info()` re-reading every file per refresh, S4.1), so worth the same
  "memoize if it ever measures otherwise" note rather than assuming it is fine forever. Not
  required; no sprint-4 card owns performance work.
  **Fixed** (S9.6, 28 Sep 2026): focus mode is a `StateField` now. A cursor move inside the lit
  paragraph returns the same state; typing inside it shifts the existing dimming with the text;
  only a paragraph change or an edit elsewhere rebuilds. It also stopped copying the whole
  document into a string per keystroke (`currentParagraphRange` reads CodeMirror's `Text`).
  `focus.test.ts` counts rebuilds: zero across nine typed characters.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

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
  **Fixed** (S9.6, 28 Sep 2026): `Project::info` keeps the graph and a stamp (exists, size,
  modified time) of every file it walked, missing ones included, and walks again only when a
  stamp differs — one `metadata` call per document file instead of a read. `info` takes
  `&mut self` for it. Tested by counting walks: unchanged, and a non-document file changed,
  cost none; a chapter gaining an include, and a missing file appearing, cost one each.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **The gutter's `markers` callback re-merges on every view update.**
  `src/lib/editor/diagnostics.ts` runs `mergeMarkers` on every CodeMirror view update,
  allocating two Map/array/RangeSet triples per keystroke on the typing path (DESIGN.md §2
  commitment 2, <16 ms). Almost certainly under budget at realistic diagnostic counts, so
  not blocking — logged as the one place in this diff that allocates per key, for a future
  loop to memoize on the two field values (`markers`, `lspMarkers`) instead of the view
  update. Found by the reviewer, S3.3b, 13 Sep 2026.
  **Fixed** (S9.6, 28 Sep 2026): `mergedMarkers`, a `StateField` that merges only when either
  source is replaced and otherwise shifts with the text; a cursor move returns the identical
  set (tested by reference). A deleted line break that lands two markers on one line re-merges,
  the one case shifting alone would get wrong — also tested.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

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
  **Fixed** in the test (28 Sep 2026, commit `1201016`; this entry was only updated on 29 Sep):
  it now sends ten frames at a time and reads their replies before sending more, which stays
  under one pipe's worth and still keeps several frames in flight. The bridge's own version of
  the hazard is not fixed, and is the *Latent: the LSP bridge does not read while it writes* entry
  above.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **A linked Zotero collection's entries are indexed but never reach the PDF: the compile does
  not know about `extra_bib_files`.** (S8.4 preparation, builder, 28 Sep 2026, found writing the
  exit demo's script) S8.2 records the export path in `preamble.toml`'s `extra_bib_files`, and
  `bibliography::build_index` indexes it, so `\cite` completion offers its keys and the
  undefined-citation health check (S8.3) counts them as defined. But BibTeX/Biber read only the
  files the document itself names (`\bibliography{…}`/`\addbibresource{…}`), and nothing in
  `compile.rs` or `preamble-engine` reads `extra_bib_files` (grep: only `bibliography.rs`,
  `commands.rs` and `project.rs` mention it). Result: cite a key from a linked collection and the
  panel says nothing is wrong while the PDF prints `[?]` — the health check and the engine
  disagree, the one outcome a health check must not produce. Workaround until fixed: name the
  export in the document too (`\bibliography{references,zotero/Thesis}`); S8.7's `BibOrigin`
  then lists it as `named` and everything agrees. Fix is S8.8 in `SPRINTS.md`; it needs a
  maintainer decision first, because the natural fixes either edit the author's `.tex` (offer to
  add the export to the document's own command, as a one-click fix like S6.2's) or make the
  build differ from what the document says (pass extra files to the engine), and DESIGN.md §2
  rule 1 ("plain files are the truth") argues for the first.
  **Fixed** (S8.8, 28 Sep 2026) the first way: a `linked-not-named` finding — an error when a
  citation is defined only in the export — whose one-click fix adds the export to the document's
  own resource command. Verified by `applying_the_fix_makes_the_export_named_and_the_finding_goes_away`.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **The workspace `repository` field points at `github.com/verocai-vrc/preamble`, but the
  repository is `github.com/verocai-vrc/Abstract-Tex`.** (S8.5, builder, 28 Sep 2026, found
  preparing `texbib` for crates.io) `Cargo.toml`'s `[workspace.package] repository` is inherited
  by `texlog` and `texbib`, and both READMEs link the same URL, so each crate's crates.io page
  would link a repository that does not exist under that name. A published crate's metadata
  cannot be corrected without a new release, so this blocks both `cargo publish` runs (S6.5,
  S8.5) until settled. Tied to the still-open name decision (S2.9, `DESIGN.md` §10): either the
  GitHub repository is renamed to `preamble` (GitHub redirects the old URL) or the field and both
  READMEs change to `Abstract-Tex`. The maintainer's call, not an agent's.
  **Fixed** (28 Sep 2026): the maintainer settled it — the repository is
  `github.com/verocai-vrc/Abstract-Tex` and the *Preamble* name is dropped (DESIGN.md §10). The
  workspace `repository`, both crate READMEs, and the two `acquire` user agents now point there,
  as part of the whole-codebase rename.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`cargo build --release` is not the release path.**
  `tauri.conf.json` sets `devUrl` unconditionally, so even a release binary loads
  `localhost:1420` and shows a "can't reach this page" error without Vite running.
  `pnpm tauri build` is the supported route (S2.9). Documentation debt more than a code bug;
  worth a README note before anyone tests a release binary the quick way.
  **Fixed** (28 Sep 2026) as the documentation debt it was: `DEVELOPMENT.md`'s *Building and
  testing* names the trap and the supported route. The `devUrl` behaviour itself is unchanged.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`pnpm check` fails on `main`: `src/lib/bibliography.test.ts:75` indexes `groups[0]` without a
  guard, and `svelte-check` reports "Object is possibly 'undefined'".** (planning review, 28 Sep
  2026, found running the full verify gate before planning sprint 8's remainder) Introduced by
  `8a55f20` (S8.3's work, committed as `feat: add bibliography health checks…` rather than
  `S8.3: …`). S8.3's outcome paragraph in `SPRINTS.md` records `pnpm check` 445 files / 0 errors
  and Vitest 402/402; the tree as committed gives 443 files / 1 error and 399/399, so the recorded
  numbers came from a working tree that differs from what landed. Vitest itself passes — the
  test runs fine, only the type check rejects it — but `pnpm verify` is red, so every loop from
  here would start from a failing gate. Likely fix: `groups[0]?.findings` or an
  `expect(groups).toHaveLength(2)` followed by a non-null assertion. Also worth checking in the
  same pass: the S8.3 row in `SPRINTS.md`'s sprint 8 table still reads `[ ]` although its outcome
  says `[x]`.
  **Fixed** (S8.3 follow-up, 28 Sep 2026): `groups[0]?.findings` — a missing group now fails the
  `toEqual` rather than the type check. `pnpm check` 443 files / 0 errors; the S8.3 table row is
  ticked and its outcome's counts corrected.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`cargo test -p texbib --features acquire --test fixtures` fails all six real-fixture cases on
  this machine, purely from CRLF byte-offset drift.** (S8.2, builder, 23 Sep 2026, found running
  the full `texbib` test suite as this loop's own verification step) Every failure is the same
  shape: the parsed spans in `left` are consistently larger than the recorded `expected.json`
  spans in `right`, by exactly the count of `\r` bytes before that point in the file — the
  fixture `.bib` files are checked out CRLF on this machine, but `expected.json` was recorded
  against an LF checkout. Confirmed pre-existing and unrelated to this loop's diff: `git stash`
  (removing every S8.2 change) reproduces the identical six failures with identical `left`/`right`
  diffs. Likely the same root cause as the synctex real-fixture failure below (a `.gitattributes`
  gap letting Windows checkouts normalise line endings in fixture text files that must stay
  byte-for-byte what they were recorded against) — worth checking both under the same fix rather
  than two separate ones. Does not block this loop: the card's `Verify` line does not run the
  `fixtures` integration test, and all unit tests (including this loop's new `zotero.rs` and
  `bibliography.rs` ones) are green.
  **Fixed** (S8.3 follow-up, 28 Sep 2026) together with the S7.3 entry below: same root cause, a
  `.gitattributes` fix. The synctex failures turned out *not* to share it (see their own entries).
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`RUSTDOCFLAGS="-D warnings" cargo doc --workspace` fails on `preamble-synctex`: its module doc
  links to a private item.** (S7.6, builder, 23 Sep 2026, found running `cargo doc --workspace`
  as this loop's own final check, since `crates/preamble-synctex` was untouched by the diff)
  `crates/preamble-synctex/src/lib.rs:6` reads `decompressing and tokenising the SyncTeX text
  format (see [`parse`])`, and `parse` is a private module-level item — `cargo doc` on this crate
  alone (`cargo doc -p preamble-synctex`) does not catch it, only a `--workspace` run does,
  because per-crate doc builds do not turn on `rustdoc::private-intra-doc-links` the same way
  `-D warnings` does at the workspace level. Not caused by this loop, which touched only
  `crates/texbib` and `src-tauri`; both build clean under `RUSTDOCFLAGS="-D warnings" cargo doc -p
  texbib -p preamble --no-deps`. Likely fix: plain backticks for `parse` (the convention
  `crates/texbib/src/acquire/arxiv.rs`'s own `zero_span` doc comment already uses for the same
  reason, after S7.5 hit an identical class of error), or `#[allow(rustdoc::private_intra_doc_links)]`
  if the module doc is meant to describe internals a reader is expected to open the source for.
  **Fixed** (S8.3 follow-up, 28 Sep 2026): plain backticks for `parse`, the first option above.
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` is clean.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`texbib`'s fixture harness fails on a checkout with `core.autocrlf=true`: six of seven
  fixtures mismatch on every byte span after the first line ending.** (S7.3, builder, 21 Sep
  2026, found running `pnpm verify` for an unrelated frontend loop) `crates/texbib/tests/
  fixtures.rs`'s `run_fixture` reads `main.bib` with `fs::read_to_string` — raw bytes, no
  normalisation — and compares the parsed spans against a committed `expected.json` computed
  when the S7.1 fixtures were built and verified on a Linux sandbox session, where line endings
  are `\n`. This repository has no `.gitattributes`, and this checkout's `git config
  core.autocrlf` is `true` (a common Git-for-Windows default), so `git checkout` silently
  rewrote every committed `\n` in the six multi-line fixtures to `\r\n` on disk; `file` confirms
  it (`ASCII text, with CRLF line terminators`). Every span after the first line ending is then
  off by however many `\r`s precede it, and `fixture_better_bibtex`, `fixture_biblatex_
  inheritance`, `fixture_broken_middle`, `fixture_hand_typed`, `fixture_jabref` and `fixture_
  strings_and_preamble` all fail; the seventh, `fixture_doi_negotiation`, is one line with no
  terminator (S7.4's own note) and passes untouched. Not caused by S7.3, which touched no Rust
  and no `crates/texbib` file. Likely fix: a `.gitattributes` marking `crates/**/fixtures/*.bib`
  (and any other fixture depending on an exact byte layout) `-text` or `eol=lf`, so a fresh
  checkout matches what `expected.json` was computed against regardless of the checking-out
  machine's global `autocrlf` setting.
  **Fixed** (S8.3 follow-up, 28 Sep 2026): a root `.gitattributes` marks `fixtures/**` and
  `crates/*/fixtures/**` `text eol=lf` (and `*.gz`/`*.pdf`/`*.png` binary), and the fixture files
  were re-checked-out so this machine's working copy is LF. Scoped to fixtures on purpose — a
  repo-wide `eol=lf` would have been a whole-tree rewrite for no failing test. All seven
  `texbib` fixture tests pass under `core.autocrlf=true`.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **`src-tauri/src/synctex.rs`'s own real-fixture test fails on any checkout path other than the
  original author's, the same way the already-logged `preamble-synctex` one does.** (S7.3,
  builder, 21 Sep 2026, found running `pnpm verify` for an unrelated frontend loop)
  `inverse_search_end_to_end_against_the_real_fixture` (line 138) calls `table.forward_search(&source,
  3).expect("line 3 is on page 1")`, and the `.expect` panics with exactly that message: the
  committed `crates/preamble-synctex/fixtures/multi.synctex.gz` bakes in the absolute path Tectonic
  resolved at capture time (`C:\Users\arthur\Desktop\LaTeX Editor\Abstract Tex\Abstract-Tex\...`),
  `paths_match` normalises case and separators but not the checkout prefix, and this checkout sits at
  `C:\Ambiente de Desenvolvimento\Abstract-Tex`. Same root cause as `the_real_fixture_parses_and_both_
  searches_answer`'s entry below (S4.6, 16 Sep 2026) in the *other* crate's copy of this pattern — this
  is the sibling failure in `src-tauri`'s own end-to-end test, not a second bug, and not caused by
  anything in S7.3 (which touched only `src/lib/editor/cite.ts`, its test, and two files wiring it in —
  no Rust). Same fix candidates apply: look the tag up by the fixture's own recorded `Input:` path, or
  compare by trailing path components in the test rather than the full resolved path.
  **Fixed** (S8.3 follow-up, 28 Sep 2026): not a line-ending problem, as the S8.2 entry above
  guessed, but the recorded absolute path. The test now copies the fixture into a temp folder,
  rewrites its `Input:1:` line to that folder, re-gzips it (`flate2` added as a dev-dependency)
  and runs `open` → forward → inverse → `to_relative` against the temp folder as project root —
  the whole chain on real paths, on every OS. Asking the fixture for its recorded folder instead
  would have passed on Windows only: `strip_prefix` cannot split a Windows path on Linux/macOS,
  where CI also runs this test.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

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
  (`4169c3c`) with S4.6's changes stashed, so it predates this loop; not caused by anything in
  S4.6, which touched no Rust. Fix candidates: have the test look the tag up by the
  fixture's own `Input:` path (the `files` table is already parsed) instead of by
  `CARGO_MANIFEST_DIR`, or compare by trailing components (`fixtures/multi.tex`) in the test
  only — `paths_match` itself should stay exact, since a real project can have two files with
  the same tail. Found running `cargo test --workspace --exclude preamble` for S4.6's gate.
  **Fixed** (S8.3 follow-up, 28 Sep 2026) by the first candidate above: the test asks the fixture
  where it recorded `multi.tex` (the input named by the record nearest page 1's top-left) and
  searches with that path; the check that it *is* `multi.tex` compares normalised strings,
  since `Path::ends_with` sees a Windows path on Linux as one component. `paths_match` untouched.
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **S8.1's `zotero.rs` probed a JSON-RPC method, `item.libraries`, that does not exist in Better
  BibTeX's real API.** (S8.2, builder, 23 Sep 2026, found checking the real JSON-RPC method list
  against `retorque.re/zotero-better-bibtex/exporting/json-rpc/` and the project's own
  `content/json-rpc.ts` source while designing S8.2's collection listing, the same "check live
  before building on it" step S7.5's outcome recommended after a documented endpoint 404ed in
  practice) The real method for listing libraries is `user.groups` (optionally
  `includeCollections: true` to also list each library's collections). Calling the nonexistent
  `item.libraries` did not break S8.1 itself — Better BibTeX answers an unknown method with a
  JSON-RPC `error` envelope, and `detect_with` only checks *shape* (any `jsonrpc` + `result`/`error`
  reply counts as `Ready`), so detection was accidentally still correct.
  **Fixed** in this loop (S8.2) by switching the probe to `user.groups`, which is both a valid
  liveness check and the call S8.2 needs anyway for listing collections — one request now does
  both jobs. Verified by `cargo test -p texbib --features acquire -- zotero` (12 passed).
  *(Moved from Open to Fixed on 2 Oct 2026 — design interview C4; its status already said Fixed.)*

- **A diagnostic inside an extensionless `\input{sections/foo}` matched no tab: the click, the
  one-click fix and the gutter all missed it.** (S6.4, builder, 17 Sep 2026) `\include` always
  opens `sections/foo.tex`, which S6.3's thesis capture checked, but `\input{sections/foo}` — the
  far more common spelling — is echoed by this engine as `(sections/foo`, extension and all
  missing, and that is what `Diagnostic.file` carries (`resolver.rs`'s module doc names this as
  the limit of text-only resolution and hands it to "a caller with access to the real file
  tree"). `diagnosticTarget` compared that spelling to the tab path `sections/foo.tex` with no
  normalisation, so nothing matched: clicking the card opened nothing, "Apply fix" refused with no
  target, and the gutter drew the dot on no tab at all. Found by the very first step of the
  torture walk (`crates/preamble-engine/tests/torture.rs`), which is what that document is for.
  Fixed in `drawer.ts`: `diagnosticTarget` now takes the project's `documentFiles` (S4.1's include
  graph) and completes `file` to `file + ".tex"` when that, and not the bare spelling, is a file
  the graph knows — never a guess, only a name that exists. The group heading uses the same
  completed name. Regression tests: `drawer.test.ts`'s "completes an extensionless `\input` to the
  `.tex` file the graph knows" and the torture walk itself.

- **The raw log view showed the previous build's log after a new build finished while it was
  open.** (S6.3, builder, 17 Sep 2026) `toggleRawLog` read `main.log` once, at the moment the view
  was switched on; `handleCompileEvent`'s `finished` branch left `showRawLog` alone on a failed
  build and never re-read the file, so an author who kept the raw view open while fixing an error
  and rebuilding saw the cards update to the new diagnostics and, one click away, a log that still
  described the old ones. Found while reading `controller.svelte.ts` for S6.3. Fixed in the same
  loop: the read moved into `refreshRawLog`, which `finished` calls whenever the view is showing
  (and only then — a failed build with the view closed still reads nothing, per DESIGN.md §2's
  "never a raw log by default", which a second test now pins). Regression test:
  `controller.test.ts`'s "re-reads the log when a build finishes while the raw view is open".

- **`jumpToDiagnostic` still assumed every diagnostic was about the root file, though `Diagnostic.
  file` has named the real one since S5.6.** (S6.2, builder, 17 Sep 2026) The function's own doc
  comment said as much — "Until the paren-stack resolver lands (S5.2)..." — but S5.2 and S5.6 had
  both already landed by the time S6.2 started, and nothing had come back to update the code once
  the field it was waiting on existed. Clicking a diagnostic that happened inside an `\input`ed
  chapter would open `main.tex` and jump to whatever line number that diagnostic named there,
  landing on unrelated text rather than a "file not found" error. Found while wiring S6.2's
  one-click fix, which cannot risk editing the wrong file the same way a click could silently
  mis-jump. Fixed by routing through `diagnosticTarget` (`diagnostic.file`, falling back to the
  project's root file only when `file` is `null`); `jumpToDiagnostic` and `applyDiagnosticFix`
  (S6.2) both use it now. Regression test: `controller.test.ts`'s "jumping to a diagnostic in a
  different file opens that file, not the root (S5.6 wiring)".

- **`fixtures/thesis/SMOKE.md` §3 step 3 described a command-palette search the app cannot do.**
  (S4.4, deferred reviewer pass, 16 Sep 2026) The step asked the reader to search `convergence`
  while `sections/conclusion.tex` was the active tab and expected a section from
  `sections/results.tex` to appear from "the document map's own section list" — but the palette's
  section search (S4.3) is scoped to the active file's outline only (S4.2's own design choice), so
  a section belonging to a different, inactive file can never surface there. Fixed by reordering
  the walkthrough to make `results.tex` the active tab before searching for its own section.

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

- **Turning shell-escape consent on or off makes the next latexmk build a full one.** (29 Sep
  2026, found verifying S9.12) `.fdb_latexmk` is latexmk's own dependency database, and it
  records every source file by the path latexmk saw — relative (`"main.tex"`) from the project
  folder, absolute (`"/home/ada/thesis/main.tex"`) from the build folder. S9.12 moves the working
  folder on exactly that toggle, so every path in the database changes at once and latexmk
  reruns everything: `Rule 'pdflatex': Reasons for rerun`, measured directly. Bounded and rare —
  consent is answered once per project folder (S9.8), not per build — so it is logged rather than
  worked around. The fix, if it ever matters, is to run *every* latexmk build from the build
  folder and pay S9.12's log-name cost everywhere; that trade was considered in S9.12 and
  declined, and this entry is the evidence for reconsidering it.
  **Won't fix (2 Oct 2026, design interview E5):** accepted as a documented limitation —
  `DEVELOPMENT.md` says so — with no general fix available. Revisit only if minted v3's
  `\inputminted` turns out not to work either once a machine can run it.

- **A shell command handed a project-relative path finds nothing, on either engine.** (29 Sep
  2026, S9.12; split out of the entry above, which had it as a footnote) Both engines run the
  document's shell commands in the build folder — Tectonic through `-Z shell-escape-cwd` (S9.8),
  `latexmk` through the process's own working directory (S9.12) — because that is the only way to
  keep what they write out of the source tree. The cost is symmetric: `\write18{cat code/x.py}`,
  and `\inputminted{python}{code/x.py}`, which is the same command with a friendlier name, now
  resolve `code/x.py` against the build folder and find nothing. Pinned by
  `a_shell_command_given_a_project_relative_path_finds_nothing` so it cannot change unnoticed.
  Not fixed because there is no general fix: a shell command's arguments are opaque to us, and
  neither engine offers a way to run the commands in one folder and resolve their arguments in
  another. What would close it is minted-specific — minted v3 resolves input paths through
  `latexrestricted`, whose readable roots include `TEXMF_OUTPUT_DIRECTORY` and the kpathsea
  paths, so `\inputminted` may already work where a raw `\write18` does not. Unverified here:
  minted v3 cannot run on this machine at all (next entry).
  **Won't fix (2 Oct 2026, design interview E5):** accepted as a documented limitation —
  `DEVELOPMENT.md` says so — with no general fix available. Revisit only if minted v3's
  `\inputminted` turns out not to work either once a machine can run it.

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
