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

- **Two answers to "this project folder is not a Git repository" for comments.** (10 Nov 2026, found
  merging `origin/main`, S14.3e) The 7 Oct `project.rs` in `abstract-tex-comments` gives such a folder
  its own repository for comments; the 9 Nov `src-tauri/src/comments.rs` (S14.3b) refuses it with a
  sentence. Both compile and both are tested, but the Tauri commands still use the refusing path, so
  `project.rs` has no caller and a non-Git project cannot yet be commented on. Not decided: which one
  wins, and whether `comments.rs` should call `project::*` instead of `open`. **Open.**

- **Starting a branch looked like a failure: the picker stayed open and the name stayed in the box.**
  (10 Nov 2026, found walking B4 in the app, in code not yet committed) Tauri answers a command that
  returns `()` with `null`, and `runGitVerb` also returns `null` for a refusal, so `startBranch` and
  `switchToBranch` read success as failure. The unit tests were green because their mocks returned
  `undefined`. **Fixed:** the mocks now answer `null` like the real bridge (and failed first), and both
  verbs report success explicitly instead of through `runGitVerb`'s return value. Worth knowing for any
  future verb that returns nothing: do not test `runGitVerb`'s result for `null`.

- **Ctrl Z did nothing in the editor on Linux.** (3 Oct 2026, found under Xvfb while checking that an
  applied assistant suggestion undoes in one step; plain typing did not undo either) `y-codemirror.next`
  undoes through the browser's own `beforeinput` "historyUndo" event and ships its `Mod-z` keymap as an
  export the app must add. WebView2 and WKWebView fire that event on Ctrl Z; WebKitGTK does not, so on
  Linux there was no undo or redo at all. Reproduced with a temporary key probe: the `z` keydown reached
  CodeMirror with `defaultPrevented` false. **Fixed (3 Oct 2026):** `baseKeymap()` in
  `editor/setup.ts` puts `yUndoManagerKeymap` first (its bindings `preventDefault`, so a webview that
  does fire the native event is not asked to undo twice); unit-tested for presence, precedence and no
  collision with an app shortcut. Checked in the running app: typing then Ctrl Z removes it, on screen
  and on disk.

- **Opening another project left the previous project's PDF on screen under "Compiling…".** (3 Oct
  2026, found under Xvfb) `app.pdfUrl` was reset on open, but `PdfPane`'s effect returned early on a
  null URL and the viewer kept its rendered pages. **Fixed (3 Oct 2026):** `PdfViewer.clear()`,
  called when there is no PDF to show (also covers a comparison still being built). Checked in the
  running app: opening an empty folder shows "The PDF will appear here after the first build."

- **A modal dialog did not take keyboard focus: `Escape` did nothing and typed text reached the
  editor behind it, and was saved.** (3 Oct 2026, found under Xvfb, rung 4 of S11.7b) The merge view,
  the Snapshot viewer and the Clone window were `<div role="dialog">` with a key handler on the
  dialog element and nothing focusing it, so with focus still on the Source Control row behind,
  `Escape` was never heard and typing went to the previous focus. Reproduced: "curric⏎" typed with
  the merge view open appeared as a new first line of `main.tex` on disk. **Fixed (3 Oct 2026):**
  `use:modalFocus` (`src/lib/dialog.ts`) focuses the dialog on open, keeps Tab inside it, and
  returns focus to what had it on close; on those three and on New project. Tab arithmetic is
  unit-tested; the DOM part was checked in the running app (typed text stays out of the editor,
  `Escape` closes). Also in the same pass: the New project chips say "CV" not "Cv", and the example
  text in the fields is muted so it no longer looks like a typed answer.

- **`Ctrl Shift G` in the editor opened the find panel as well as switching to Source Control; the
  merge view's "N unchanged lines" bars were white bands in the dark theme.** (3 Oct 2026, found
  under Xvfb, rung 4 of S10.3a / S11.7b) The first: CodeMirror's search keymap binds `Mod-g` with a
  `Shift` form (find previous), and a CodeMirror binding does not stop the key reaching the
  window's handler, so one keypress did two things. **Fixed:** `editorSearchKeymap` drops `Mod-g`
  (`F3` / `Shift F3` remain); `setup.test.ts` also asserts no search binding collides with an app
  shortcut. The second: `.cm-collapsedLines` is restyled with the app's own colours in
  `editor/diff.ts`. Both checked in the running app.

- **On WebKitGTK a document longer than the window pushed the status bar, the outline and every
  scrollbar out of the window.** (3 Oct 2026, found with the entry below, under Xvfb, on a template
  CV and on `fixtures/paper`) The app grid's middle row was `1fr`, whose minimum is `auto`, so it
  grew to the editor's content height; nothing scrolled and `Ctrl+End` moved the cursor out of
  sight. A short document hid it. **Fixed (3 Oct 2026):** `grid-template-rows: auto minmax(0, 1fr)
  var(--statusbar-height)` and `min-height: 0` on `.editor-column`. Checked in the running app on
  the paper fixture: status bar, outline and scrollbars present. No unit test is possible (CSS).
  Whether WebView2 on Windows ever showed it is unknown; the rule is correct on both.

- **On Linux, every build triggers another build, for ever: the watcher reports reads.** (3 Oct
  2026, found by running the app under Xvfb for the first time — rung 4 of S11.11/S11.12 — on a
  template project and on `fixtures/minimal` alike) Tectonic opening `main.tex` produces an inotify
  access event; the debounced watcher forwards it as a change; the controller's `handleFsEvent`
  calls `triggerCompile`; the new build opens `main.tex`. Observed: ~940 builds in two minutes,
  each cancelling the last before it finished, the PDF pane stuck on "Compiling…" and the status
  bar on "Compiling… -0.0s". Reproduced by `watcher::tests::reading_a_file_is_not_a_change`.
  Never seen on Windows, which reports no reads. **Cause:** `notify` 8.2 hard-codes the inotify
  `OPEN` mask, and `notify-debouncer-mini` throws away each event's kind. **Fixed (3 Oct 2026):** the
  watcher uses `notify-debouncer-full`, which keeps the kind, and drops `Access` events; one report
  per path per window. Checked in the running app: one build, "Built in 0.5s", PDF shown.

- **`pnpm tauri dev` fails: `cargo run` cannot pick a binary.** (3 Oct 2026, found launching the
  app for a hand walk-through) `src-tauri` declares three `fake-git-lfs-*` `[[bin]]` targets
  beside the app, and without a `default-run` key `cargo run` refuses to guess among four. The
  plain `pnpm tauri dev` therefore exits with code 101 before any window opens. Workaround:
  `pnpm tauri dev -- --bin abstract-tex`. The fix is `default-run = "abstract-tex"` under
  `[package]` in `src-tauri/Cargo.toml`. Separately, the CLI prints a "version mismatched Tauri
  packages" notice (`tauri` crate 2.12.0 vs `@tauri-apps/api` 2.11.1); it did not stop the
  launch, but the two should be brought to the same minor. **Second cause, same launch:** from a
  shell started inside the VS Code snap, the binary then dies at startup with `symbol lookup
  error: /snap/core20/.../libpthread.so.0: undefined symbol: __libc_pthread_init`, because the
  snap's `GTK_PATH`, `GIO_MODULE_DIR`, `GDK_PIXBUF_*`, `GSETTINGS_SCHEMA_DIR`, `LOCPATH` and
  rewritten `XDG_DATA_DIRS` make GTK load the snap's old libc-era modules. Workaround: unset those
  and restore `XDG_DATA_DIRS` from `XDG_DATA_DIRS_VSCODE_SNAP_ORIG` before `pnpm tauri dev`. A
  shell opened outside the snap would not need it.
  **First cause fixed (3 Oct 2026):** `default-run = "abstract-tex"` added to `src-tauri/Cargo.toml`;
  `cargo run -p abstract-tex` now starts the app binary (checked: it got as far as the snap error
  above). **Second cause worked around (3 Oct 2026):** `scripts/dev.sh` unsets the snap's GTK
  variables and restores `XDG_DATA_DIRS` / `XDG_CONFIG_DIRS` from their `_VSCODE_SNAP_ORIG`
  copies, then runs `pnpm tauri dev`; with that environment `cargo run -p abstract-tex` ran for
  60 s with no libc error. A shell outside the snap needs nothing. **Fixed (3 Oct 2026):** `@tauri-apps/api` raised to 2.12.0 to match the `tauri` crate;
  `tauri info` no longer reports a mismatch. Closed.

- **`credentials()` may offer a rejected token for ever.** (3 Oct 2026, suspected while building
  S11.5a) libgit2 calls a credential callback again after the server refuses what it returned, until
  the callback answers with an error. `abstract_tex_git::credentials` returns the same token every
  time it is asked, so a revoked or wrong token would not produce "authentication failed" but a
  fetch, push or clone that never finishes. **Not confirmed**: every unit-test remote here is a
  local path, which never asks for a credential at all. S11.8's smart-HTTP test (3 Oct 2026) does
  put a server that demands a password in front of the callback, but that server is not
  github.com, so the callback correctly gives it nothing and the loop cannot start; it narrows
  the suspicion and does not settle it. What settles it: a real GitHub with a revoked token (the
  OAuth app, B1). The fix, if it loops, is a flag so the token is offered once.
  **Fixed defensively (3 Oct 2026):** `TokenOffer` offers the token once per operation; the second
  request answers "GitHub did not accept the saved sign-in. Sign in to GitHub again." (unit test
  `a_token_the_server_refused_is_not_offered_a_second_time`). Whether libgit2 really looped is still
  unobserved; a real revoked token (the OAuth app, S11.8b) would confirm the sentence appears.

- **A comparison's "Raw output" is the engine's stderr, not its `main.log`.** (3 Oct 2026, S11.4d)
  The drawer's comparison section (A7: "raw log one click away") can only show what the
  `compile-diff` `finished` event carries, which is `stderr`. `read_log` reads the *live* build's
  log, and nothing reads a log by path, so a comparison that fails inside TeX shows the engine's
  last lines and not the full transcript. The diagnostics cards (from `texlog`) are unaffected.
  What closes it: a Rust command that reads `<comparison folder>/build/main.log`, or `logPath`
  from the event passed to a generic read — a small card, noted here rather than widened into
  S11.4d (which has no Rust in it). **Fixed (3 Oct 2026, S11.4e):** `read_comparison_log` takes no
  path (the folder holds only the latest comparison) and the drawer reads it when *Raw output* is
  opened, falling back to stderr if there is no log.

- **Leaving a comparison does not cancel its build.** (3 Oct 2026, S11.4d) *Back to live PDF*, the
  toolbar's × and `Esc` make the frontend ignore the comparison's answer and events, but no
  command asks the diff lane to stop, so a build in flight finishes unseen (and the next
  comparison cancels it, as S11.4c's lane always does). Harmless except for the CPU a thirty-second
  compile of a thesis takes. Closing it needs a `cancel_comparison` command. **Fixed (3 Oct 2026,
  S11.4e):** `cancel_comparison` bumps the lane's ticket (retiring a comparison still exporting) and
  cancels the orchestrator; the controller calls it when a *building* comparison is left.

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
  **Seen on screen, 15 Nov 2026 (sixth pass, Linux, virtual display): resolved.** `fixtures/thesis` with
  a chapter edited in the app; the PDF pane photographed every 200 ms through one build. The status bar
  read "showing a draft of sections/results", and with the pane scrolled to page 2 the middle frames
  showed the draft's page (the Results chapter's paragraphs) where the full PDF has "Chapter 1
  Introduction", then the pane went back to the full PDF when it landed. So the asset scope reaches
  `.abstract-tex/draft/build/main.pdf`. The draft is on screen only between its own finish and the
  full build's: about 60 ms on the 0.6 s fixture, about 1.2 s when the full build was made slower. Not
  checked on Windows.

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
  **Fixed (10 Nov 2026):** `system_engine_warm_builds_stay_under_a_wall_clock_ceiling` in `corpus.rs`,
  with its own `SYSTEM_ENGINE_GATE` table (document, program, ceiling), and S9.5's two comments now say
  "the bundled Tectonic". Checked three ways: it passes at the real ceilings, fails with a clear message
  with a ceiling set below the measurement, and fails loudly under `ABSTRACT_TEX_REQUIRE_SYSTEM_TEX` on a
  partial install. **Limits, stated plainly:** only two documents could be measured on this machine
  (`beamer` 1.1 s, `tikz-figures` 3.5 s warm p95, pdfLaTeX); the other four skip because its TeX Live
  lacks `IEEEtran`, `siunitx` and `luabidi`, and their ceilings are three times Tectonic's, marked
  `UNMEASURED` in the file. **CI's runner has no TeX installed**, so the gate skips there today; it bites
  only once a full TeX Live is installed in the workflow and the require variable is set (a comment in
  `verify.yml` says where). That install is a cost the maintainer should choose.

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
  **Fixed (10 Nov 2026):** `fragile_command::offer_missing_fixes` (`src-tauri/src/fragile_command.rs`)
  reads the diagnosed line and offers the `\protect` fix when it holds exactly one of `\footnote`,
  `\footnotemark`, `\footnotetext`, once, and not already behind a `\protect`. It lives at the app edge
  in Rust rather than in the frontend as the card said: it reads the line from disk, which is the text
  the build compiled and so the text the log's `l.NN` counts, and it needs no async read in the drawer.
  With two candidates on one line it offers nothing, because `Fix.find` matches the first and a second
  click would stack a second `\protect`. **Still open, narrower:** when the cut-off context happens to
  contain some *other* backslash (`...\emph{words} and more text{a note}}`), `texlog` names that command
  instead, and the fix it offers (`\protect\emph`) is harmless but does not cure the error. Only a
  context with no backslash at all is completed here, as the card said.

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
  **Fixed (10 Nov 2026):** `Diagnostic.open_files` carries the stack; `drawer.ts`'s `diagnosticPlace`
  files the card under the innermost entry the project has and drops the package's line number (and
  with it the one-click fix, which would have edited the wrong text). Two corrections to the decision as
  recorded: the project's files are the include graph **and** the file tree, because a local `mystyle.sty`
  is in no `\input` graph yet is a tab with a true line; and a `./main.tex` or an absolute path in the
  stack is respelled like `file` is (S9.12), or the stack would match no tab under a system `latexmk`.
  Checked on the real `babel-unknown-language` capture and in the running app. The card opens the
  author's file but does not find the `\usepackage` line (E7's shape).

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
  **Fixed (10 Nov 2026):** `texlog::bibtex_diagnostics(blg)` (`crates/texlog/src/blg.rs`) with three
  rules, from real captures by the bundled Tectonic: `bibtex-style-not-found` (an error; the app finds the
  `\bibliographystyle` line in the root file), `bibtex-repeated-entry` (a warning, with the `.bib` file and
  line BibTeX names) and `bibtex-no-citations` (a warning). Every other BibTeX message produces nothing.
  `compile.rs` reads only the root's `.blg` (named like the log) and **only if this build wrote it**,
  because a build that does not run BibTeX leaves the last one behind. Found while walking the app:
  the drawer's header and the status bar counted warnings only on a successful build, so "1 error, 1
  warning" read "1 warning"; both now count both (`problemCounts`). **Known limit:** these cards'
  `rawMessage` is BibTeX's, so "Raw log" does not highlight it in `main.log`.

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
  **Fixed (10 Nov 2026):** an include only excludes a candidate when the including file is no deeper
  than the candidate (so `old/submission.tex` including `paper.tex` no longer demotes it), and when the
  rule would exclude every candidate it is ignored (two files that include each other now yield `a.tex`,
  not nothing). Three tests in `project.rs`, one of them a guard that a subfolder chapter included from
  above is still not a root.

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
  **Fixed (10 Nov 2026):** a new `Unresolved::DepthLimit { path }` is pushed for the file the walk
  stopped expanding, so `is_complete()` is false. Tested with a forty-file chain.

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
  **Fixed (10 Nov 2026):** `build_graph` normalises the root the way it normalises every argument, so
  `./main.tex` and `main.tex` are one node and `graph.root` is `main.tex`. A root that climbs out of the
  project cannot be normalised and is left as written.

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
  **Fixed (10 Nov 2026):** `\subimport` (two groups) and `\InputIfFileExists` (three) are recognised and
  reported as `Unparsed`, like `\import`, consuming their arguments so an `\input` inside one is not
  reported twice. The graph therefore says it is incomplete instead of saying nothing. Following a
  `.sty`/`.cls` stays out of scope and `graph.rs`'s module doc now says so.

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
  **Fixed (10 Nov 2026):** once a candidate is confirmed to exist, `on_disk_spelling` reads each
  component's real name back from the directory listing. Only when the filesystem itself says the file
  exists: on Linux `Sections/Intro` finds nothing, TeX would not find it either, and the graph must not
  claim it did. The unit tests of the spelling run everywhere; the wiring test takes the branch the
  host's filesystem takes, so **the case-insensitive branch has not run on this Linux machine** and will
  first run on the Windows and macOS legs of CI.

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
  **Fixed (3 Oct 2026, S12.0):** `controller.test.ts` now has three cases at `handleFsEvent`: a `.tex` nothing includes does not rebuild; an included `.tex` and a `.bib` do; an unresolved graph rebuilds for any `.tex`.

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
  **Fixed (3 Oct 2026, S12.0):** `normalizeUri` in `lsp.ts` (lower-case drive letter, `%3A` → `:`) is the key for `LspDiagnosticStore`, on `publish` and `forPath` alike.

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
  **Fixed (3 Oct 2026):** `rustfmt.toml` (`max_width = 110`), one `cargo fmt --all` over the
  whole repository, and `cargo fmt --all --check` is now the first step of `pnpm verify:rust`
  (so CI runs it too). Run `cargo fmt --all` before committing Rust.

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
  **Fixed (3 Oct 2026, S12.0):** `blankComments` blanks each unescaped `%` to end of line before the scan (offsets kept; the span's TeX is still cut from the original text), and `renderMath` passes `strict: 'ignore'`. The `$a$$b$` boundary advisory is cosmetic and left.

## Fixed

- **`pkg-config` is not installed on this Linux agent session (no root to add it), so any
  `--workspace` Rust build failed before reaching whatever loop's own diff prompted it — first
  hit at the start of S14.3a, on a plain `cargo test --workspace`.** (7 Oct 2026) `libdbus-sys`
  (`keyring`'s Secret Service backend, reached through `abstract-tex-assistant`/`abstract-tex-github`,
  and `tao`) and `openssl-sys` (`git2`'s `https` feature, enabled somewhere in the graph by
  `abstract-tex-git`/`abstract-tex-snapshot`/`abstract-tex-latexdiff`) both need it; Cargo's
  feature unification means either one poisons a single `--workspace` invocation even for a crate
  that touches neither. `CLAUDE.md`'s Linux notes say WebKitGTK is installed but do not mention
  `pkg-config` itself, and this session genuinely has neither it nor `libdbus-1-dev` (confirmed
  with `dpkg -l`: `libssl-dev` *was* already present, headers and `.pc` file both — only the
  `pkg-config`/`pkgconf` binary and the dbus headers were missing).
  **Fixed (7 Oct 2026), without root:** `apt-get download` fetches a `.deb` to the working
  directory with no privilege check (only *installing* one needs root), and `dpkg-deb -x <deb>
  <dir>` extracts a package's files into an arbitrary directory, also with no root. Downloaded
  `pkgconf`, `pkgconf-bin`, `libpkgconf7` (the real binary behind the `pkg-config` metapackage on
  this Ubuntu release, which itself ships no binary at all) and `libdbus-1-dev`, extracted all
  four into a scratch prefix, then two small patches: `dbus-1.pc`'s `prefix=/usr` rewritten to
  that prefix (so its relative `includedir`/`libdir` resolve inside it, not into the real `/usr`
  this session cannot write to), and its `Requires.private: libsystemd >= 209` line deleted (no
  `libsystemd.pc` exists here, and nothing this build needs cares about `dbus`'s systemd
  socket-activation support). Linking still failed once
  (`undefined symbol: sd_listen_fds`/`sd_is_socket`, from the extracted package's static
  `libdbus-1.a`, which was compiled against systemd); fixed by symlinking an unversioned
  `libdbus-1.so` in the scratch prefix to the system's real, already-installed, already-linked
  `/usr/lib/x86_64-linux-gnu/libdbus-1.so.3`, which prefers dynamic linking over that static
  archive and carries no unresolved symbol. With `PKG_CONFIG` pointed at the extracted `pkgconf`,
  `PKG_CONFIG_PATH` at the scratch prefix's `pkgconfig` dir (plus the system's own, for the
  already-present `openssl.pc`), and `LD_LIBRARY_PATH` at the scratch prefix's lib dir,
  `cargo test --workspace --exclude abstract-tex` went from failing at the first `-sys` crate to
  **575 tests passing across every library crate, zero failures** (one further fix needed: a
  `git config --global user.name`/`user.email` — also genuinely absent on this session — for the
  `abstract-tex-git` tests that expect a resolvable identity).
  **Still not fixed, and a much bigger job:** `abstract-tex` itself (the Tauri app crate) does not
  build here — `tao`/`tauri-runtime-wry` need `glib-2.0` and roughly fifty more GTK3/WebKitGTK
  `-dev` packages and their own recursive dependencies, an order of magnitude more than the two
  packages above, with real ABI-compatibility risk if versions are mismatched by hand. Left for
  the maintainer's own `apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev
  libsoup-3.0-dev libjavascriptcoregtk-4.1-dev` (the standard Tauri Linux prerequisite list) rather
  than extended further by hand — this is the same standing gap `CLAUDE.md`/`SPRINTS.md` have
  recorded since sprint 2, just now isolated to exactly that one crate instead of the whole
  workspace. **For the next session on this machine:** the extracted prefix does not survive a
  fresh sandbox (it lives under this session's scratchpad, not the repository), so this fix
  itself is not durable — rebuilding it takes four `apt-get download`s, two `dpkg-deb -x`s, one
  `sed`, and one `ln -s`, all above, plus the three environment variables.

- **A joined session's own content, after the joiner cleared it, silently stopped a peer's edits
  from ever being learned about — permanently, with no error anywhere.** (4 Oct 2026, found
  running S14.2c's own two-window demo under Xvfb) `RelayProvider.receive` answered a peer's
  sync-step-1 with this doc's own sync-step-2, as `y-protocols/sync` already arranges — but never
  sent *this* doc's own sync-step-1 back. For the sharer (first in the room), that is harmless:
  their own sync-step-1, broadcast the moment they shared, reached nobody (the room was empty) and
  nothing ever asks again. For the joiner's buffer-clear (`joinSession`, the entry below) that is
  fatal: clearing leaves a tombstoned item in the joiner's own history, and the sharer — never
  told to ask the joiner what *they* had — never learns that item's identity. Any edit the joiner
  later made that anchored to it as a CRDT position, even only structurally and never visibly
  (found because `y-codemirror.next` anchors "insert at the end of the visible text" to whatever
  item is structurally last, which a tie-break between two concurrent origin-null items can leave
  as the joiner's own invisible tombstone rather than the visible content before it), was a
  dependency the sharer could never resolve. `Y.applyUpdate` does not throw or call its error
  handler for this — it is indistinguishable from a normal out-of-order update still waiting on a
  dependency that, this time, will never arrive. Caught only by typing in the *joiner's* window and
  watching the *sharer's* window never update, confirmed byte-for-byte in isolation (the relayed
  update decoded correctly with `Y.decodeUpdate`; the dependency really was simply never sent).
  **Fixed:** `receive` now also sends its own sync-step-1 immediately after replying to one — the
  "the server should reply with SyncStep2 immediately followed by SyncStep1" handshake
  `y-protocols/sync.js`'s own module doc already asks for, which this had only half-implemented.
  Pinned by `live-session.test.ts`'s `a joiner who cleared independently-loaded content before
  connecting can still edit, and the sharer sees it` — written against the *room*-based
  `FakeWebSocket` (an empty room when the sharer's own sync-step-1 goes out), not a pre-paired
  socket pair, because a pre-paired pair delivers that broadcast anyway and the bug never
  reproduces there. `src/lib/relay/provider.ts`.

- **The first message a `LiveSession` ever sent — its own sync-step-1, and its first awareness
  announcement — was silently dropped if the `WebSocket` had not yet reached `OPEN`.** (4 Oct 2026,
  same session) `RelayProvider` and `RelayAwareness` each send immediately on construction; a real
  `WebSocket.send` throws `InvalidStateError` before `OPEN`, and both sends were `void`,
  fire-and-forget. The exact message every peer most needs — "here is my state" / "here is who I
  am" — was the one most likely to race the handshake and vanish as an unhandled rejection no test
  with a synchronous or already-open fake socket could see. **Fixed:** `LiveSession.connect` now
  awaits the socket's own `open` event (or its `error`) before constructing `RelayProvider`/
  `RelayAwareness`, so neither ever sends before the channel can carry it.
  `src/lib/relay/live-session.ts`; the test fixture's own `FakeWebSocket` was tightened to throw on
  a premature `send`, the real behaviour it exists to stand in for, so this class of bug cannot
  pass silently again.

- **`LiveSession.destroy` could close the socket while `RelayAwareness`'s own "I'm leaving"
  broadcast was still mid-flight, throwing it away.** (4 Oct 2026, same session) Encrypting a
  message is asynchronous; `destroy` fired the broadcast (via `removeAwarenessStates`, synchronous
  to call but not to send) and then closed the socket on the very next line, before that send's
  `crypto.subtle.encrypt` could resolve. The goodbye a peer most needs on a clean leave was the one
  message most likely to never arrive. **Fixed:** `RelayAwareness.destroy` is now `async` and
  awaits that specific broadcast; `LiveSession.destroy` awaits `awareness.destroy()` before closing
  the socket. Separately, `RelayProvider.sendEnvelope` now swallows a send that fails because the
  channel already closed, for every caller, not only this one — a live session's channel can close
  at any moment a message happens to be mid-flight, and most callers fire sends without awaiting
  them; the alternative was an unhandled rejection grade intermittent across the whole test suite,
  not a sign of a real failure to act on. `src/lib/relay/awareness.ts`, `live-session.ts`,
  `provider.ts`.

- **Joining a live session on a file already open duplicated its content instead of converging.**
  (4 Oct 2026, same session) Both sides load the same file from disk independently before ever
  connecting, so their two `Y.Doc`s share no common CRDT history even when the text matches byte
  for byte — Yjs has no way to know "this insert" and "that insert" are the same edit, so syncing
  two non-empty buffers concatenates them. Seen immediately in the two-window demo: the joiner's
  buffer showed the whole document twice. **Fixed:** `joinSession` clears the joiner's buffer
  before connecting, so the buffer's entire content arrives as an ordinary remote update once the
  session answers back — the same path any later edit takes, so there is exactly one way content
  gets into a joined document, not two. `src/lib/controller.svelte.ts`. (This clear is what exposed
  the sync-step-1 bug above — logged separately because the two are independent defects, not one.)

- **A `RelayAwareness`'s own presence never reached the first peer who had already connected
  when it joined.** (4 Oct 2026, writing S14.2c's test) The constructor called
  `setLocalStateField('user', presence)` — which fires y-protocols' `'update'` event
  synchronously — *before* `this.awareness.on('update', this.broadcastLocalChange)` registered
  the listener that broadcasts it, so that first, defining call to say who this client is never
  went out. A later field change (`yCollab`'s own cursor update, in the real editor) broadcasts
  the *whole* current state and would eventually carry the name along with it, which is exactly
  why `provider.test.ts`-style unit tests of sync alone never exercised this path and the first
  draft of `awareness.test.ts` passed one assertion by accident before a second one caught it
  with no masking update to hide behind. **Fixed:** the listener is attached before the first
  `setLocalStateField`, with a comment at the call site saying why the order matters.
  `src/lib/relay/awareness.ts`.

- **Sending an explanation checked the assistant's opt-in against the root file's name, not the
  project folder.** (3 Oct 2026, found walking S13.2 under Xvfb) `assistant_send_explain` took
  `project_root`'s two paths from a tuple in the wrong order, so `ready_to_send` asked "is the
  assistant on for `main.tex`?" and answered "not switched on for this project" at the moment of
  *Send*, after the dialog had been shown. Two `PathBuf`s in a tuple type-check in either order.
  **Fixed (S13.2):** `project_root` returns a `ProjectRoot { dir, file }`, so the field is named
  where it is used. The unit tests could not see it (the commands take Tauri state); the walk did.

- **The payload dialog's Send button scrolled out of reach when the exact request was opened.**
  (3 Oct 2026, found walking S13.3 under Xvfb) The whole dialog scrolled, so the footer with *Send*
  and *Cancel* sat below the fold exactly when the person was reading the long request body — the
  moment they most want to act. **Fixed (S13.3):** only the middle scrolls (`.scroll`), the footer
  stays. Not caught by a unit test; a screenshot found it.

- **The undefined-citation scan never saw `\Citep`, `\Cite` or `\Citeauthor`.** (3 Oct 2026, found
  writing S12.2's attack tables) `scan_citations` matched `name.contains("cite")`, which is
  case-sensitive, so natbib's and biblatex's capitalised commands — which contain no lower-case
  "cite" — were skipped: an undefined key in one never produced a warning, and a key used only
  there looked "never cited". **Fixed (S12.2):** the scan now uses `texbib::is_citation_command`
  (case-insensitive), the same predicate the assistant's guard uses; test
  `scan_citations_reads_capitalised_cite_commands`. The editor's completion (`cite.ts`) was
  already case-insensitive.

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

- **`crates/abstract-tex-comments/src/anchor.rs`'s `context_before`/`context_after` doc comments
  link to the private constant `CONTEXT_CHARS`, which fails `RUSTDOCFLAGS="-D warnings" cargo doc`
  (pre-existing, not introduced by S14.3a).** (7 Oct 2026, found while running that check as S7.5's
  outcome established for `texbib`; not part of `pnpm verify` or CI for this crate, so it was
  never caught before.) **Won't fix for now:** low priority, cosmetic, and this crate (unlike
  `texlog`/`texbib`) is not slated for separate publishing where docs.rs would surface it; fix by
  swapping the two links for plain backticks, the same move S14.3a made for its own two new links
  to private items, whenever someone next touches `anchor.rs`.

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

- **The comment box did not take focus when *Comment on selection…* opened the Comments view.**
  (S14.3c, 9 Nov 2026, found on the first Xvfb walk.) The draft appeared with the quoted words,
  but the keyboard stayed in the editor, so typing the comment edited the document (or was lost)
  instead. Fixed in the same loop: the textarea takes focus when it is created
  (`CommentsView.svelte`). **Fixed.**
- **`scripts/xvfb-app.sh` can open the app on the real Wayland desktop.** (9 Nov 2026) With
  `WAYLAND_DISPLAY` set in the calling shell, GTK ignores `DISPLAY=:99`, the window appears on the
  person's own screen and the virtual display stays black. Worked around by running it as
  `(unset WAYLAND_DISPLAY; GDK_BACKEND=x11 scripts/xvfb-app.sh …)`. **Fixed** the same day: the script now unsets `WAYLAND_DISPLAY` and sets `GDK_BACKEND=x11`.

- **After *Leave*, the status bar kept showing "Live session ended" for the file just left.**
  (S15.2, 9 Nov 2026, found on the Xvfb walk against the real relay.) `leaveLiveSession` removed
  the file's status, then the session's own asynchronous `destroy` reported `closed` through the
  callback still attached and wrote it back. Fixed the same loop: the callbacks are detached before
  `destroy` is called. **Fixed.**

- **An external edit that restores an earlier version of a file is swallowed as "an echo of our own
  write", leaving the buffer different from the disk.** (Found 9 Nov 2026 on the owed smoke walk,
  `fixtures/paper/SMOKE.md` §3 step 4. **Fixed** the same session: `is_our_own_write` now forgets a
  remembered hash the first time the file is seen holding anything else; regression test
  `restoring_an_earlier_version_from_outside_is_reported_not_taken_for_our_own_echo`, and the
  sequence re-walked in the app.) Sequence: the app
  saves `main.tex` (content A). Another program changes it to B (the buffer follows, a build
  runs). The other program then changes it back to exactly A. The watcher logs "ignoring echo of
  our own write" and does nothing: no reload, no rebuild. The editor still shows B, the disk holds
  A, and nothing says so. Cause (to confirm in `watcher.rs`): self-write suppression is by content
  hash and the hash of A is still remembered from the app's own save, long after that save's echo
  came and went. Why it matters: rule 1 — the file on disk is the truth — and the next save from
  that buffer would overwrite the author's restore with B.
- **The Outline panel lists every heading twice and shows no labels once the language server is
  answering.** (Found 10 Nov 2026 on the owed walk of `fixtures/thesis/SMOKE.md` §1. **Fixed** the same session;
  regression tests in `symbols.test.ts` and `outline.test.ts`, and the panel re-walked on the
  thesis chapters.) TexLab puts a section symbol's `selectionRange` on its `\label` line when it
  has one (probed with the real binary: `\chapter` on line 1, `range` starts at 1, `selectionRange`
  at 2). `flattenSymbols` reads `selectionRange`, so the server's "Background" sits on line 2 while
  the scan's sits on line 1; `mergeOutline` only drops a scanned item whose line a symbol claims, so
  both survive, at two indents. The same claimed-line test also removes the label rows themselves,
  which is why none appeared. Fix: take the line from `range.start`, and only let a symbol replace a
  scanned *section*.

- **Editor hover tooltips are near-white text on a near-white box in the dark theme.** (Found 10 Nov 2026
  on the thesis walk: hovering `\ref{ch:introduction}` shows "Chapter (Introduction)" almost
  invisibly.) CodeMirror's default `.cm-tooltip` is a light grey box; the editor theme is not marked
  dark, so it keeps that box while the text inherits `--fg`. The maths popover was already set to
  `--bg-panel`, which hid it there. **Fixed** the same session: the editor theme styles `.cm-tooltip`
  with the app's tokens (`--bg-panel`, `--fg`, both already in the contrast guard `contrast.test.ts`),
  and the tooltip re-photographed in the app. No dedicated test: a CSS rule inside a CodeMirror
  theme is not worth a brittle assertion.

- **An error that comes after the first page has been shipped is filed under a file name with the
  page marker glued on: `sections/08-undefined-environment [1`.** (Found 10 Nov 2026 on the walk of
  `fixtures/torture/SMOKE.md` §2, row 08. **Fixed** the same session: `without_page_marker` in `crates/texlog/src/resolver.rs`, two
  regression tests that failed first, and the heading re-photographed in the app.) Rows 01 to 07 fail
  before any page is output and are headed correctly. The heading is the log's file stack, so the
  `texlog` file tracking is taking `(sections/08-undefined-environment [1` as one name. The card's
  line and explanation are right; only the heading is wrong, and clicking it would open no file.

- **Row 14 of the torture document ("Display math should end with $$") fails the build and the
  drawer shows no error card: "The build failed. 0 errors, 2 warnings".** (Found 10 Nov 2026 on the
  walk of `fixtures/torture/SMOKE.md` §2.) The real log has the `!` line and `captures/14.log` parses.
  Worst case for rule 3 and rule 6: a failed build with nothing to read except the raw log. Cause:
  the line that opens chapter 14, `) (sections/13-caption-outside-float) (sections/14-display-math-wrong-delimiter`,
  is exactly 79 characters, `unwrap_lines` took that for a wrapped line and glued the `!` line onto
  it, so the error never existed as a line of its own. **Fixed** the same session: a physical line
  that starts `! ` is never a continuation (TeX has already broken the line itself after 79
  characters). Two regression tests that failed first (`tokenizer.rs`, and the real log's shape
  through `diagnostics` in `rules.rs`), and the card re-photographed in the app. The remaining
  failure mode, a full-width line followed by a wrapped remainder that really does start with `! `,
  is not realistic.

- **Long file names in the tree squash the chevron column, so rows sit at different horizontal
  offsets and the name is cut off with no ellipsis.** (Found 10 Nov 2026 on the torture walk, whose
  chapter names are long; cosmetic.) `.tree .row` is a flex row; its `text-overflow: ellipsis` does
  not reach the flex children, so when a name was too wide the 12 px chevron shrank instead.
  **Fixed** the same session in `app.css`: the chevron and badge do not shrink, and the name clips with
  an ellipsis. Checked in the app; no test (pure CSS layout).

- **After a *Sync* that merged two diverged histories, or hit a conflict, the sentence under the button
  is blank.** (Found 10 Nov 2026 while wiring comments into Sync, from reading, not from a walk.)
  The Rust `SyncOutcome` has five kinds (`merged` and `conflicted` were added by S11.2a); the
  TypeScript `SyncOutcome` and `syncOutcomeSentence` still had three, so for the other two the
  function fell off the end of its `switch` and `git.lastSync` became `undefined`. Nothing said a
  merge had happened or that files now held conflict markers. **Fixed** the same session: both kinds
  are in the type and have sentences; `git.test.ts` asserts all five.

- **After a file changes on disk and the buffer follows, the Outline (and perhaps the language
  server's view of the file) stays on the old text until the next keystroke.** (Found 10 Nov 2026 on
  the walk of `fixtures/paper/SMOKE.md` §5 step 6: a line inserted from outside left every outline
  heading doubled, one row per source, one line apart, until a character was typed. **Fixed** the same
  session. Cause: only a *save* told TexLab about new text (`afterSave`), and an outside change is
  not one, so the server described the old file. `tellServerTextArrivedFromDisk` now sends
  `didChange`/`didSave` after both places that load disk text into a buffer and then redraws the
  Outline; two controller tests failed first, and the same external edit was re-walked in the app.)

- **The sidebar's `+` asked for the new file's name with the browser's own `window.prompt`, a dialog
  titled "JavaScript - http://localhost:1420/".** (Found 10 Nov 2026 on the walk of
  `fixtures/paper/SMOKE.md` §6.) Off-brand, unstyleable, and `prompt()` is not reliably implemented by
  every webview the app ships on (WKWebView on macOS has historically ignored it, which would make
  *New file* do nothing there; not checked on that machine). It was the only native dialog in the
  frontend. **Fixed** the same session: an inline field under the Files header, with
  `sections/new.tex` as its placeholder; Enter creates and opens the file, Esc or leaving the field
  cancels. First attempt pre-filled the suggestion and selected it; typing then appended to it in the
  webview, so it became a placeholder. Walked in the app.

- **A cold first build shows only "Compiling… N.Ns" for as long as it takes, with no
  "Downloading <package>" suffix.** (Found 10 Nov 2026 on the walk of `fixtures/paper/SMOKE.md` §3
  step 5, with an empty cache: 88 seconds, no suffix once. **Fixed** the same session; see below.) Rule 4
  ("zero setup to first PDF") promises the wait is explained. Two causes: the app runs Tectonic with
  `--chatter minimal`, which suppresses the `note: downloading <file>` lines it prints for each file
  it fetches (checked by running the bundled binary on a cold cache), and nothing translates those
  lines into the "Downloading <name>" the status bar was written to show. The unit test for
  `pump_stderr` only ever fed it a made-up `Downloading amsmath.sty` line. A third cause turned up
  on the first attempt at a fix: Tectonic's command-line interface (the one the app uses) prints
  those notes to **stdout**, which the engine threw away (`Stdio::null()`); only `-X` mode uses
  stderr. **Fixed:** default chatter, stdout piped and drained for progress only, and
  `progress_from_line` turns `note: downloading <file>` into `Downloading <file>` and forwards
  nothing else. Tests: the translation, and a fake engine run through the real `run`, which failed
  first; the ignored real-engine suite (corpus, torture, timings) still passes; and a cold-cache
  build re-walked in the app, where the suffix changed file by file for the whole fetch.

- **Toggling focus mode or typewriter mode from the palette throws the cursor back to the first line.**
  (Found 10 Nov 2026 walking S4.5: cursor on line 28, *Toggle typewriter mode*, cursor on line 1.)
  A mode switch is a display preference; the author's place in the document must not move. Cause:
  the effect in `Editor.svelte` that builds the editor read both flags to pass them as initial
  values, so toggling either re-ran it and rebuilt the whole editor. **Fixed** the same session by
  reading them with `untrack` (the two effects below already push later changes into the live
  view's compartments); re-walked in the app: the cursor stays and typewriter mode centres its line.
  No unit test: it is a reactivity detail of a Svelte component and the repository has no component
  test harness.

- **A "runaway argument" error shows under "Not inside any file" with no line, so clicking it goes
  nowhere.** (Found 9 Nov 2026, same walk, `SMOKE.md` §3 step 2 expects a line number. **Fixed** 10 Nov 2026
  by `place_unlocated_errors`.) The card is honest — it explains that TeX reports the end of the file and the
  tooltip says "TeX did not say which line" — but the group heading names no file although the
  project has exactly one, and the author is given nowhere to go. A better answer for this rule is
  the file the build was started from, at its last line, which is what `place_unlocated_errors`
  (`src-tauri/src/compile.rs`, three tests) now does for any error with neither a file nor a line.

- **Agentic review of the E1–A11 card commits (`97670b9`..`05177e9`), 10 Nov 2026: no defect
  confirmed; three risks left open.** Read in full: `branches.rs`, `tags.rs`, `fragile_command.rs`,
  `latexdiff.rs` (working-copy export), `compile.rs` (BibTeX and placement), the `detect_root` and
  include-scan changes, the branch/tag controller code. Real Tectonic run: `main.blg` is rewritten
  on a text-only rebuild and `Repeated entry---line N of file F` matches `blg.rs`'s parser, so the
  freshness check holds for the bundled engine. **Open, unverified:**
  1. *Switching branch during a live session.* `switchToBranch` saves open tabs first, but nothing
     stops a switch while a live (Yjs) session is open; the files rewrite under the shared document.
     Needs a walk with a session, or a guard that refuses the switch with a sentence.
     **Fixed 10 Nov 2026** by the guard: `switchToBranch` refuses while any live session is open
     (`git.error`: "A live session is open, and switching versions would change the shared text for
     everyone in it. Leave the session first, then switch."). Test with a stubbed `LiveSession`:
     refused with no `switch` verb sent, and the same switch goes through after leaving. Creating a
     branch is not guarded, since it changes no file.
  2. *`read_bibtex_diagnostics` with `latexmk`.* A system `latexmk` run skips BibTeX when `.aux` and
     `.bib` are unchanged, so `.blg` is not rewritten, and a still-true repeated-entry warning
     disappears from the drawer after a text-only edit. Bundled Tectonic does not do this.
     **Fixed 10 Nov 2026.** `read_bibtex_diagnostics` remembers, per `.blg`, when it was written and
     what it said; a build that did not rewrite it repeats that answer while the file's time still
     matches and the root file still names a bibliography (`scan_bib_resources`). Deleting the
     `\bibliography` line clears the card; a `.blg` never read in this session is not trusted. Two
     tests (repeat; never-read and deleted-bibliography), the first red before. Limit: only the root
     file is asked about `\bibliography`, so a bibliography named in a chapter loses the repeat
     (as before this change).
  3. *Working-copy comparison drops symlinked files.* `copy_working_tree` copies regular files and
     directories only, so a symlinked chapter is silently missing from the "working copy" side.
     **Fixed for files, 10 Nov 2026.** A symlink to a file is copied as what it points at (as TeX
     reads it); a link to a folder, and a link to nothing, are still skipped. One test. **The size
     point is not changed, on purpose:** the commit side exports every tracked file of any size too,
     and a size limit would make comparisons fail for projects with large figures; untracked
     non-ignored large files are copied each time, which is the price of "as it is now".

- **Agentic review, second pass (10 Nov 2026): `texlog` rules, latexdiff retry and working-copy
  lane, E6 gate, crash reports (S16.3), relay (S14.1) and the encrypted channel (S14.2). Read, not
  run, except where said. One real defect, the rest risks.**
  - **Real, by reading: a crash report's GitHub URL can exceed its cap.** `issue_url`
    (`src-tauri/src/crash.rs`) cuts lines from the end until the URL fits 6 000 bytes, but stops at
    three lines (`lines.len() <= 3`) and returns whatever is left. The third line is the panic
    message, so one long message (an `unwrap()` on an `Err` whose `Debug` is a few thousand
    characters, which percent-encodes to three times its length) gives a URL GitHub or the browser
    refuses, on exactly the report the author most wants to send. Fix: cut the message line itself
    to fit, as the backtrace is. Needs a test with a 3 000-character message first.
    **Fixed 10 Nov 2026.** `issue_url` keeps the cut loop but, once three lines are left, cuts the
    message a hundred characters at a time (never inside a multi-byte one) until the link fits.
    Test with a 3 000-character message of two-byte characters; it failed first.
  - **Risk: the crash hook reports caught panics too.** `install` replaces the process-wide hook, so
    a panic in a spawned task that tokio or a `JoinHandle` catches, with the app still running,
    writes a report and the next launch asks about "a crash" that did not stop anything.
    **Fixed 10 Nov 2026.** The hook cannot know, so the exit does: on an ordinary exit
    (`RunEvent::Exit`, `lib.rs`) the reports written during the session move to
    `crash-reports/caught/` (kept, not deleted); a real crash never reaches that code and its report
    is still offered. Test on a folder with an old report and two from the session.
  - **Risk: `scrub_home` is a case- and separator-sensitive string replace.** On Windows a path
    spelled `c:/Users/…` against `USERPROFILE=C:\Users\…` keeps the account name. The module's own
    note says the author's read-through is the guarantee, so this is a leak the review step catches,
    not one the scrub prevents.
    **Fixed 10 Nov 2026.** `scrub_home` compares with `\` read as `/` and the case folded, and only
    replaces a home that ends a name (`/home/ada` no longer eats the start of `/home/adam`, which
    the old replace would have turned into `~m`). One test.
  - **Risk: the relay has no resource limits** (`crates/abstract-tex-relay`). Per-peer outbox is an
    unbounded channel, there is no message-size setting beyond tungstenite's default, no cap on
    connections, rooms or peers per room, and no handshake timeout, so sockets that never finish
    the upgrade hold a task each. It binds `0.0.0.0` by default, for a person's own VPS. A slow
    reader in a room makes memory grow without bound. Acceptable for a trusted friends-only
    deployment; worth a bounded channel (drop the peer when full) and a handshake timeout before it
    is described as safe to expose.
    **Fixed 10 Nov 2026.** `Limits` (defaults: 10 s handshake, 16 peers a room, 1 000 rooms, 16 MiB
    a message and frame, 1 024 queued messages a peer, 2 048 connections) applied in `accept`,
    `serve`, `broadcast` and the new `Relay::run` that `main.rs` now calls. A peer whose queue is
    full is dropped from the room and its connection closed; the queue is a bounded channel with
    `try_send`. Five tests (silent handshake, full room, oversized message, stalled reader, the
    connection cap), all written before the code. Not covered: an idle connection that sends nothing
    after joining is still held (no ping/pong timeout), and there is no per-address limit.
  - **Risk: invites default to plaintext `ws://`** (`invite.ts`). The key never crosses the wire (it
    is after the `#`, and independent of the 128-bit room id), so confidentiality holds; but the
    room id, which is the only thing gating entry to a room, is visible to anyone on the path, who
    can then join and flood it. A `wss://` default for any address that is not `localhost` would
    close that.
    **Handled differently, 10 Nov 2026:** not by changing the default, because the relay binary
    speaks no TLS and `wss://` by default would break running one without a proxy (and the existing
    test names `ws://` as the deliberate default). `plaintextRelayNotice` returns a sentence for an
    address that reaches another machine without TLS (not for TLS, this computer, or nothing typed),
    and the Share window shows it under the address field. Three tests; not walked in the app.
  - **Risk: working-copy comparison has no size bound.** `copy_working_tree` copies everything not
    ignored, including large untracked files, on every click; the commit side only exports tracked
    files. A project with a big untracked data folder pays for it each time.
  - **Checked and fine:** key derivation and room id are independent; AES-GCM draws a fresh IV per
    message and nothing reuses one; `dismiss` rejects every path-shaped id; `fragile_command` offers a
    fix only when exactly one candidate is on the line; the E6 gate's skip/require switch matches
    the other `ABSTRACT_TEX_*` flags in the repository.

- **Agentic review, third pass (10 Nov 2026): the remaining areas. Findings for one fix batch
  later; nothing below is fixed yet.** Method: read the code, then reproduce what could be
  reproduced with throwaway probes kept outside the repository (a mutation fuzzer over the
  repository's own `.tex`, `.bib`, `.log` and `.blg` files, about 920 seeds times 60 rounds, and
  three one-off programs). Severity is the reviewer's guess. **R** = reproduced, **S** = by reading.
  Covered: `abstract-tex-comments` (store, anchor, app wrapper), `abstract-tex-github`, the
  credential path in `abstract-tex-git`, `abstract-tex-assistant` (provider, guard, review flow),
  `abstract-tex-snapshot`, `texbib` acquisition, `texwords`, the watcher, the file commands, the
  LSP frame reader, SyncTeX, the CSP and the one `innerHTML` use.

  **High**
  1. **R. `texwords::count_prose` panics on a backslash followed by any non-ASCII character** (a
     stray `\é`, `\ç`, `\—`, or a Windows path in prose; minimal input `"\\𝑥"`, panic at
     `crates/texwords/src/lib.rs:277`: slicing inside a multi-byte character). Its callers are the
     per-compile snapshot (`abstract-tex-snapshot`, `prose_words`) and Source Control's prose
     summary (`abstract-tex-git`, three call sites). A project with one such line can stop getting
     snapshots on every compile, which is the "never press commit" safety net, and panics the
     summary. Fix with a test per call shape; also make the snapshot treat a failed word count as
     "unknown", never as a failed snapshot.
     **Fixed 10 Nov 2026.** `command()` now steps over the whole escaped character (it stepped two
     bytes, which stopped inside a multi-byte one), the multi-byte arm of `scan` uses `get` rather
     than an index, and `group_inner_end` drops a group's last byte only when it really is the
     closing brace (`headings` and `braced_name` both had the "always drop it" rule, so
     `\begin{é` at the end of a file panicked too). Four tests, all three of the first batch
     failing with the original panic messages before the fix; the mutation fuzzer then ran 79 650
     mutated documents (seeded with `\`, `{`, `é`, `𝑥`, `\é`, `\section{ο`) with no panic. Not
     done: making the snapshot treat a failed word count as "unknown". With the panics gone there
     is no known way to reach one, and catching a panic to hide a bug is not something to add
     without a case.
  2. **R. The citation-fabrication guard misses `\textcquote`, `\blockcquote`, `\foreigntextcquote`
     and `\bibentry`.** All take a key, none has "cite" in its name, and `is_citation_command` is a
     substring test. `As shown \textcquote{fake2020}{quote}.` against a `.bib` without that key
     comes back clean. The design calls the guard "not negotiable". Add the names (and test the
     list against biblatex's and bibentry's own command lists), and consider holding any unknown
     command that is followed by a key-shaped brace group in a prose edit.
     **Fixed 10 Nov 2026.** `key_command_shape` in the guard: any name containing `cquote` (with a
     language group first for the `foreign` and `hyphen` forms) and `bibentry`, reading only the first
     key group, because the next group of `\textcquote{key}{words}` is the quoted words and reading
     it as keys would refuse every honest use. Three tests in `tests/citation_guard.rs` (the attacks
     from the reproduction, the honest uses that must pass, a wrapper around one), two of them red
     before. Not done, and recorded here so it is not forgotten: `texbib::is_citation_command`, which
     the editor's own undefined-citation scan uses, still does not see these commands (widening it
     would report the quoted words as undefined citations, so it needs the same shape handling);
     and "hold any unknown command followed by a key-shaped group" was not built, since it would
     refuse honest text.
  3. **S. The guard also learns the author's own citation wrappers from the root file's preamble
     only** (`assistant_review.rs`, `preamble_of(root_text)`). A `\see` or `\citeas` defined in an
     `\input`ed preamble file, a `.sty` or a `.cls` is invisible, so `\see{invented}` passes. Read
     the include graph's files and the project's own style files.
     **Fixed 10 Nov 2026.** `macro_sources` (`assistant_review.rs`) reads every file of the include
     graph plus the project's own `.sty`, `.cls` and `.def` files (four folders deep, never `.git`
     or our folder, nothing over 2 MB); a `.tex` file the document does not read is left out on
     purpose. Two tests with a project in a temporary folder (wrappers in an `\input`ed preamble, a
     local package and a chapter all found; one in an unread file not), red against the old
     root-preamble-only behaviour. `preamble_of` had no caller left and was removed.
  4. **S. One malformed comment record blanks every comment.** `read_tree_entries`
     (`abstract-tex-comments/src/store.rs`) returns on the first JSON error, so a blob a coauthor's
     newer version wrote, or a damaged one, turns the whole Comments view into an error. Skip the
     bad entry and say so.
     **Fixed 10 Nov 2026.** `read_tree_entries` skips an entry that does not parse (a `tracing`
     warning names it; it stays in the tree for a version that understands it). Test: a comment
     plus two damaged blobs, one named like a comment and one like a resolution; the comment loads.
     It says so only in the log, not in the panel: a visible "one comment could not be read" is
     left for the Comments view's next pass.

  **Medium**
  5. **R. `texwords::headings` panics on a heading line whose `{` is never closed and which ends in
     a non-ASCII character** (`"\\section{ο"`, `lib.rs:140`: `inner_end` is `end - 1`, a byte index
     inside the last character). Reached from Source Control's summary (`git/lib.rs:940`).
     **Fixed 10 Nov 2026** together with item 1 (`group_inner_end`).
  6. **R. Snapshots follow symlinks**, against what their own comment says. `tree_for_folder` tests
     `path.is_dir()` and `is_file()`, which follow links, so a link inside the project copies
     whatever it points at into the snapshot (a link to `outside/secret.txt` and one to a folder
     both came back from `read`), and a link such as `sub/up -> ..` is walked about forty levels
     deep before the OS stops it. Use `symlink_metadata`.
     **Fixed 10 Nov 2026.** The walk takes the entry's own type from `symlink_metadata`; a link is
     neither a folder to enter nor a file to store. Two tests (a link to a file and one to a folder
     outside the project; a link back up the tree), both red before; the second one printed the
     forty nested copies. A symlink is now simply absent from a snapshot, as the old comment said.
  7. **S. `comments::list` reads each anchored file twice** (once inside `load`, once to convert
     offsets), then slices `text[..byte]`. If the file changes between the reads (autosave every
     700 ms, a branch switch), a byte offset can be out of range or inside a character and the
     command panics. Convert from the same text the offsets came from.
     **Fixed 10 Nov 2026.** `list_with` reads each file once per listing through a cache and places
     every comment against that copy; test: a reader that returns the whole file the first time
     and a stub the second (it panicked at the slice before the change).
  8. **S. Re-anchoring is quadratic.** `context_matches` calls `last_chars(&text[..start], n)`, which
     collects the whole prefix into a `Vec<char>` for every occurrence of the quote. A comment on a
     common word in a long chapter does that thousands of times on every list. Take the last `n`
     characters from the end with `char_indices().rev()`. An empty quote from a hostile or damaged
     record matches at every position and makes it worse; refuse empty quotes on load.
     **Fixed 10 Nov 2026.** `last_chars` walks back from the end; `reanchor` answers an empty quote
     "not found". Tests: 4 000 occurrences of a word in 168 KB re-anchored in under a second (it
     was the failing case), the exact-context comparison still behaves (including multi-byte
     characters), and the empty quote.
  9. **S. A comment's `file` is trusted on read.** `list` does `root.join(relative)` on a string a
     coauthor wrote; an absolute path or `..` makes the app read any file the user can, and the
     answer (anchored, orphaned, ambiguous, and the offsets) is a yes/no oracle on its contents.
     Run it through `Project::resolve`'s rules before reading.
     **Fixed 10 Nov 2026.** `is_inside_project` (empty, absolute, `..`, drive prefix and backslash
     all refused) guards both `list`, which answers "file missing" without reading, and
     `add_comment`, which refuses with a sentence. Three tests.
  10. **S. No lock around comment writes, and the ref is updated with force.** `write_blob_on_ref`
      reads the tip, builds a commit, then overwrites the ref. The commands run on blocking threads
      with no lock (unlike `AT_A_TIME` for snapshots), so two overlapping writes (a double click, a
      resolve during a sync) can drop one. Use a compare-and-set or a mutex.
      **Fixed 10 Nov 2026.** Writes build on the tip they read and move the ref with
      `reference_matching` (a compare-and-swap); a write or a merge that finds the ref moved or
      locked rebuilds on the new tip, up to 40 times with a short growing pause. Tests: eight
      threads times six comments (all 48 present; before the fix libgit2's lock errors failed the
      writes), and a merge racing 25 adds (it lost exactly one comment, three runs of three, before
      the fix).
      **Correction, same day, found by the full `pnpm verify` and not by the focused runs:** the
      compare-and-swap alone was not enough. `writes_made_at_the_same_time_all_survive` lost one
      comment of 48 in roughly four runs in ten once the whole workspace was loading the machine
      (every `add` returned `Ok`, so a swap succeeded against a tip that was no longer current;
      libgit2's check of the expected value is evidently not airtight here, cause not pinned down).
      Writers of one process are now also serialised by a lock (`ONE_WRITER_AT_A_TIME`, taken by
      `write_blob_on_ref` and `merge_from`, as the snapshot crate's `AT_A_TIME` does): 0 failures in
      40 runs of the crate's tests. The swap and the retry stay for a second process. **Open:** a
      second writer in another process (another window, a `git` command) is still only protected
      by the swap, which has just been shown to be leaky.
  11. **S. The ISBN lookup builds follow-up URLs from the response.** `"https://openlibrary.org{key}.json"`
      with `key` taken from the JSON (`isbn.rs`): a key of `@evil.example/x` or `.evil.example`
      changes the host. Needs a hostile or intercepted response, but the fix is one `starts_with`
      check (`/works/`, `/authors/`).
      **Fixed 10 Nov 2026.** `is_record_key` accepts only `/works/OL<digits>W` and
      `/authors/OL<digits>A`; anything else is not requested (a hostile author key is skipped and
      the honest ones still resolve). Two tests; the first showed the request going to
      `https://openlibrary.org@evil.example/x.json` before the change. Note for anyone running the
      tests: `texbib`'s acquire tests need `--features acquire` when run on their own.
  12. **S. Project paths are checked lexically only** (`Project::resolve`). A symlink inside a cloned
      project that points outside it is followed by `read_file`, and `create_file`'s
      `create_dir_all` creates folders through it. Clone is a feature (S11.5b), so a repository an
      author did not write is in scope. Refuse a resolved path whose existing ancestors leave the
      project.
      **Fixed for writes, 10 Nov 2026; reads deliberately unchanged.** `Project::resolve_for_write`
      canonicalises the nearest existing folder above the path (a dangling link counts as a link)
      and refuses if it is outside the project; `write_file`, `create_file`, `snapshot_restore`,
      the paste-to-cite write, the Zotero export path and `snapshots::restore_file` use it. Only the
      folder is checked, not the file name: a write is atomic and replaces a link that is itself
      the file, so a `refs.bib` linked to a shared export keeps working (tested), as does a link
      to a folder inside the project. Reading through a link out of the project still works, on
      purpose, for the same shared-folder reason; the exposure that remains is a clone whose link
      points at a file the author then opens, which shows them a file they can already read.

  **Low and design notes**
  13. **S. `abstract_tex_comments::open` needs the repository root exactly** while Source Control
      discovers it, so a thesis in a subfolder of a larger repository is told "this folder is not a
      Git repository yet" by Comments only. The new `project` module (`open_or_create`, discover,
      then an own bare repository) is not called by the app. Pick one rule and delete the other.
  14. **S. Comment and resolution identity is unauthenticated.** `author` and `created` are
      whatever the writer put there, and a resolution with a very large `created` wins for ever.
      Acceptable among trusted coauthors; say so in the design.
  15. **S. The guard's plain-text rule accepts `[12]`-style numeric references** and any citation
      form it has no pattern for; the design already calls it a heuristic.
  16. **S. DOI lookups** send no user agent (the arXiv and ISBN ones do) and read the whole body with
      no size cap; the SyncTeX gunzip and the LSP `Content-Length` are likewise unbounded, and
      `body_start + length` can overflow on a hostile length. All three sources are local or
      trusted; bound them anyway.
      **Fixed 10 Nov 2026.** LSP: `Content-Length` over 256 MiB is a protocol error (the test
      showed the `attempt to add with overflow` panic first). SyncTeX: the gunzip stops at 256 MiB
      and answers `TooLarge` (test with a 4 MiB expansion against a 1 MiB limit; the app maps it to
      the existing "could not be read" sentence). Bibliography replies (DOI, arXiv, Open Library):
      refused beyond 2 MiB, DOI requests now carry the same named user agent the others do.

  **Checked and fine:** the GitHub token is offered only to `https://github.com` (user-info and
  look-alike hosts included) and only once per operation; the assistant client follows no
  redirects, refuses to send a key over `http://` to another machine, and shows what it will send;
  keychain use matches its own tests; snapshot restore goes through `Project::resolve`; the
  watcher's echo filter forgets a hash once the file differs; KaTeX renders with `trust` off and
  the CSP is tight; and the fuzzer found **no panic** in `texbib::parse` and `render_entry`,
  `texlog::diagnostics`, `quick_errors` and `bibtex_diagnostics`, `abstract_tex_includes::scan_includes`,
  `Guard::check`, or the comment anchor functions.

  **Not reviewed in any pass:** `abstract-tex-lsp` beyond framing, `abstract-tex-reconcile`,
  `abstract-tex-templates`, `abstract-tex-sidecar`, the engine crate's process handling,
  `abstract-tex-git`'s sync and merge internals, `bibliography.rs`, `paste.rs`, `assistant.rs`,
  and the frontend's editor, PDF viewer and accessibility pass (S16.4).

- **Agentic review, fourth pass (10 Nov 2026): the areas the third pass listed as unreviewed.
  Findings for the same later fix batch; nothing below is fixed yet.** **R** = reproduced with a
  throwaway probe outside the repository, **S** = by reading. Covered: the rest of
  `abstract-tex-git` (stage, discard, commit, push, sync, merge, clone), `abstract-tex-engine`
  (`process.rs`, `latexmk.rs`), `abstract-tex-reconcile`, `abstract-tex-templates`,
  `abstract-tex-sidecar`, `abstract-tex-lsp` (bridge, server), `paste_cite` and the Zotero commands,
  `bibliography.rs` path handling, `document.ts` (save and external change), and the dialogs.

  **High**
  1. **R. A project's `.latexmkrc` runs as code on every build with a system engine, with no
     consent.** The app calls `latexmk` with no `-norc` and, when shell escape is off (the default),
     with the project folder as the working directory (`latexmk.rs`, `invocation`). latexmk reads
     `.latexmkrc` there and executes it as Perl; with the same flags the app uses, a rc file of
     `system("touch ../PWNED_BY_RC");` created the file. The engine is chosen by `abstract-tex.toml`
     in the project (`engine = "pdflatex"`), and that file arrives with a cloned repository, which is
     a feature (S11.5b). So cloning a repository that carries both files and opening it runs the
     author's code on a machine with latexmk installed. `consent.rs` states the opposite premise
     ("nothing a project carries may turn this on"). The comparison lane builds an exported copy of
     the repository the same way. Fix: pass `-norc` (and test that a rc file in the project is
     ignored), and say in the design that a project may name a system engine but never its
     configuration.
     **Fixed 10 Nov 2026.** `-norc` is the second argument of every `latexmk` run
     (`latexmk.rs`, unit test over all three programs and both shell-escape modes), plus an
     `#[ignore]`d test against the real program (`tests/latexmk.rs`,
     `a_project_rc_file_is_never_run`) whose rc file plants a marker: it failed with the flag
     removed (marker created) and passes with it. DESIGN.md §5.1 now says a project may name an
     engine but never configure it. Side effect to know about: the user's own `~/.latexmkrc` is
     skipped too, so a custom engine flag set there no longer applies.
  2. **R. `diff_ops` is slow on a large change, and runs on the UI thread.** The Myers character
     diff in `abstract-tex-reconcile` has no deadline. In release builds, a text of 81 KB with a
     third of its lines rewritten took 4.7 s, with every line rewritten 35 s, and a 244 KB case
     had not finished after about four and a half minutes. `commands::diff_ops` is a plain (not
     `async`) `#[tauri::command]`, which Tauri runs on the main thread, so the whole window freezes.
     It is called for every external change to an open file (a branch switch from B4, a pull, a
     snapshot restore). Fix: give `similar` a deadline and fall back to the single replace the
     module already uses for small changes, and make the command `async` with `spawn_blocking`.
     **Fixed 10 Nov 2026.** `DIFF_BUDGET` (150 ms) is passed to `similar`'s `timeout`, which makes
     it finish the rest as one block, so the result is coarser but exact (test: a 300-line document
     with every third line rewritten failed at 6.7 s in a debug build against a 1 s bound and now
     passes; the round trip through `apply_ops` is asserted). `diff_ops` is `async` on
     `spawn_blocking`.

  **Medium**
  3. **R. Git verbs act on unvalidated paths.** `discard` (and `stage`, `unstage`, `diff_sides`) join
     the path the window sent onto the working directory with no `Project::resolve`. Run against a
     repository, `discard` of an absolute path and of `../outside/precious.txt` both returned
     `Deleted` and the file was gone. Also, an error from `status_file` is read as "untracked", so
     a tracked file whose status cannot be read is deleted instead of restored. The webview is our
     own code, so this needs a compromised or buggy frontend, but `Project::resolve`'s own comment
     says every write command is checked and these are not.
     **Fixed 10 Nov 2026.** One `inside_project` check (empty, absolute, drive prefix, `..`, or
     inside `.git`) now guards `stage`, `unstage`, `discard` and `diff_sides` (which had it inline,
     without the `.git` rule); `discard` deletes only when Git says "new in the working tree" or
     has no status at all, and returns any other error. Three tests (the reproduced absolute and
     `../` paths plus `.git/config` and `.git`, across all three verbs; a folder path; a file that
     is already gone). Side note found by the clippy gate: `Option::is_none_or` needs Rust 1.82 and
     the workspace floor is 1.80.
  4. **S. `paste_cite` hides its write from the watcher.** It rewrites the `.bib` with
     `remember_write`, so the echo filter drops the change and an open tab holding that `.bib` is
     never reloaded (snapshot restore deliberately does the opposite, `snapshots.rs`). The next
     autosave from that tab then writes its stale buffer over the file and the pasted entry is
     gone, while the `\cite{key}` that was inserted stays. It also reads the file and writes it in
     two separate steps, so an edit saved in between is lost.
     **Fixed 10 Nov 2026.** `paste::append_new_entry` reads, renders and writes in one call made
     inside a single hold of the project lock (the editor's `write_file` takes the same lock), and
     the command no longer calls `remember_write`, so the change reaches an open tab as any outside
     edit does (applied if clean, asked about if dirty). Two unit tests (two pastes, distinct keys,
     nothing lost; an unreadable file is an error and nothing is created). Not covered by a test:
     that the watcher now reports the write, which is true by construction (nothing registers it)
     and is the existing external-change path that the controller tests already exercise.
  5. **S. A failed autosave is silent.** `scheduleSave` runs `void this.save()`; when `writeFile`
     rejects (disk full, read-only file, folder gone) the rejection is unhandled, no notice is
     shown, and nothing retries until the next keystroke. `save` clears its timer before the write.
     **Fixed 10 Nov 2026.** `OpenDocument.autosave` catches the failure, calls `onSaveError` once,
     keeps the buffer dirty and retries every 5 s until a write lands, then calls `onSaveRecovered`;
     it stops when the document is disposed. The controller turns these into a notice ("Could not
     save main.tex: …") and takes it back on recovery. Three tests (document, retry-stops-on-dispose,
     controller), the controller one checked by removing the wiring.
  6. **S. `applyExternal` gives up quietly.** After three attempts in which the author kept typing
     while Rust diffed, it falls through to `lastSavedText = diskText`, leaving the typed text in the
     buffer, so the next save overwrites the external change with no question asked (the conflict
     bar is for the dirty-at-arrival case only). Finding 2 makes the window long enough to matter.
     **Fixed 10 Nov 2026.** `applyExternal` returns whether the change landed and records nothing
     when it did not; the controller then raises the same conflict bar a dirty buffer gets, with
     saves held, from both the watcher path and "load from disk". Three tests (document, watcher
     path, load-disk path), the controller one checked by removing the guard.
  7. **S. Cancelling a build kills only the direct child.** `process.rs` calls `child.kill()` and
     sets `kill_on_drop`; latexmk's own children (pdflatex, bibtex) are not in a process group and
     keep running and writing into the build folder, and their open pipes keep the two stderr/stdout
     pump tasks alive. There is also no wall-clock limit on a build, and both pumps collect all
     output in memory (and a line with no newline grows `pending` for ever), so a runaway
     `\loop` that prints grows memory until someone cancels.
     **Fixed 10 Nov 2026** (`process.rs`). The engine runs in its own process group on Unix
     (`process_group(0)`) and `libc::kill(-group, SIGKILL)` ends the whole tree on cancel, on the
     time limit, and if the future is dropped (a `ProcessTree` guard); on Windows `taskkill /T /F`,
     **written but not run, there is no Windows machine here**. `BUILD_TIME_LIMIT` is 15 minutes and
     ends in `EngineError::TimedOut` with a sentence of its own; the two output readers are aborted
     on those paths. Output is kept as its last megabyte (`TailOfOutput`, first line says how many
     earlier lines were left out) and a line with no newline is split at 64 KiB. Four new tests,
     each red on its own behaviour first (the grandchild `sleep` survived the old kill); the real
     Tectonic and latexmk ignored tests still pass. The 15-minute figure is a judgement, not a
     measurement: the longest build on record here is the 90 s cold first build.
  8. **S. Two modal dialogs miss the focus rules.** `BibliographyHealth.svelte` and
     `ZoteroLink.svelte` are `role="dialog"` over a backdrop but neither uses `use:modalFocus`
     (`lib/dialog.ts`) and neither handles Escape, which is the bug `dialog.ts` says it was written
     to fix: focus stays on the editor behind, so typed text lands in the manuscript. Rule 5
     (keyboard first).
     **Fixed 10 Nov 2026.** Both now use `modalFocus` and close on Escape (the handler sits on the
     dialog, where `modalFocus` puts focus). `src/components/dialogs.test.ts` is a guard over the
     markup (the repository has no component harness): every `role="dialog"` over a `.backdrop`
     must close on Escape, set `aria-modal`, and use `modalFocus` unless it is one of the two
     palettes that focus their own input. It failed 17 ways first. **Not walked in the running
     app** (no Xvfb run this pass): the next by-hand walk should open the Bibliography panel and
     the Zotero window, press Tab and Escape.

  **Low and design notes**
  9. **R. Sync with something staged shows libgit2's own sentence**, not ours: `Git reported: 1
     uncommitted change would be overwritten by merge; class=Merge (22); code=Conflict (-13)`.
     Nothing is lost (the merge refuses, and an unstaged edit is kept and not pushed; both checked),
     but a staged file blocks a diverged sync with no explanation in plain words. The merge commit
     is titled `Merge commit '<sha>'`, not the "Merge branch 'origin/main'" the code comment promises.
     **Partly fixed 10 Nov 2026.** `GitError::SyncNeedsCommit` ("…while you have changes that are
     not committed. Commit them (or discard them), then sync again. Nothing was changed.") replaces
     libgit2's text for a merge or a fast-forward it refused; test: a staged edit blocks a diverged
     sync, the sentence has no `class=`, and the head, the remote and the staged edit are untouched.
     The merge commit's title was not changed (cosmetic; the comment that promised the other
     wording is the thing to correct, left for the next pass over `merge`).
  10. **S. `amend` in the git crate has no "already pushed" check**; the design leaves it to the panel
      (`ahead_behind`). A second caller would amend published history.
      **Fixed 10 Nov 2026.** `GitError::AlreadyPushed`: `amend` refuses when the newest commit is
      on `refs/remotes/origin/<branch>` (equal to its tip or an ancestor of it). Two tests (refused
      after a push with history untouched; allowed for a commit made after it).
  11. **S. `resolve_bib_argument` treats any dot in a name as an extension**, so `\bibliography{refs.v2}`
      is looked up as `refs.v2` and reported missing, where BibTeX looks for `refs.v2.bib`.
      **Fixed 10 Nov 2026.** `.bib` is appended unless the name already ends in it (case
      insensitively). One test over seven spellings.
  12. **S. LSP: `request` has no timeout**, and a caller that is dropped while waiting leaves its entry
      in `pending` until the server answers, which a cancelled request never does.
      **Fixed 10 Nov 2026.** `Bridge::request_within` (default `REQUEST_TIMEOUT`, 120 s) ends in
      `CallError::TimedOut` and removes the entry; an `EntryGuard` removes it when the caller's
      future is dropped. Two tests against a new `silent` method of the fake server, both red first
      (the table kept the entry).
  13. **S. `sidecar::find_on_path` accepts any file** (not only executables) and an empty `PATH` entry
      resolves a bare name against the current directory.
      **Fixed 10 Nov 2026.** `sidecar::find_in`: executable files only (the execute bit on Unix) and
      empty entries skipped. One test; it changes the working directory of the test process, which is
      why the sidecar crate's tests must not start relying on relative paths.
  14. **S. No dialog sets `aria-modal`**, so a screen reader's virtual cursor can leave one although
      `modalFocus` keeps keyboard focus inside.
      **Fixed 10 Nov 2026** with item 8: `aria-modal="true"` on all thirteen dialogs, enforced by
      the same test.

  **Checked and fine:** template fields are LaTeX-escaped and written through a staging folder renamed
  into place; folder names from titles and `clone_destination` refuse separators, `..` and the
  characters Windows refuses; a failed clone cleans up only what it created; sync keeps an unstaged
  edit and does not publish it (reproduced); the token is still offered only to github.com; the
  shell-escape consent file is outside any project and fails closed; `modalFocus` traps Tab and gives
  focus back; the reconcile ops are correct in UTF-16 (the module has a property test) and differ
  from the slow case only in speed; the LSP supervisor clears waiting requests and restarts the
  server a bounded number of times.

  **Still not reviewed:** the PDF viewer and the editor extensions beyond `document.ts`, the SyncTeX
  frontend, `assistant.rs` and `assistant_explain.rs`, `tectonic.rs`, `incremental.rs` and `draft.rs`
  inside the engine crate, the correctness of the `texlog` rule catalog's wording, and the
  `texbib` parser beyond what the fuzzer reached.

- **Agentic review, fifth pass (11 Nov 2026): the areas the fourth pass listed as unreviewed,
  continued. Findings are logged as they are found; the fix batch follows the list.** Method as
  before: read the code, then reproduce on the virtual display (`scripts/xvfb-app.sh`, a planted
  crash report, a shim for `xdg-open`) where a screen can show it. R = reproduced, S = by reading.
  Area 8 of 8 first: `lib.rs` start-up, `capabilities/default.json`, `tauri.conf.json`, the assets
  scope, and `assistant.rs` / `assistant_explain.rs`.

  1. **R. The app can never open the person's browser.** `capabilities/default.json` grants
     `opener:allow-open-url`, which the plugin describes as "the open_url command *without any
     pre-configured scope*", and with no scope every URL is refused (`ForbiddenUrl`). `opener:default`
     (which adds `allow-default-urls`) was never granted. Both callers are affected: the GitHub
     sign-in page (`openVerificationPage` swallows the error on purpose, because the code is on
     screen, so the button just does nothing) and *Report on GitHub…* on the crash dialog. Reproduced
     on the virtual display with a planted report: the dialog shows "Could not open your browser: Not
     allowed to open url https://github.com/…/issues/new?title=…" and the `xdg-open` shim was never
     called. S16.3's note said the opener was "not walked … the same call `openInBrowser` already
     makes for the GitHub sign-in page"; neither call had ever been walked.
     **Fixed 11 Nov 2026.** `capabilities/default.json` now grants `opener:allow-open-url` *with* an
     address list: GitHub's device page and this project's new-issue page, nothing else (the opener's
     own `default` would have allowed every web address and "reveal in folder"). New
     `src-tauri/tests/capabilities.rs` reads the real file and matches the real addresses
     (`crash::issue_url`'s output; the device page) with the plugin's own `glob` matching; two tests
     were red first, and a third pins that other addresses (another repository, `http:`, `file:`,
     a look-alike host) stay refused. Walked again on the virtual display: *Report on GitHub…* now
     calls `xdg-open` with the full address and the report moves to `handled/`.

  2. **S. A project can make `.abstract-tex` a link, and the app then writes through it.**
     `Project::open` does `create_dir_all(.abstract-tex/build)` and writes `.abstract-tex/.gitignore`
     through whatever the name points at; `latexdiff` clears `.abstract-tex/latexdiff` before each
     comparison; `open_project` widens the asset-protocol scope to that folder, and Tauri also
     allows the link's *target*. A cloned repository can carry such a link (same door as R3.12). Not
     reproduced; the symlink behaviour of `create_dir_all` is the whole of it. Fix: refuse, with a
     sentence, a project whose `.abstract-tex` is a link.
     **Fixed 11 Nov 2026.** `Project::open` refuses a state folder that is a link (`symlink_metadata`),
     before anything is created, with a sentence that says to remove or rename it. One test (nothing
     is written into the target), mutation-checked.
  3. **S. `read_file` reads whatever the name points at, whole, on the UI thread.** `std::fs::read`
     with no size ceiling and no check that the target is a regular file: a project file that is a
     link to a FIFO or `/dev/zero`, or a 2 GB data file clicked in the tree, blocks or exhausts the
     app (sync `#[tauri::command]` runs on the main thread). Whether the tree offers non-text files
     to open is a frontend question (area 7 below).
     **Fixed 11 Nov 2026.** `project::read_text`, used by `read_file`: it asks `metadata` first
     (a regular file only; at most 32 MB; no NUL byte, so an image, a PDF or UTF-16 text is refused
     with a sentence instead of opening as garbage that a save would write back) and only then reads.
     The red run of its test was killed by the OS: the unfixed code read `/dev/zero` into memory
     until none was left, which is the finding itself. Three tests (a device and a folder, a sparse
     file one byte over the limit, a PNG header and a Latin-1 file that still opens).
  4. **S (design gap). The PDF pane has no text layer and no link layer.** `DESIGN.md` §4 chose
     pdf.js because its "text layer gives us search, selection and copy for free"; `viewer.ts`
     renders canvases only (no `TextLayer`, no annotation layer, no find controller anywhere in
     `src/`). So text in the preview cannot be selected, copied or searched, and `\href`, `\url`,
     `\ref`, `\cite` and table-of-contents links in the PDF do nothing. The comment on
     `onInverseSearch` ("a single click inside the text layer is how pdf.js's own selection … already
     work") describes something that is not there. Not a crash; a promise in the design that is not
     kept and not recorded as deferred.
     **Fixed 11 Nov 2026 (find-in-PDF not done).** The pane is now pdf.js's own `PDFViewer`
     (`src/lib/pdf/viewer.ts`, rewritten), which keeps a text layer and a link layer on every page.
     Walked on the virtual display with a 308-page document: dragging selects text (the blue
     highlight is the browser's), a table-of-contents link scrolls to its page, and a link to a web
     page calls `xdg-open` with its address while the window stays where it is. A click on a link is
     decided by `geometry.linkAction` (http and https go to the browser through `openPdfLink`;
     `#…` is pdf.js's own; `file:`, `javascript:`, `mailto:` and the rest are dropped, and nothing
     can navigate the window). The opener's capability grew from two addresses to every `http:` and
     `https:` address (a click is the trigger, and `file:`/`mailto:`/`javascript:` stay refused; the
     capability test says both), and `no_silent_network.rs` now reads permission identifiers as
     JSON instead of searching the file for `"http:`, which the new scope entry matched as data.
     Six geometry tests and one controller test, red first; the DOM half of `viewer.ts` has no
     automated test (the test environment has no DOM) and is covered by the walk.
     **Find in the PDF, done 15 Nov 2026.** `Ctrl F` with focus in the PDF pane, `Ctrl Shift F`
     from anywhere, or "Find in the PDF" in the palette, opens a bar over the pane's top corner (it
     does not shift the pages): every match is highlighted (yellow, the current one orange), the bar
     says "3 of 12" or "No matches", Enter / Shift Enter move on and back and wrap, and there are
     Match case and Whole words switches. Words selected in the PDF start the search. It is pdf.js's
     own `PDFFindController`; `src/lib/pdf/find.ts` (`FindSession`) speaks its event protocol and
     `find-controller.ts` takes over where the view goes: a match out of view is put a third of the
     way down the pane (pdf.js's own default is flush against the top edge, with the sentence around
     it out of sight), and a search re-run by a rebuild moves the view nowhere, so a reader on
     page 25 stays on page 25 and the count follows the new document (walked: "4 of 7", rebuild,
     "4 of 8", same page). Tested against pdf.js's real controller on a real PDF built in the test
     (37 new tests, then 11 for the polyfill below; mutation-checked: whole-word, previous, close, wrapping, both holds on the view, the
     reveal rule). Known limits: no regular expressions; no "highlight all" switch (all are always
     highlighted); after a rebuild the current match is the first one at or after the page being
     read, not the one that was current. (A word hyphenated at the end of a line is found: probed, and
     "photograph" matches "photo-" / "graph".)
     **Found while testing the find bar, fixed 15 Nov 2026: pdf.js 5.7 needs `RegExp.escape`, which a
     webview older than 2025 lacks, and nothing says so.** pdf.js builds its search pattern with
     `RegExp.escape` for any query with punctuation in it ("non-trivial", "e-mail", "U.S.A.", "C++",
     "O(n)", "[12]"). Where the function does not exist it throws inside a promise nobody sees, no
     result event is ever sent, and the bar says "Searching…" for ever. Reproduced in Node 22 (no
     `RegExp.escape` there) with the real controller: 27 of 29 punctuation queries never answered.
     This machine's WebKitGTK (2.52.6) has the function, so the walk could not have shown it; the real
     app searched "well-known" and "U.S.A." fine. Webviews that lack it: Safari before 18.2's WebKit
     (macOS), WebKitGTK before about 2.48 (Ubuntu 24.04 ships 2.44), Chromium before 136. **Fixed:**
     `src/lib/pdf/regexp-escape.ts` follows the specification (a hyphen must be `\x2d`, because
     pdf.js's patterns carry the `u` flag) and installs itself only where the function is missing, from
     `find-controller.ts`. 11 tests, red first (the hang reproduces as "searching"), mutation-checked
     (not installed, no hex for a leading letter, replacing a native one). **Not verified:** a real old
     webview. **Open question for the maintainer:** the oldest webview the app supports is not pinned or
     tested anywhere, and the modern pdf.js build assumes recent engines; `pdfjs-dist/legacy` is the
     alternative if older WebKitGTK or macOS must work.
  5. **S. Every page is rendered eagerly, and the new PDF appears only when the last page is done.**
     `viewer.ts` says so in its header ("a later loop, noted in SPRINTS.md") but SPRINTS.md has no
     such note. Memory is a canvas per page at the screen's pixel ratio (about 5 MB a page at a ratio
     of 2 in a 560-pixel pane, 22 MB at 4× zoom); the swap is at the end, so a long thesis shows the
     old pages until the whole new set exists, which counts against the "<2 s warm recompile"
     commitment as the author sees it. Ctrl+wheel re-fetches and re-renders everything on every
     tick (older renders are cancelled by the token, so it is wasteful, not wrong). Also: a PDF that
     fails halfway through rendering leaves its `PDFDocumentProxy` undestroyed. Not measured.
     **Fixed 11 Nov 2026**, with item 4: `PDFViewer` draws only the pages near the window and keeps a
     few; a new PDF is opened in a second, hidden viewer in the same place and takes over once the
     pages in view are drawn (4 s at most), so the pane still never flashes; zoom is applied at once
     with no re-fetch; a failed load destroys what it made. Measured before and after with the same
     script on a 308-page document (a screenshot poll from the build-finished log line to the new
     page on screen): 5.49 s, 5.49 s, 5.49 s with the old viewer; 0.62 s, 0.65 s, 0.69 s with the
     new. Walked: scroll, Ctrl+wheel zoom, forward search (the page and the highlight) and inverse
     search (a double-click on the second paragraph put the editor on line 10).
  6. **R (at the crate boundary). SyncTeX forward and inverse search do nothing when the project is
     opened through a symlink** (a mounted data disk, `~/thesis` → elsewhere, macOS `/tmp` and
     `/var`). The engines record the folder TeX saw (`getcwd`, physical) in `Input:`; the app's
     `root_dir` is `std::path::absolute` (logical, deliberately not canonicalised). Probe with a real
     `latexmk` `.synctex.gz`: `forward_search` through the physical path answers, through the linked
     path answers `None`; the inverse hit's file does not `strip_prefix` the linked root, so
     `to_relative` returns `file: None`. Both callers swallow it on purpose ("nothing typeset for this
     line"), so the feature just never works for that author. Fix: compare canonical forms on both
     sides.
     **Fixed 11 Nov 2026.** `synctex::forward_in_project` tries the path as the project spells it
     and then with every link resolved; `to_relative` tries the root as opened and then resolved;
     `synctex_forward` uses the first. One test with a real fixture relocated into a folder reached
     through a link, red first for the forward half, and the inverse half mutation-checked.
  7. **S. The explain request's scrub removes the project folder only**, although the comment on
     `without_project_path` says it is so "a path to the author's home folder or user name does not
     leave". Any other absolute path the log holds (a figure from `~/Pictures`, a package under
     `~/.texlive`, a font cache) goes out with the user name in it. The payload dialog is always shown
     for this request, so the author can see it, which is why this is low. `crash.rs` already has the
     right tool (`scrub_home`).
     **Fixed 11 Nov 2026.** The excerpt goes through `crash::scrub_home` after the project folder is
     removed (`crash::home_folder()` names the home; the project stays `<project>`). One test, red
     first.
  8. **S. A remembered explanation is trusted although the folder it lives in can come from someone
     else.** `.abstract-tex/cache/explain/<hash>.json` is read back as "an answer to this same
     problem from earlier on this computer" with no request made. The folder is self-ignoring in Git,
     but a zipped project carries it, and the hash is computable. The effect is a planted sentence
     shown as the assistant's.
     **Fixed 11 Nov 2026.** The memory moved out of the project: `<app config>/explain-cache`, beside
     `assistant.json`, one folder for every project (an answer is about the problem, and its signature
     already has the project's folder taken out). A project can no longer carry an entry that is
     read. The module's "must never" rule says so. One test (the exact location, and a planted
     entry in the project's old place is not found). Answers remembered in old projects are simply
     never read again, which is harmless.
  9. **S. Opening any project reads the OS keychain once a provider has ever been chosen**, on the UI
     thread, whether or not the assistant is on for that project: `refreshAssistant` runs from
     `openProject` and `assistant_status` computes `has_key` unconditionally. Where the keychain asks
     for a password (macOS, a locked Secret Service) the prompt appears on every project open, for a
     feature the author did not switch on, and a slow keychain stalls the window. The key is needed
     only when the settings view is open or a request is about to be built.
     **Fixed 11 Nov 2026.** `assistant::status` takes `include_key`: the keychain is read only when
     the settings view asks, or when the assistant is on for the open project (it needs the answer to
     say whether it is ready). `refreshAssistant` passes `false` on project open and `true` from the
     settings view and its saves. Tests, red first: a counting key store in Rust (no read for a
     project it is off for, one read each for the other two cases) and one in the controller tests
     (what `openFolder` and `openAssistantSettings` ask for). Not done: the remaining reads still run
     on the main thread.
  10. **R. A project's root file named `-shell-escape` turns shell escape on, with no consent.**
      High. The engine builds the command line as `latexmk … <root file>`; the root comes from
      `abstract-tex.toml` (`root = "…"`), checked only with `is_file()`, and it is passed bare. A file
      named `-shell-escape` in a cloned repository, with `engine = "pdflatex"`, makes latexmk read the
      last argument as its own option (and, with no file named, build every `.tex` in the folder with
      `-shell-escape` on). Reproduced in the real app on the virtual display: the project's
      `main.tex` carried `\immediate\write18{touch …}`, the app's own log shows
      `args=["-pdf","-norc",…,"-synctex=1","-shell-escape"]` for a job whose consent was never given,
      and the marker file existed after the project opened (the build runs on open). Same class as
      R4.1 and the same door: the setting that selects the program arrives with the folder. Tectonic
      is not exploitable this way (the lone positional is consumed as the option's value and the build
      fails with a clap usage error), but a root file whose name starts with `-` also breaks it.
      Fix at the engine boundary (a path that starts with `-` goes as `./<path>`), and refuse such a
      root in the project model.
      **Fixed 11 Nov 2026.** `file_argument` in the engine crate: a path that starts with `-` goes to
      the program as `./<path>`; used for the root file by both engines (the draft wrapper and the
      shell-escape-on path are already absolute or start with `.`). Three tests, each red first:
      one per engine's argument list, and an ignored one against the real `latexmk` that fails when
      the helper is switched off (mutation-checked) and passes with it. Walked in the real app on the
      same hostile project: the spawn line now ends `./-shell-escape` and no marker appears. The
      project model is left as it was (a root named `-x` still *builds*, and fails as a document):
      the boundary is where a name becomes an argument, which is where the fix is.
  11. **S (low). `git lfs track` gets project file names bare** (`lfs.rs`: `["lfs", "track", paths…]`),
      so a large file whose name starts with a dash is read as an option (`--dry-run` would make the
      call do nothing, `--lockable` changes the attributes written). Not reproduced (no `git-lfs` on
      this machine); a `--` before the paths, if `git lfs track` accepts it, or `./` as above.
      **Fixed 11 Nov 2026, not verified against the real program.** `track_with` passes `--` before
      the paths (git-lfs is a cobra program, where `--` ends the options). The test stand-in now
      treats a leading dash as an option until `--`, like the real command, and a test tracks a file
      named `--dry-run` (red first: nothing was written). `git-lfs` is still not installed here, so
      the `--` is the one thing worth running once on a machine that has it.
  12. **S (low). The build time limit is per engine process, not per build.** A warm build runs up to
      three single passes and then a full build, each with its own 15 minutes, so the longest build
      the limit allows is an hour, not 15 minutes as the constant's comment says.
      **Fixed 11 Nov 2026.** `Tectonic::build` fixes one deadline (`started + build_limit`) and every
      pass is run with what is left of it; the error names the build's limit. `with_build_limit` lets
      a test use a small one. One test (five 0.4 s runs against a 1 s limit: before, the build ran
      1.65 s and succeeded; now it times out), red first.
  13. **Test flake, seen once. `tectonic::tests::a_cancelled_cold_build_still_leaves_the_next_one_full`
      failed with `ExecutableFileBusy` ("Text file busy")** in a full `cargo test -p
      abstract-tex-engine --lib`, passed on three reruns. The test writes a script and runs it while
      other tests in the same process fork: a child that inherits the write handle keeps the file
      busy. Fix when touched: retry the spawn on that error in the test helper, or write the script
      before any test thread starts.
      **Fixed 11 Nov 2026.** Reproduced first outside the repo: with eight threads spawning, 30 of 400
      freshly written scripts failed their first run with `ETXTBSY`. `write_runnable_script` (in the
      tectonic tests) now runs the script once with a probe argument it answers by exiting, retrying
      on error 26 until it can be run; a stress test (150 scripts under eight spawning threads) failed
      on script 0 before, and the engine's tests passed three runs in a row after.
  14. **R. A one-click fix can edit the wrong place, and leave the error where it was.** `locateFix`
      (`src/lib/fix.ts`) replaces the *first* occurrence of the fix's `find` on the diagnosed line, and
      several catalog fixes carry a `find` that is a bare symbol or command, not anchored to where TeX
      stopped. Reproduced with a real `pdflatex` log for `As shown in Section~\ref{sec_intro}, the
      value a_n grows.`: the rule is right (`missing-dollar`, "`_` used outside maths") and the fix is
      `find "_"` → `\_`, so the button rewrites the `_` inside `\ref{sec_intro}` to `\ref{sec\_intro}`
      — a new undefined-reference warning — and `a_n` is still broken. The same shape in the rest of
      the catalog, by reading: `fix_misplaced_alignment_tab` (`find "&"`; on `Tom \& Jerry & Co` the
      first `&` is the escaped one, giving `\\&`), `fix_illegal_unit_of_measure` (`find "{1}"` meets an
      earlier `{1}` first), `fix_include_cannot_be_nested` (`\include` is also the start of
      `\includegraphics`), `fix_duplicate_documentclass`. The same blindness in the *explanation*:
      `detect_math_symbol` looks at the whole context (`contains('_')`) rather than at the character
      TeX stopped on, so a line holding `\ref{sec_intro}` and a `^` error is explained as an
      underscore. DESIGN §5.2's rule is "a fix may only be offered when it cannot be wrong"; as built it
      can be. TeX's context ends exactly at the culprit, so the fix should anchor on the context's
      tail (a few characters ending at the symbol) and `locateFix` should decline when the anchor is
      not found or not unique.
      **Fixed 11 Nov 2026.** `anchor_tail` in `texlog`: the last 24 characters of the context, which
      ends where TeX stopped. `fix_missing_dollar`, `fix_misplaced_alignment_tab`,
      `fix_illegal_unit_of_measure`, `fix_fragile_command_in_moving_argument` and
      `fix_duplicate_documentclass` now find and replace *that* text (`e value a_` → `e value a\_`);
      `fix_include_cannot_be_nested` finds `\include{`, which `\includegraphics{` cannot match. The
      symbol the missing-dollar explanation names is the last character of the context, not any `_`
      in it; with none it names no symbol and offers no fix. `locateFix` also declines when the
      `find` appears twice on the line, the contract `texlog` always claimed. Tests, red first: six
      in `rules.rs` (the reproduction among them), two in `fix.test.ts`; the six fixtures'
      expected `fix` entries were rewritten (nothing else in them changed), and the older unit
      tests that pinned the bare symbols now pin the anchors.
  15. **S (low). Wording that depends on the engine does not know it.** `explain_file_not_found` says
      the engine "downloads packages on demand … fetched the next time you build with a network
      connection" (true for the bundled Tectonic, false for a system `latexmk`, which needs the
      package installed), and sends a missing `.cls` or `.bst` through the "document includes" branch
      ("resolved relative to the folder holding the main file").
      **Fixed 11 Nov 2026.** A missing package now says what happens with each kind of engine (the
      bundled one downloads on demand; a TeX on the computer needs `tlmgr install` and the name), and
      a missing `.cls` has its own wording (check `\documentclass`, and a supplied class belongs next
      to the main file) instead of "the document includes". Two tests, red first; the one fixture
      that pinned the old sentence was rewritten.
  16. **R. Entries made by paste-to-cite break the build when a field holds `&`, `%`, `#` or `_`, and
      unbalanced braces are not checked.** `render_entry` writes every value as `{…}` verbatim, and
      the acquirers copy their sources' text unchanged (arXiv: XML entities decoded, so `Q&amp;A`
      becomes a bare `&`; Open Library: `publisher` as catalogued, and "John Wiley & Sons" is among the
      commonest). BibTeX passes field text to the `.bbl` as TeX. Reproduced with real tools:
      `render_entry` of `title = {Learning from Q&A: a 50% faster approach to x_i models}`, then
      `pdflatex`, `bibtex`, `pdflatex` on a document citing it, fails with `! Misplaced alignment tab
      character &` (and a bare `%` would swallow the rest of the line, a bare `_` is "Missing $
      inserted"). The acquirers' own tests use titles with none of these. Also seen: `plain` lowercases
      the title ("q&a"), because nothing protects a capital, which Better BibTeX does. The `by_statement`
      fallback puts "Thomas H. Cormen ... [et al.]." in `author`, which BibTeX reads as one name.
      Fix: escape per source (Open Library is plain text: escape `& % # _ $ { } ~ ^ \`; arXiv is TeX:
      escape only an unescaped `& % #` outside `$…$` and balance braces), at acquisition, with tests
      that build the result with real BibTeX.
      **Fixed 11 Nov 2026.** New `texbib::tex_text`: `escape_plain_text` (Open Library: every TeX
      special escaped; `{`, `}`, `\`, `~`, `^` become `\textbraceleft{}` and friends), `repair_tex_text`
      (arXiv: valid TeX such as `$\Lambda$CDM` and `Q\&A` is left alone, a bare `& % #` or an `_`
      outside maths is escaped, an odd number of `$` is read as text, and the function is idempotent),
      and `balance_braces`, which `render_entry` applies to every value as the floor. Found while
      writing it: BibTeX counts every brace and ignores a backslash before it, so `\{` does *not*
      protect a lone brace; unpaired ones are replaced by the `\textbrace…{}` commands, not escaped.
      The titles, authors, publishers and abstracts are escaped; the generated key is made from the
      text as it came. Tests, red first: nine for the helpers, one each for arXiv, Open Library and
      the renderer, and an ignored one that builds the result with real `pdflatex` and `bibtex`
      (mutation-checked: with the escaping off it stops with "Misplaced alignment tab character &").
      Not done: protecting capitals (`plain` still prints "q&a" for "Q&A") and the `by_statement`
      author fallback.
  17. **Checked against real BibTeX (nine files, a field-dump `.bst`, the same input read by
      `texbib::parse`): no parser defect.** Strings, `#` concatenation, month macros, parenthesis
      delimiters, stray text, whitespace and newlines inside values, nested braces, odd keys and a
      missing last comma all read the same. Differences, none a defect in the parser: (a) `@comment{…
      @article{ghost,…} }` — BibTeX reads `ghost` as an entry, texbib treats the comment as one unit
      (as Biber does); (b) a malformed entry (`\"` inside a quoted value, an unbalanced `\{`) is
      dropped whole with a `ParseError`, where BibTeX salvages the key and the fields before the
      error; (c) a field set to an undefined macro is present-but-empty for texbib and missing for
      BibTeX; (d) two keys that differ only in case are both listed, where BibTeX skips the second as
      a repeated entry (the E3 `.blg` rule reports it after a build; there is no health check for it).
  18. **S (low). "Undefined citation" is an error when the key differs from the entry's only in case,
      although BibTeX builds it.** `BibliographyIndex::entry` compares exactly ("the comparison that
      is never wrong about an existing entry"), which is true for Biber and false for BibTeX, which
      resolves `\cite{Smith2020}` to `@article{smith2020`. The app does not know which backend the
      document uses.
      **Fixed 11 Nov 2026.** A citation that matches an entry only ignoring case is now one warning,
      `citation-case` ("'Smith2020' is cited, but the entry is 'smith2020': they differ only in
      capitals. BibTeX accepts that; Biber does not."), not the error `undefined-citation`; and the
      never-cited check compares ignoring case, so the entry is not also reported as uncited. Two
      tests, red first.
  19. **S (low). Generated keys drop non-ASCII letters instead of folding them, and can come out
      empty.** `ascii_fold_lower` keeps ASCII only (`Gonçalves` → `gonalves`, `João` → `jo`, a
      documented simplification); a record whose author, year and first title word are all non-Latin
      (an Open Library entry with no `publish_date`) gives the key `""`, and `unique_key` does not
      treat an empty base as a collision, so `@book{,` is written. Fix: fold the common Latin
      accents, and fall back to a fixed word (`ref`) when the base is empty.
      **Fixed 11 Nov 2026 (the accent half); the empty-key half was a mistake in this entry.**
      `ascii_fold_lower` (now shared by the arXiv and Open Library keys) folds the accented Latin
      letters of Latin-1 and Latin Extended-A, so `Gonçalves` is `goncalves`; one test, mutation-
      checked. An empty key cannot come out: `unique_key` recomputes from the fields when the key is
      empty and falls back to `unknown`.
  20. **R. A failed *Join* empties the document it was meant to join.** `joinSession` cleared the open
      document (so the host's copy can arrive as ordinary remote updates) *before* it connected, and
      its `catch` only set the error sentence. With a relay that cannot be reached, a mistyped
      address, or a bad key, the tab ends up empty, and autosave writes the empty file to disk.
      Reproduced with a controller test (`connect` rejecting; the document read `''` instead of
      `'hello'`). Also found: `connect` left its socket open when the key import failed while the
      socket opened. **Fixed 11 Nov 2026.** `LiveSession.connect` takes a `beforeSync` callback that
      runs once the socket is open and the key is in hand, before the first message; `joinSession`
      clears the document there; a failed connection closes its socket. Tests, red first: the
      controller test above (and its twin, that a successful join still empties the document only at
      that point), and two in `live-session.test.ts`. **Still open (a decision):** the Join window
      does not say that the tab's text is replaced by the host's copy, and a join that connects but
      finds nobody in the room (the host left, a wrong invite on a working relay) also leaves the tab
      empty and autosaved empty. Either a confirmation naming the file, or a copy kept beside it.
      **The open half done 11 Nov 2026, as a recommendation the maintainer can overrule:** before a
      join, `keepCopyBeforeJoin` writes what the tab holds to
      `.abstract-tex/join-backups/<file>.<timestamp>` (invisible in the tree, ignored by Git), the
      Join window says so, the notice after a successful join gives the path, and a copy that cannot
      be written stops the join with nothing changed. A join into an empty room still leaves the tab
      empty, but the author's text is no longer only in the buffer. Two tests, red first.
  21. **S, confirmed in the library. A peer's presence is drawn without being checked.**
      y-codemirror.next writes a peer's `color` into `style="background-color: ${color}; border-color:
      ${color}"` (and `colorLight` into the selection's `style`), and `RelayAwareness` applied the
      peer's awareness update as it came. Anyone holding the invite is a peer, so `color: "red;
      position: fixed; inset: 0; …"` turns the remote caret into an overlay over the whole window
      (a `data:` image is allowed by the page's CSP, so it can show a picture). Reproduced across the
      encrypted channel in a test: the receiving side's awareness state held `red; inset: 0`.
      **Fixed 11 Nov 2026.** `sanitisePeerUpdate` decodes the update, keeps each `user` to a clean
      name (control characters removed, 40 characters, `Anonymous` when empty) and two hex colours
      (the default pair otherwise), skips a state that is not JSON, passes a `null` (a departure)
      through, and applies nothing from an unreadable update. Six tests, red first; the colour check
      is mutation-checked (a pattern that accepts everything fails two of them).
  22. **S (low). The relay client has no connection timeout.** `waitForOpen` waits for the socket's
      own `open` or `error`; an address that accepts nothing and answers nothing leaves *Share* and
      *Join* on "connecting…" for as long as the webview's own timeout, with no way to cancel. The
      server side has a handshake timeout since the second review pass.
      **Fixed 11 Nov 2026.** `waitForOpen` gives up after `CONNECT_TIMEOUT_MS` (10 s), closes the
      socket so it cannot open later unseen, and rejects with "The relay did not answer. Check the
      address, and that the relay is running." The reconnect path shares it. One test with fake timers
      and a socket that never answers, red first.
  23. **R. Two folder links back to the project's own folder hang the app for good.** Opening a
      project builds the file tree on the main thread, and `list_tree` recursed into every folder
      link (`is_dir()` follows links); the kernel stops a path after 40 links, so one link back to
      its own folder gives an 82-node tree and two give about 2^40 nodes. Measured with a probe: no
      link 1 node in 20 µs, one link 82 nodes in 0.9 ms, two links did not finish in 150 s. A cloned
      repository can carry such links (same door as R3.12 and R5.2); a freshly opened project that
      hangs the window is the worst way to meet it. **Fixed 11 Nov 2026.** `list_tree_inner` keeps
      the real path of every folder above the one it is listing and lists, without walking, a folder
      link that leads back to one of them; a link to a folder that is not an ancestor (a `chapters`
      link to a shared folder) is walked as before, and a test says so. Tests, behind a ten-second
      deadline so a regression fails instead of hanging the run, red first. Walked in the real app on a project with two links to its own folder: it opens, lists `loopa` and `loopb` as empty folders, and builds.
  24. **S (low). The tree is built whole, eagerly, for whatever folder is opened.** `list_tree`
      reads every folder to the bottom and sends it as one JSON value, so opening a folder with a
      very large data directory (hundreds of thousands of files; not `node_modules`, which is
      skipped) costs seconds on the main thread and memory in the webview. The `.gitignore` is not
      consulted.
      **Measured 11 Nov 2026, not fixed** (a probe, since removed): 200,000 files in 700 folders take
      0.72 s to walk on this machine and make 15 MB of JSON. That is a pause, not a hang, for a folder
      far bigger than any project; it is not worth a cap and a truncation notice until a real folder
      shows otherwise.


- **Agentic review, sixth pass (15 Nov 2026): the hub, the seam and the file boundary
  (`REVIEW-HANDOFF.md` §3 P1, in the order of its §8). Findings are logged as they are found; the
  fix batch follows the list.** Method as before: read the code, reproduce with the real thing
  (a throwaway probe, then a failing test), then fix test-first. R = reproduced, S = by reading.
  Covered: `REVIEW-HANDOFF.md`; `src/lib/ipc.ts` (all 88 `invoke` calls, by script; and the seven
  `listen` wrappers); `src-tauri/src/commands.rs` (all of it); `src-tauri/src/git.rs`;
  `src-tauri/src/project.rs` (`Project::open`, `resolve`, `resolve_for_write`, `info`, the include-graph
  cache, `detect_root`, `list_tree`, `read_text`, `write_atomically`); `src/lib/controller.svelte.ts`
  lines 1-600, 780-900, 1878-2660 and 2905-3110 (the language-server glue, the assistant settings, Git
  refresh, GitHub sign-in, crash reports, `start`, `openFolder`, `openFile`, `closeTab`, `triggerCompile`,
  the comments flows, `handleCompileEvent`, `handleFsEvent`, `reconcileOpenDocument`);
  `src/lib/documents.ts`; `src/lib/document.ts` lines 84-310; `crates/abstract-tex-git/src/lib.rs`
  (`log`, `export_tree`, `write_gitignore`, `inside_project`, `stage`, `unstage`, `discard`, `amend`).
  Continued the same day: the rest of `src/lib/controller.svelte.ts` (the assistant review flow, the
  diff view, snapshots, clone, live sessions, templates, comparison, commit/sync/branch/tag verbs,
  placeholders, SyncTeX, `resolveMergeConflict`); `src/lib/conflict.ts`; `src/lib/git.svelte.ts` lines
  40-384; `src/components/SourceControl.svelte` (script and markup, not the style block);
  `src/components/ConflictResolver.svelte` (script and markup); `src/components/Tabs.svelte`;
  `src/components/Editor.svelte` lines 140-170; `src/lib/editor/paste.ts`, `definition.ts`, `cite.ts`;
  `src/lib/lsp.ts` lines 70-165 (`didChange`). Then, the same evening: the other editor extensions
  (`setup.ts`, `diagnostics.ts`, `completion.ts`, `hover.ts`, `positions.ts`, `placeholders.ts`,
  `math-preview.ts`, `focus.ts`, `typewriter.ts`, `synctex.ts`, `diff.ts`, `selection.ts`) and all of
  `Editor.svelte`; one finding (item 21), in the one that reads the layout. **Not read, so silence there
  is not a clean bill:** `project.rs`'s tests; the style blocks; the other Svelte components; `symbols.ts`;
  the state modules other than `git.svelte.ts`; the SyncTeX crate beyond timing `SyncTex::open`.

  **Clean, with the evidence, so nobody re-does it.** The Rust/TypeScript seam agrees everywhere it
  was checked. A script (scratchpad, not kept) paired every `invoke` in `ipc.ts` with its
  `#[tauri::command]`: 88 names each way; the argument names (camelCase on this side, snake_case in
  Rust) agree for all 88; every argument that the TypeScript may send as `null` or leave out is an
  `Option` in Rust (`compile`, `assistant_save_provider`, `github_create_repository`, `git_clone`,
  `compare_revisions`, `save_comparison_pdf`); every return type agrees, `Option<T>` as `T | null` and
  `()` as `void`; 34 serialised Rust structs match their TypeScript interfaces field for field
  (`rename_all`, `rename` and `skip_serializing_if` taken into account; the one report, `Finding`, was
  two Rust types of one name, and the one the command returns matches); the tagged enums carry the
  tags and variant names the unions expect (`SyncOutcome`, `CompileEvent`, `ComparisonStarted`,
  `SignInEvent`, `Jump`, `BibOrigin`); and the seven event names `ipc.ts` listens for are the ones
  Rust emits (`compile`, `compile-diff`, `fs:changed`, `lsp`, `git:status-changed`, `github:sign-in`,
  `bibliography:changed`). The hand-off's open question about argument names and return types is
  answered: no mismatch.

  1. **R (high). A save writes through a link standing at its temporary file name.**
     `project::write_atomically` writes `<name>.abstract-tex-tmp` with `fs::write` and renames it over
     `<name>`. `fs::write` opens with create-or-truncate, which follows a link. A cloned repository can
     carry `main.tex.abstract-tex-tmp` as a link to any file the author can write (the tree hides that
     name as junk, so nothing in the window shows it), and the next autosave of `main.tex` writes the
     document into the link's target; the rename then makes `main.tex` itself the link. The document
     text is largely the repository author's own. Same door as R3.12, R5.2 and R5.23.
     Reproduced with a probe against the real function: the "victim" file outside the project held the
     document text afterwards and `main.tex` was a symlink. Used by `write_file`, `create_file`, the
     bibliography append, snapshot restore, `abstract-tex.toml`, and the two settings files.
     **Fixed 15 Nov 2026.** The temporary name is removed first (`remove_file` removes a link, never
     its target; a half-written leftover from a crash goes the same way) and the new file is made with
     `create_new`, which refuses anything that exists, a dangling link included; a failed write removes
     its temporary file. Three tests, red first (a link to a file, a dangling link, a leftover file);
     mutation-checked (the `remove_file` step taken out: all three go red).
  2. **R (high). Opening another folder threw away every unsaved edit.** `openFolder` called
     `ipc.openProject` (Rust switches projects at once) and then `closeAllDocuments`, which disposes
     each `OpenDocument`; `dispose` clears the save timer and destroys the `Y.Doc`, writing nothing.
     So the text was lost in three situations: a save that was failing (disk full, permission) with
     the tab still dirty; **a conflict bar still on screen**, where saves are held on purpose and the
     buffer holds edits that exist nowhere else (`openFile` refuses to even switch tabs in that state,
     "Answer the question first", but `openFolder` did not ask); and an edit typed less than 700 ms
     before an `openFolder(path)` that does not wait for a picker (Clone, New project from a template,
     the command line). The folder-picker case is mostly safe because the debounce fires while the
     dialog is up. Reproduced with three controller tests: the buffer read `hello`, not `hello and more`;
     `app.conflict` was `null` afterwards; the disk still held `hello`.
     **Fixed 15 Nov 2026.** `flushBeforeLeavingProject` runs before `ipc.openProject`: it refuses while
     a conflict is open (same sentence as `openFile`), writes every tab with `manager.saveAll()`, and if
     any tab is still dirty (a failed or held save) it says which file and that the folder was not
     changed, and does not switch. Four tests (the three above plus the failing one), red first; the
     flush taken out turns three red. **Cost to the test file:** `controller.test.ts` used
     `openFolder('/proj')` in its `beforeEach` as a reset, which relied on this data loss to forget
     what the previous test had typed; the `beforeEach` now writes leftovers to the old fake disk
     first, and one test ("clears when the project closes") saved its heading and opens a project
     without one, so it still tests the outline resetting rather than the loss.
  3. **R (medium). Asking for the same file twice before the first read returned made a second
     `Y.Doc`, and a late read landed in the next project.** `openFile` checked `isOpen`, awaited
     `ipc.readFile`, then called `manager.open`; `DocumentManager.open` says in its own comment that it
     must never be called for an open path ("silently orphan the live one"). Two overlapping calls
     (a double click on a slow disk, Quick Open pressed twice, a jump to a diagnostic racing a click)
     both passed the check: `docs.set` replaced the first document without disposing it and the path
     went into the tab row twice, which a keyed `{#each}` in `Tabs.svelte` takes as a duplicate key.
     The same gap let a read started in project A finish after `openFolder` had opened B and open A's
     file text as a tab of B. Reproduced with two controller tests (`['chapter.tex', 'chapter.tex']`;
     the stale file open in the new project).
     **Fixed 15 Nov 2026.** One read per path in flight (`opensInFlight`): a second request waits for
     the first and ends; and `projectEpoch`, bumped by `openFolder`, is checked after the read so a
     late answer for the old project is dropped. Two tests, red first; each half mutation-checked.
  4. **R (low). Closing a tab whose text cannot be saved did nothing, and said nothing.** `closeTab`
     awaited `save()` with no `catch`; `Tabs.svelte` calls it with `void`, so a failing save was an
     unhandled rejection, the tab stayed and the author saw a dead button. `save()` also returns
     `false` without writing when saves are held (a reconcile in flight), and the tab was then closed
     with its text. Reproduced: `closeTab` rejected with the write error.
     **Fixed 15 Nov 2026.** A failed save keeps the tab and says why in the notice; a tab still dirty
     after the save is kept too. One test, red first, mutation-checked.
  5. **Measured, not fixed. 60 of the 88 commands are synchronous, so they run on the main thread, and
     the save path does real work in several of them.** After the 700 ms idle autosave: `write_file`
     (sync) writes, then, for any `.tex` or `.bib`, `emit_bibliography` rebuilds the whole index
     (include graph, every `.tex` read twice, every `.bib` parsed) and sends it as one event; the
     `triggerCompile` that follows calls `compile` (sync), whose `root_file()` falls back to
     `detect_root` (a three-level walk that reads every `.tex` it finds) whenever the root is not
     `main.tex` and none is configured, and `emit_bibliography` calls it again; and the
     `git:status-changed` the write announces makes the frontend ask six synchronous Git questions in a
     row (a seventh, the history, only when `HEAD` moved), each opening the repository again. `read_log`, the SyncTeX commands and `compare_revisions`
     call `root_file()` too, the SyncTeX ones also parsing the whole `.synctex.gz` per click under the
     project lock. Measured with a probe (a thesis-shaped project, root `thesis.tex`, 800 `.bib`
     entries, ~7 KB per chapter): **release build** 15 chapters `root_file()` 0.8 ms, `build_index`
     5.0 ms, `info()` 1.2 ms; 60 chapters 3.1, 12.5 and 4.1 ms; 200 chapters (1.4 MB of `.tex`) 10.1,
     36.3 and 12.8 ms. In an **unoptimised** build, which is what `pnpm tauri dev` runs, the same code
     is about ten times slower (a project of 100 chapters at 27 KB each: 89 ms, 409 ms, 107 ms). So on a
     release build a typical document costs a few milliseconds once per pause, a long book 50 ms, and
     the dev build can stall for a few hundred ms after each pause. The SyncTeX parse, timed on a real
     3.2 MB table from a 300-page document: 13 ms in a release build, 113 ms in a debug build, once per
     request, and requests are explicit (Ctrl-Alt-J, a double click in the PDF; the comment in
     `syncTexForward` saying it "fires on ordinary cursor movement" is stale). Not measured: the Git
     questions on a large repository. Options if it is ever felt:
     `spawn_blocking` for `emit_bibliography`, a cached root file (`Project` already caches its graph by
     file stamps), and one `git_overview` command in place of six.
  6. **S (low). A build of the previous project that finishes after `openFolder` has reset the window
     lands on the new project.** `handleCompileEvent` accepts a `finished` when its generation is not
     older than the current one; `openFolder` resets `app.compile` but keeps the generation, and
     `open_project` does not cancel the running build ("a running build of the previous project
     finishes on the engine it started with"). Between the reset and the new project's own `started`
     (the language server's start-up sits in that gap) the old project's PDF address and diagnostics
     can be shown over the new project's tabs. Needs a build still running when a folder is opened (a
     cold package fetch takes long enough). Not reproduced. Fix: cancel in `open_project`, or compare a
     project epoch in the handler as `openFile` now does. `refreshOutline` has the same shape (it
     guards on the path only; two projects both have `main.tex`).
  7. **S (low, unverified). `export_tree` joins the names in a commit's tree to the destination.**
     `dest_dir.join(format!("{root}{name}"))` with no check on a name of `..`. Git itself and GitHub's
     receive-pack refuse such a tree, so a commit reaching `latexdiff`'s comparison from one of them
     cannot carry it; whether libgit2's fetch from another server does is not known. Not reproduced.
     A one-line check that every component is a normal name would close it.
  8. **S (low, Windows). `clone_destination` refuses the characters Windows refuses in a name, but not
     the reserved device names (`con`, `nul`, `aux`, `com1`) or a trailing dot or space.** A repository
     named `con` would fail inside libgit2 with its own sentence. Cannot be checked on this machine.

  9. **R (low; a consequence of the fix to item 2). Making a project from a template, or cloning one, did
     the work and only then found it could not leave the open project.** `createProject` and
     `cloneRepository` call `openFolder` last; once item 2 made `openFolder` refuse while a tab holds text
     that cannot be saved, the author ended up with a new folder on disk (or a finished download) and a
     notice saying the folder "was not changed". Reproduced with two controller tests (the template was
     made; the clone was downloaded). **Fixed 15 Nov 2026.** The check is `whyNotToLeaveProject`, which
     answers the sentence or `null`; both windows ask it before anything is made and show the sentence
     in the window, and `openFolder` shows it as a notice. Both mutation-checked.
  10. **R (high for anyone with `merge.conflictStyle = diff3` or `zdiff3`). The conflict view put the
     common ancestor, and Git's `|||||||` line, into "Yours".** `parseConflictMarkers` ended the first
     side at the first `=======`, so the base section of a diff3 hunk was part of `ours`, and one click
     on *Keep mine* wrote the marker line and the original sentence into the manuscript. The app meets
     such files whenever a conflict was made by a `git` in a terminal (the watcher shows them as
     conflicted). It also took any line that merely *started* with `=======` (a heading underline of
     nine, a longer rule) as the separator. Reproduced with two tests (the first side held
     `My sentence.\n||||||| merged common ancestors\nThe original sentence.`). **Fixed 15 Nov 2026.**
     Markers are matched as Git writes them (seven of the character, then the end of the line or a space;
     `=======` alone), a trailing `\r` is ignored for the match, and the base section is skipped. Three
     tests (diff3, the longer rule, Windows line endings), the first two mutation-checked.
  11. **R (high). A conflicted file could be staged with its markers in it, and committed.** The
     *Merge Changes* rows reused the ordinary row, so each had a Stage (+) and a Discard (↺) button.
     `stage` is `index.add_path`, which replaces the conflict entries with the working file; with
     `<<<<<<<` still in it the path stopped being conflicted, `commit` was no longer refused, and the
     commit held the markers (reproduced against a real conflict: the committed `main.tex` was the
     marker text). Discard on a conflicted path ran `checkout_index`, answered `Restored`, and in fact
     wrote fresh `ours`/`theirs` markers over whatever the file held, so the panel said "back to its last
     committed version" about a file still in conflict. **Fixed 15 Nov 2026.** `stage` refuses a
     *conflicted* path whose file still has a marker line (`StillConflicted`; a file that talks about
     markers but is not conflicted is allowed, and so is a conflicted one the author has cleaned up,
     which is how the resolver finishes); `discard` refuses a conflicted path (`ConflictedPath`); the
     rows no longer offer either button. Four crate tests, red first (the first two against the real
     behaviour printed above), the two refusals mutation-checked.
  12. **R (low). Discard told the wrong version.** A tracked file is restored from the *index*, so a file
     that is also staged goes back to the staged version and keeps the staged work; the confirmation
     said "the last committed version… changes since then are lost" and the report afterwards said the
     file "is back to its last committed version". Safe in effect, untrue in both sentences. **Fixed 15
     Nov 2026.** The question and the report say "the version you staged" when the path is also in the
     staged list. One test, red first.
  13. **R (medium). Amend in the middle of a merge made a one-parent commit and left the merge hanging.**
     After a conflicted Sync the local commit is unpushed (so *Amend* is offered) and `MERGE_HEAD` is
     set; `amend` keeps `HEAD`'s own parents, so once the conflict was resolved and staged it wrote a
     commit holding the coauthor's text with a single parent, and `MERGE_HEAD` stayed. Reproduced: the
     amended commit had `parents=1`, the state was still `Merge`, its tree held the resolved text.
     **Fixed 15 Nov 2026.** `amend` refuses while a merge is waiting (`MergeInProgress`: commit it
     first). One test, red first.
  14. **R (medium). A failed write in *Mark Resolved* was an unhandled rejection.** `resolveMergeConflict`
     had no `catch` and the component no `finally`, so with a write error the button stayed on
     "Resolving…" for good and nothing said why. And when the write worked but Git then refused to stage
     (item 11's new refusal, or any other), the sentence went to the Source Control panel while the
     author was looking at the file. **Fixed 15 Nov 2026.** A failed write is a notice and changes
     nothing; a refused staging is a notice from the same place; the component's button returns in a
     `finally`. Two tests, red first.
  15. **S (latent). `ConflictResolver.load` did not check the path after its read.** `sections` and
     `resolutions` are written when the read returns, but *Mark Resolved* writes them to
     `app.activePath`. If two loads were ever answered out of order (every command is synchronous and
     answered in order today, so not now; `read_file` is the first candidate for item 5's
     `spawn_blocking`), one conflicted file's text would be saved over the other's. **Fixed 15 Nov
     2026** with the guard (`loadedPath !== path`); no test, because the repository has no component
     test harness.
  16. **R (medium). Ctrl Enter in the commit box committed during a sync.** The Commit button is
     disabled while a sync runs ("the two should not race"), but the key calls `commitStaged`, which
     checked only `committing`. Reproduced with a controller test (a commit verb was sent with
     `syncing` set). **Fixed 15 Nov 2026** in `commitStaged`, `amendCommit` and `commitThen`; one test,
     red first.
  17. **S (medium; a design decision). Hand-typed resolutions are lost when the tab changes.** The
     resolver's boxes live in component state, and `Editor.svelte` shows it only `{#if activeIsConflicted}`
     for the active tab, so looking at another file for a moment unmounts it, and coming back re-reads
     the file and resets every box to "Yours". Not reproduced in the running app. Keeping the state per
     path in a module (as `git.message` is kept) is the fix and is a change to the view, not a patch.
  18. **S (low; a decision). A forward search that finds nothing is silent.** `syncTexForward` swallows
     every failure on the grounds that it "fires on ordinary cursor movement"; it fires on Ctrl-Alt-J. A
     person who pressed the key and got "No SyncTeX data yet. Build the project first." from Rust sees
     nothing happen. Either the comment or the policy is wrong; the sentence is already written.

  19. **R (high). Paste-to-cite put the cite on other text if the author typed meanwhile, destroying it.**
     `pasteCite` (S7.6) fetches the entry over the network, so the cite is inserted seconds after the
     paste, at the `{ from, to }` captured when the paste happened. Its comment said "if the document was
     edited there in the meantime CodeMirror simply maps the position through those changes itself". It
     does not: positions in a `dispatch` are in the document as it is now, and a position a caller holds
     is mapped by nobody. Reproduced with the real CodeMirror state: with `see 10.1000/x and more` and
     the DOI selected, three letters typed at the start while the answer was out gave
     `ABCs\cite{knuth1984}00/x and more`: the four characters `ee 1` of the author's sentence were
     replaced. A paste over a selection replaces that selection, so the loss can be as long as it.
     **Fixed 15 Nov 2026.** `trackPaste` keeps the place in the editor's own state (a `StateField` that
     `setup.ts` installs with the paste handler), where every transaction maps it: a cursor ends up
     after text typed at its own spot, a selection is not widened by text typed at its edges, a deleted
     range collapses to a point; `pasteCite` reads the place back when the answer arrives and releases
     it either way. Eight tests, red first (seven on the unit, one through `pasteCite`); mutation-checked
     twice (the stale range back: the corruption above; the edge association changed: the selection
     test). The comment is corrected. Not walked in the app: the field is tested on real CodeMirror
     state in Node, not in a window with the clipboard.
  20. **Measured, fine. The cite completion slices the whole document up to the cursor on every request.**
     `citeSource` and `citeThenLsp` start with `state.sliceDoc(0, pos)` and then read only the current
     line. On a 1.03 MB document with the cursor near the end: 0.71 ms per request, against 0.003 ms for
     the current line alone (Node, real CodeMirror). About 4% of the 16 ms budget; not worth a change on
     its own, but a one-line fix if it is ever near one. `lspCompletion` also sends the whole text with
     `didChange` before each request (skipped when it equals what the server has); not measured.

  21. **R (medium). Typewriter mode throws on the first cursor move and switches itself off.**
     `typewriterModePlugin` (S4.5) calls `view.coordsAtPos` from its `update`, and CodeMirror forbids
     reading layout while an update is running (`readMeasured` throws "Reading the editor layout isn't
     allowed during an update"). CodeMirror catches the throw, logs it, and deactivates the plugin for
     that view, so the author sees nothing: the line is simply never centred. Reproduced in the real
     app on the virtual display (WebKitGTK, CodeMirror view 6.43.11): thesis fixture with a 200-line
     `results.tex`, *Toggle typewriter mode* from the palette, three Up keys; the cursor stayed at the
     bottom of the screen (line 40 of 46 visible) and the dev log held `CodeMirror plugin crashed:
     readMeasured … coordsAtPos … centerCursor@typewriter.ts … update@typewriter.ts`. The ledger's
     10 Nov entry says the mode was re-walked and "centres its line"; I cannot reconcile that with this
     run. The unit tests cover only `centeredScrollTarget`, the arithmetic, never the call that reads
     the layout. **Fixed 15 Nov 2026.** `centerCursor` now files a `requestMeasure` (one key, so several
     updates in a frame are measured once): the read of the cursor's coordinates runs in CodeMirror's
     measure phase, where layout is allowed, and the scroll is written there too, before CodeMirror's own
     scroll-into-view checks and finds the cursor already in view. Four tests, red first, against a fake
     view that refuses a layout read during an update the way `readMeasured` does (the red run failed with
     CodeMirror's own message); mutation-checked twice (a key per call: the coalescing test; the scroll set
     to the cursor's raw offset: the centring test). Walked in the app after the fix, same document and
     keys: the cursor line sits at the vertical centre of the editor and the log holds no new crash. The
     fake enforces one rule of the real view and nothing else; the walk is what shows the real one agrees.

  22. **R (low). A clock seeded once is wrong until its first tick: "Compiling… -16.8s", and every
     snapshot "just now".** The same pattern in four components: `let now = $state(Date.now())` is set
     when the component is created, and an effect later starts an interval that advances it, without
     setting it when it starts. (a) `StatusBar.svelte`: `now` stops moving when a build ends, so the
     next build's `startedAt` is newer than `now` and the clock shows a negative time for the first
     250 ms. Reproduced in the app on the virtual display by photographing the status bar every 120 ms
     after an edit, 16 s after the previous build: two frames read "Compiling… -16.8s", then 0.3 s, 0.3 s,
     0.5 s. The size of the minus sign is how long the app had been idle. (b) `SnapshotViewer.svelte`
     has `const now = Date.now()`, evaluated once when the app starts, because the component is always
     mounted: `relativeTime` clamps a negative age to nothing, so every snapshot taken since the app
     started is "just now · N words" in the viewer's header however old it is. Reproduced: the same
     snapshot is "7 minutes ago · 3,368 words" in the Snapshots list and "just now · 3,368 words" in the
     viewer opened from it. (c) `CloneWindow.svelte`: the sign-in countdown's `now` is from app start
     until the first tick a second after the code appears, so `timeLeft` reads "N min left" with N the
     minutes since launch plus the code's lifetime, for a second. By reading, not reproduced (it needs
     a GitHub sign-in). (d) `SourceControl.svelte`: the effect that ticks the relative times does not
     refresh `now` when it restarts (the sign-in stage changes its interval), so labels can be up to a
     minute stale; harmless because `relativeTime` clamps, listed so the four are fixed alike.
     **Fixed 15 Nov 2026.** Each effect now sets `now` as it starts (`StatusBar`, `CloneWindow`,
     `SourceControl`; none of them reads `now`, so writing it cannot re-run the effect), and the viewer's
     title is a `$derived` that reads the clock when a snapshot is selected. No unit test: these are
     component effects and the repository has no component harness, and the arithmetic they feed
     (`relativeTime`, `timeLeft`) was never wrong. Walked in the app after the fix, same steps: after 25 s
     idle the first frames read "Compiling… 0.0s", and the viewer's header for the 8-minute-old
     snapshot reads "8 minutes ago · 3,368 words", the list's own words. (c) was not reproduced and is
     fixed on the reading alone.

  23. **R (medium). A failed save made by anything but the autosave timer cancels the retry, and the
     notice's promise ("will be saved as soon as it can be") is then false.** `OpenDocument.save` clears
     the pending timer before it writes, and when the write throws only `autosave` re-armed one, so a
     save started by something else that failed (a build's save-all, a tab closing, the New project
     button's check) took the retry with it. The buffer stays dirty with no timer until the next keystroke
     in that file. Reproduced in the app on the virtual display: the chapter folder made read-only, a
     word typed, "Could not save sections/results.tex: Permission denied (os error 13). The text is
     still in the editor and will be saved as soon as it can be." (correct), *New* then *Create* (whose
     check runs a save-all that fails: also correct, and the second window's message was right); the
     folder made writable again; two minutes later the tab still had its dot and the word was not on
     disk; Ctrl S wrote it at once. Two Vitest tests, red first (a save-all fails while a retry waits; a
     save-all fails before the 700 ms debounce has fired). **Fixed 15 Nov 2026.** `save` itself now puts
     the retry back on the timer when its write throws (unless the document was disposed, or typing has
     already set a sooner debounce); `autosave` keeps only the once-per-failure notice. Four more tests
     pin the guards (disposed during the failing write; the waiting debounce is kept), and each of the
     three parts was mutation-checked (no re-arm: three tests fail, the two new and R4.5's; ignoring
     `disposed`; overwriting a waiting debounce). Walked in the app after the fix, same steps: the
     folder made writable again, the dot cleared and the word reached disk within 8 s, no key pressed.

  Looked at and left alone: `save_comparison_pdf` copies to a path the webview sends (the Save dialog's
  answer; a compromised webview could do far more); `write_gitignore` follows a `.gitignore` that is a
  link (the author said yes to it); the sync commands hold the project lock across their disk work
  (`synctex_*`, `read_log`), and an async command that needs the project (`comments_*`, `snapshot_*`,
  `compare_revisions`) waits behind it: a delay of the same size as item 5's, not a deadlock.

  **Verification, 15 Nov 2026.** `pnpm verify` exit 0 with items 1-16 and 19 fixed: `cargo fmt --check`, 1,031
  Rust tests passed (36 ignored), clippy `-D warnings`, svelte-check 0 errors, 939 Vitest tests. Two gate
  runs failed on the way and were read, not skipped: the run with items 9-16 exited 101 on a clippy lint
  in the new `holds_conflict_markers` (`byte_char_slices`), and the run with item 19 exited 1 on three type
  errors in `paste.ts` that Vitest does not see (`ReadonlyMap` has no `set`); both fixed and the gate run
  again. Not walked in the real app: items 2-4, 9, 12, 14, 16 and 19 are flows tested with a faked `ipc`
  or a fake view over real CodeMirror state; items 10, 11 and 13 are tested against the real parser and
  the real libgit2; item 1 is tested against the real filesystem but not through a saved file in the
  running window; the removal of the two buttons from a conflicted row (item 11) and the resolver guard
  (item 15) are template changes with no test of their own.
