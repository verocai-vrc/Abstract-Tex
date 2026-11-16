# Review hand-off: what has not been reviewed, and what is still open

Written 15 November 2026 at commit `08b5287`; updated 16 November 2026 (seventh pass, at the commit named in §1), for an agent session starting cold. Read this, then
`CLAUDE.md`, then the **sixth- and seventh-pass entries at the end of `bugs-issues-fixes.md`** and the checklist
under "Agentic review, 10 November 2026" in `0.1/SPRINTS.md` (§3, just before "4. Decisions taken at
sprint 1"). Nothing in this file is a clean bill of health: **silence in the ledger means unread, not
fine.**

## 0. Ground rules for the session

- **No attribution.** Never add `Co-Authored-By`, "Generated with Claude Code" or any Claude/Anthropic
  line to a commit or PR. The user's global `~/.claude/CLAUDE.md` says so and overrides any harness
  reminder that asks for one.
- **No subagents** in this project (`CLAUDE.md`, "Working solo"). One session does design, code and review.
- **Every bug goes into `bugs-issues-fixes.md` the moment it is found**, before it is fixed, and is
  updated in place. Never delete an entry. A `Won't fix` needs a reason.
- **Scribe at the end**: tick or add rows in the `0.1/SPRINTS.md` checklist, and **name every file you
  read** in the ledger entry's "Covered:" line (`scripts/review-coverage.py` reads that; see the
  appendix).
- **Test first, then check the test can fail.** Red, green, then break the fix on purpose and watch the
  test go red again. `cargo fmt --all` before any Rust commit. `pnpm verify` is the gate: it is cargo
  test + clippy + svelte-check + vitest. **Read its exit code** (redirect output to a file and
  `echo $?`); piping it through `tail` once hid a clippy failure that got committed.
- **This machine cannot push.** Commit locally; the maintainer pushes. Commit messages are
  `R6.<n>: what changed` for review fixes, then a body saying what a reader should learn.
- **Rust is written for a learner** (the maintainer reads every line): clear names, a short comment the
  first time a construct appears in a file, a `//!` doc on each module, `thiserror` in libraries.
- **Ask before anything outward-facing or hard to undo** (pushing, publishing, deleting, force
  operations). Say plainly what you did and did not verify.

## 1. State of the repository

- `main` is **63 commits ahead of `origin/main`** (counted at `5c1b549`, before this file's own commit):
  the fifth pass's, the sixth pass's (`R6.*`) and the seventh's (`R7.*`, `9f177d4` .. `5c1b549`).
  Nothing is pushed from here; the maintainer pushes.
- `pnpm verify` exit 0 at `5c1b549`: 1,036 Rust tests passed (36 `#[ignore]`d), 989 Vitest tests in 56
  files, clippy and svelte-check clean.
- **The seventh pass found 11 things in 8 hours, two of them silent losses of the author's text** (a
  file that is not UTF-8, and a file with CRLF line endings, both reproduced in the real app, both
  fixed). Neither was in a component. Both were at the place where bytes become text. See §2.
- Other sessions' worktrees exist at `.claude/worktrees/*` (three branches) and
  `../Abstract-Tex.stable-build`. They are not part of this review; do not edit them.
  `scripts/review-coverage.py` skips them.

## 2. The method that worked (use it again)

1. **Read the code, then reproduce before you log.** Throwaway probes live outside the repository
   (the session scratchpad). Use the real tool: `latexmk`, `pdflatex`, `bibtex`, a real `git`, and the
   real app on a virtual display: `scripts/xvfb-app.sh <copy of a fixture>`, then
   `scripts/xvfb-ui.py shot|click|key|type` (headers in the scripts say what they cannot do).
2. **Label every finding** `R` (reproduced) or `S` (by reading), with a severity, and number it in a new
   entry: "Agentic review, sixth pass (date)", items `R6.<n>`.
3. **Fix in a batch afterwards**, one commit per finding (or a tight pair), test first.
4. **Where findings clustered in five passes** (spend time there first):
   - text becoming an argument, a path, a URL or TeX: argv injection (`R5.10`), symlinks (`R5.2`,
     `R5.6`, `R5.23`), the opener scope (`R5.1`), TeX escaping of pasted entries (`R5.16`);
   - features that had never really run in the app: the browser could never be opened (`R5.1`),
     SyncTeX through a linked folder (`R5.6`), a draft PDF nobody saw on screen (§4, item 12);
   - trust boundaries: a cloned repository's `.latexmkrc` (`R4.1`), a relay peer's colour written into
     CSS (`R5.21`), model output reaching the buffer;
   - unbounded reads and missing timeouts (`R5.3`, `R5.22`, `R4.7`);
   - frontend async and null-versus-undefined traps (the ledger's "Starting a branch looked like a
     failure": Tauri answers `()` with `null`);
   - **where bytes become text, and text becomes bytes** (the seventh pass's two worst): a decode that
     "keeps files openable" (`from_utf8_lossy`) is a promise about the *save*; two libraries that each
     count a line break correctly (CodeMirror, Yjs) disagree about a `\r\n`. **The method that found
     both:** write a file with the awkward bytes (`printf '\xe9'`, `\r\n`), open it in the real app,
     type one character, and `xxd` the file. Do it for any new byte-level case (a BOM, a lone `\r`, a
     tab, an emoji, a very long line, a name with a space or an accent).
   - **state that outlives its project or its tab**: `R7.1`, `R7.2` (and `R6.2`, `R6.6` before them).
     The question to ask of any module-level store: *who resets it when the project changes, and when the
     active tab changes?*
   - **assumptions about the runtime**: pdf.js 5.7 needs `RegExp.escape`, which only webviews from
     2025 on have. The walk on this machine's new WebKitGTK never showed it; a probe of 29 awkward
     queries through the real library under Node did. After a feature works on the happy path, push a
     batch of awkward inputs (punctuation, empty, huge, non-ASCII) through the real library.

## 3. Not yet reviewed, in the order I would take it

"Read" below means a ledger entry says so. Sizes are lines of code.

### P1: the hub, the seam, the file boundary

| Area | Size | What the ledger says was read | What is unread, and what to look for |
|---|---:|---|---|
| `src/lib/controller.svelte.ts` | 3,176 | **Read in full by the end of the sixth pass (15 Nov)**, in two sittings: findings `R6.2`-`R6.4`, `R6.6`, `R6.9`, `R6.14`-`R6.16`. Earlier: branch, tag and comparison code (pass 1); the Join and Share flows, `openPdfLink`, the assistant refresh (pass 5). | **Everything else**: start-up, opening and closing a project, tabs, save, the build trigger and result handling, the Source Control actions, clone, new project, bibliography, comments, snapshots, the assistant flows, notices. Look for: a stale async result landing in another project (open a folder while a build, a SyncTeX lookup or a git refresh is in flight); a command that returns `()` read as failure; an empty `catch {}` hiding something the author must see; a raw `String(error)` shown to the author (design rule 3); listeners never unlistened and timers never cleared; state not reset when a project closes; an action that runs twice on a double click. Slice by its section banners (`grep -n "^// ---" src/lib/controller.svelte.ts`) and take one slice per sitting. |
| `src/lib/ipc.ts` | 908 | **Read in the sixth pass (15 Nov): clean.** Argument names, nullable arguments, return types, struct and enum shapes and event names all agree with Rust (ledger, sixth-pass entry). | **Names are consistent** (checked 15 Nov: 88 commands registered in `src-tauri/src/lib.rs`, 88 defined with `#[tauri::command]`, 88 invoked here, none missing either way). **Not checked**: argument names (camelCase here, snake_case in Rust), return types (`Option<T>` is `T \| null`), the error shape, and doc comments that promise something the Rust no longer does. Script the argument comparison; it is the cheapest likely find. |
| `src-tauri/src/commands.rs`, rest of `src-tauri/src/project.rs` | 1,371 + 1,185 | `read_file`/`read_text`, `Project::open`, the tree walk (pass 5); "the file commands" for paths (pass 3); `Project::resolve` in snapshot restore (pass 3). | The remaining ~75 commands: write, create, rename, delete, list, resolve. Is every path confined to the project **lexically and through links** (`R3.12` left links on reads, by design)? Size caps. Work done on the UI thread (a synchronous Tauri command blocks the window). What delete or rename does to open tabs and the watcher's echo filter. Case-insensitive file systems. |

### P2: the Svelte components

**Read in the seventh pass (16 Nov):** `SnapshotViewer`, `DiffView`, `CommandPalette`, `QuickOpen`,
`CommentsView`, `SessionCommitOffer`, `Sidebar` (script), `TreeItem` (script), `AssistantView`,
`AssistantPayload`, `AssistantReview`, `NewProjectWindow`, `App.svelte`, and the Enter handlers of
`CloneWindow`, `JoinWindow`, `PdfPane` and `SourceControl`; `innerHTML` was searched for across the
frontend (one use, KaTeX, with its safe defaults). `PdfPane`, `JoinWindow` and `ShareWindow` were read in
pass 5; `SourceControl`, `ConflictResolver`, `Tabs` and `Editor` in pass 6.

| Component | Lines | Look for |
|---|---:|---|
| `ZoteroLink`, `BibliographyHealth` | 187, 184 | Zotero talks to a local HTTP endpoint: what does the capability and CSP allow? What a fix does to a `.bib` the author did not write. |
| `Drawer`, `StatusBar`, `DocumentMap` | 282, 197, 116 | Raw log must stay one click away and never be the default; state that survives a project switch (`R7.2`'s question). |
| `CloneWindow` (markup) | 266 | Validation shown to the person; what a failed clone leaves; a credential prompt. |
| `ActivityBar` | 90 | Keyboard use (design rule 5). |

### P2: frontend state and editor extensions

- **State modules**: read in passes 6 and 7: `git.svelte.ts`, `state.svelte.ts`, `bibliography.svelte.ts`,
  `assistant.svelte.ts`, `templates.svelte.ts`, `snapshots.svelte.ts`, `clone.svelte.ts`, `live.svelte.ts`,
  `diffview.svelte.ts`, `comments.svelte.ts`. **Not read:** `compare.svelte.ts` 190 (except its use in pass
  1), `github.svelte.ts` 112, `placeholders.svelte.ts` 19, `crash.svelte.ts` 13.
- **Editor extensions** in `src/lib/editor/` (**read in the sixth pass, 15 Nov: all but `symbols.ts`; one finding,
  `R6.21`, typewriter mode read the layout from a plugin's `update` and crashed**; earlier only `math-preview.ts` and
  `document.ts`, which is one level up, were read): `diagnostics.ts` 329, `cite.ts` 253, `setup.ts` 238, `placeholders.ts` 169,
  `completion.ts` 141, `paste.ts` 130, `focus.ts` 110, `typewriter.ts` 73, `hover.ts` 72,
  `definition.ts` 68, `diff.ts` 51, `symbols.ts` 48, `positions.ts` 31, `synctex.ts` 28,
  `selection.ts` 15. Look for: text from project files flowing into the document or into a path
  (`cite.ts` and `completion.ts` insert keys from a `.bib` the author may not have written;
  `definition.ts` turns a Ctrl-click on `\input{../../x}` into a file open); UTF-16 offsets after
  edits (`diagnostics.ts`, `positions.ts`); decorations that outlive a document replacement by Yjs;
  keystroke cost on a 1 MB document (design commitment: under 16 ms); `paste.ts` with HTML or very
  large clipboards.
- **Other `src/lib` modules without a read**: `lsp.ts` 233, `lsp-protocol.ts` 191, `lsp-diagnostics.ts`
  168, `outline.ts` 249, `conflict.ts` 103, `documents.ts` 94, `fuzzy.ts` 93, `assistant.ts`,
  `assistant-payload.ts`, `assistant-review.ts`, `comments.ts`, `paths.ts`, `commands.ts`,
  `session-commit.ts`, `announce.ts`, `App.svelte` 141 (the global key handler).

### P2: code written during the fifth-pass fixes has had no second reader

Each file below was written and tested by the session that fixed the finding, and mutation-checked, but
no one else has read it. A fresh read for defects is worth more here than anywhere: `src/lib/pdf/viewer.ts`
(rewritten), `find.ts`, `find-controller.ts`, `geometry.ts`, `regexp-escape.ts`;
`crates/texbib/src/tex_text.rs` (TeX escaping of catalogue text: the same class of defect it fixes,
`R5.16`); `src/lib/relay/awareness.ts` (`sanitisePeerUpdate`); the Join backup in `controller.svelte.ts`
(`keepCopyBeforeJoin`); `src-tauri/tests/capabilities.rs` and the opener scope in
`src-tauri/capabilities/default.json`; the anchored fixes in `crates/texlog/src/rules.rs`
(`anchor_tail`) and `src/lib/fix.ts` (`locateFix`); `src-tauri/src/project.rs` (`read_text`, the link
checks, the cycle-safe tree walk); `crates/abstract-tex-engine/src/tectonic.rs` (the single deadline,
`write_runnable_script`).

### P3: Rust with only partial coverage statements

**Pass 7 read:** `snapshots.rs`; `paste.rs` (the first 330 lines); `comments.rs` (`list`, `add_comment`);
`bibliography.rs`'s index and the entry-level health read (the rest of the health rules still unread);
`project.rs`'s `read_text` and `write_atomically`. **Still partial or unread:** `bibliography.rs` 1,777
(path handling in pass 4, the case rule in pass 5), `compile.rs` 1,393 (BibTeX and placement in pass 1),
`latexdiff.rs` 1,247 (retry and the working-copy lane in pass 2), `assistant_review.rs` 880 (the "review
flow" in pass 3), `github.rs` 407, `watcher.rs` 404 (pass 3), `lsp.rs` 240, `templates.rs` 207,
`consent.rs` 133. In crates: `abstract-tex-includes` (`graph.rs`
706, `scan.rs` 473: only the root detection and the scan changes), `texlog` (`tokenizer.rs` 522,
`resolver.rs` 406, `blg.rs` 158: the rule catalog was read in passes 2 and 5, these were not),
`texbib` (`keys.rs`, `health.rs`, `identify.rs`, `value.rs`), `abstract-tex-synctex` 670 (pass 3 says
SyncTeX was covered; the crate is not named), `abstract-tex-templates`, `abstract-tex-snapshot` 846,
`abstract-tex-relay` 540. Use the appendix to see which files no entry names.

### P3: the 51 `texlog` fixtures

`crates/texlog/fixtures/<name>/` each hold a `main.log` captured from a real run, the `main.tex`, and an
`expected.json`. None was read one by one. Read each `expected.json` beside its log: would the
explanation mislead an author? Is the `fix` an edit that actually repairs this input (the `R5.14`
class: a fix that edits the first match instead of where TeX stopped)? Is there a real input the rule
should match and does not?

### P3: not code, and platform branches

- `.github/workflows/verify.yml` (the CI has no TeX installed, so the engine gate skips there),
  `src-tauri/tauri.conf.json` beyond the CSP and asset scope, `scripts/`, `templates/` (each template
  has a test that builds it; nothing checks licences or fonts), and `0.1/DESIGN.md` against the code
  (its non-goals and the six rules).
- **Windows and macOS.** The only `cfg(windows)` code is the `taskkill` branch in
  `crates/abstract-tex-engine/src/process.rs`, and it is untested. WebView2 and WKWebView behave
  differently from this machine's WebKitGTK (the ledger's "Ctrl Z did nothing on Linux" is the
  example): path prefixes (`\\?\`), drive-letter case, case-insensitive file systems, key events.

## 4. Open issues (not fixed)

Search the ledger for the title. "Needs" says what closes it.

| # | Issue | Needs |
|---|---|---|
| 1 | **Comments have two rules for a folder that is not the repository root** (`R3.13`; ledger: "Two answers to 'this project folder is not a Git repository' for comments"). `comments.rs` refuses; the `project.rs` in `abstract-tex-comments` creates its own repository, and nothing calls it. | **A maintainer decision**: pick one, delete the other. |
| 2 | Comment authorship and resolution time are unauthenticated (`R3.14`). | A sentence in `DESIGN.md` saying it is acceptable among trusted coauthors. |
| 3 | The citation guard's plain-text rule misses `[12]` references (`R3.15`), and `texbib::is_citation_command` does not see `\textcquote`-style wrapper commands. | A stated heuristic, or shape handling for the wrappers. |
| 4 | A second writer in another process (a second window, a `git` command) is protected only by a swap that was shown to be leaky (`R3.10`). | A cross-process lock, or accept and document. |
| 5 | **Join backups are never pruned**: each join writes `.abstract-tex/join-backups/<file>.<timestamp>` (`R5.20`, my recommendation) and nothing removes old ones. | Keep the last N, or replace with the confirmation the ledger offers. Maintainer's call. |
| 6 | `R5.16` leftovers: capitals are not protected (`plain` prints "q&a" for "Q&A"), and the `by_statement` author fallback. | Small `texbib` changes; test with real BibTeX as before. |
| 7 | `R5.9` remainder: some keychain reads still run on the main thread. | Move them behind async commands. |
| 8 | `R5.24`: the file tree is built whole and eagerly (measured: 200,000 files, 0.72 s, 15 MB of JSON). | Only if a huge folder is a real case. |
| 9 | `texlog`'s `unwrap_lines` does not undo `\PackageError`'s own multi-line continuation (S6.1). | Recognise the `(packagename)` continuation shape. Low. |
| 10 | The commit graph's word counts are recomputed for a whole page of 200 rows and cached nowhere (S10.3c; about 111 ms). | A `commit id -> delta` map, if it is ever felt. |
| 11 | A merge commit is titled `Merge commit '<sha>'`; the code comment promises "Merge branch 'origin/main'". | Correct the comment (cosmetic). |
| 12 | ~~S9.9's draft PDF has never been seen on screen.~~ **Seen 15 Nov 2026 (sixth pass, Linux):** the pane showed the draft's page until the full PDF landed (ledger, the S9.9 entry). | Windows only, if it is wanted. |
| 13 | `GitError::PushRejected` has never been triggered by a test (libgit2's local transport runs no server hooks). | A real GitHub repository with branch protection, after the OAuth app exists. |
| 14 | Environmental, nothing to fix in code: `latexminted` 0.6.0 crashes on Python 3.14; `reqwest` cannot resolve DNS on the Windows machine while its security suite is on (`scripts/dev-proxy.py`). | Upstream; or the security-suite exception. |
| 15 | **Find in the PDF, known limits** (`c638d77`): no regular expressions; after a rebuild the current match is the first one at or after the page being read, not the one that was current. (A word hyphenated at a line end *is* found: probed.) The DOM half of `viewer.ts` has no automated test (Vitest runs in Node). | Acceptable; walk it on WebView2 and WKWebView. |
| 16 | **The oldest webview the app supports is not pinned or tested anywhere.** pdf.js's modern build assumes recent engines: it needs `RegExp.escape` (Chrome 136, Firefox 134, Safari 18.2; WebKitGTK before about 2.48 lacks it, and Ubuntu 24.04 ships 2.44). `08b5287` adds a stand-in for that one function, but pdf.js may need others, and `src/lib/pdf/viewer.ts` itself was only ever run on WebKitGTK 2.52.6 here. | **A maintainer decision**: state the minimum (in `DESIGN.md`), or switch to `pdfjs-dist/legacy` (viewer and worker), or both; then walk the PDF pane on the oldest one. |
| 17 | The ledger's own sections are unreliable: fixed entries sit under "Open" and under "Won't fix". | A tidy by the maintainer (move, never delete). |
| 18 | **A file that is not UTF-8 is refused, not supported** (`R7.8`; ledger sixth-pass-and-seventh-pass entry, items 8-10). The app cannot work on a Latin-1 or Windows-1252 project, which is common in older Portuguese, French, German and Spanish theses (`\usepackage[latin1]{inputenc}`). | **A maintainer decision**: (1) say in `DESIGN.md` that the app edits UTF-8 only, or (2) support legacy encodings by looking at the file on disk at write time and encoding back (touches `read_text`, `write_file`, the index readers, the include graph). |
| 19 | **Other strict `read_to_string` readers skip a non-UTF-8 file quietly:** `project::detect_root` (a Latin-1 root is never found), the include graph (`Unresolved::Unreadable`, by design), `compile.rs`'s two diagnostic-placement reads, `assistant_review.rs`' two, `comments.rs`' anchor read, `fragile_command.rs`. None writes. | Part of item 18's decision. |
| 20 | **Line endings:** a file that mixes CRLF and LF is saved with the majority's, which changes the minority lines' endings (shows in `git diff`); the conflict resolver's own write path (`resolveMergeConflict`) rebuilds the file from sections and was not read for `\r\n`; a lone `\r` is left as it is (`R7.11`). | Read `resolveMergeConflict` with a CRLF file in a real conflict; decide whether a mixed file should be left alone instead. |
| 21 | **Two fixes verified for the helper only:** the input-method guard (`R7.4`, no input method can be driven on the virtual display) and `R7.5`/`R7.11` on Windows and macOS. | Walk a Japanese or Chinese IME on a real desktop (type a file name, accept a candidate with Enter in the palette, New project and the commit offer); walk a CRLF file and a Cyrillic layout on Windows. |
| 22 | `paths.toRelative` compares code units case-insensitively and does not normalise Unicode (`R7.6`, "not fixed, noted"): on macOS a watcher reporting a decomposed folder name for a project the dialog returned composed would make every change be ignored. | A macOS run; compare NFC forms. |
| 23 | Global chords (`Ctrl O`, `Ctrl Shift N`) still pass through an open modal dialog (`R7.2`, "left alone"). | A design call: which dialogs block which chords. |

## 5. Fixed, but not verified where it matters

- **`git lfs track --`** (`R5.11`): the test uses a fake `git-lfs`; the real one is not installed here.
  In a scratch repository run `git lfs track -- --dry-run.bin` and check it writes the attribute.
- **Windows `taskkill` branch** (`R4.7`): cancelling a build ends the engine's children. Untested.
- **Real `latexdiff`** (A7): not installed here (the maintainer has the `apt` command); the lane's tests use fakes.
- **E4's case-insensitive branch**: first run on Windows or macOS CI. **E6's full-TeX CI step**, and the
  four `UNMEASURED` ceilings in `crates/abstract-tex-engine/tests/corpus.rs` (`conference`, `thesis`,
  `non-latin`, `pathological-preamble`): replace each with a measured number from a complete TeX.
- **The two dialog fixes** (`R4.8`, `R4.14`): "not yet walked in the app".
- **The `RegExp.escape` stand-in** (`08b5287`): tested under Node 22, which lacks the function, and
  not in a real webview that lacks it. This machine's WebKitGTK 2.52.6 has the native one, so the
  stand-in never runs in the app here; a real search of "well-known" and "U.S.A." passed on the native one.
- **`R7.4`, `R7.5`, `R7.11` off this machine** (see §4 item 21), and **`R7.2`'s `openFolder` reset**: it is
  tested with a faked `ipc`, not walked (the GTK folder picker drops keystrokes on the virtual display).
- **The widened opener scope** (`R5.4`): the opener may now open any `http:` or `https:` address, because
  a link in a PDF needs it. `file:`, `mailto:`, `javascript:`, `ftp:`, `data:` and `asset:` stay
  refused and `src-tauri/tests/capabilities.rs` pins that. Confirm it is the policy you want.

## 6. Only the maintainer can do

Push the local commits (39 before this file's); `cargo publish` of `texlog` and `texbib`; register the GitHub OAuth app (start
of S11.8, then `DESIGN.md` §10); the decisions in §4 items 1, 5 and 16; install `git-lfs` and `latexdiff`
on this machine; the CI full-TeX step; the first Windows and macOS runs of the platform-specific code.

## 7. Practical notes from the last sessions

- Always use `cargo` from `~/.cargo/bin`. Native builds use `/usr/bin/cc`; no `CC=` prefix.
- **Virtual display**: the window is 1440x900 at the top-left of a 1600x1100 screen; click coordinates
  are screen coordinates. Function keys do not reach the page through `xdotool` here; click the button
  instead. A scratch script using libxdo through `ctypes` can send wheel events and Ctrl+wheel (see
  how `scripts/xvfb-ui.py` loads `libxdo.so.3`).
- To see which web address the app opened, put a script named `xdg-open` first on `PATH` that appends
  its arguments to a log file (the real one would open a browser on the real desktop).
- **`pkill` pattern trap**: `pkill -f "abstract_tex_lib-"` killed the shell running it. Write the pattern
  with one character in brackets: `pkill -f "target/debug/abstract-te[x]"`.
- Do not `cat` long fixture documents (`main.tex` files with 1,000-line one-line paragraphs); cut the
  width (`cut -c1-200`).
- Probes for a suspicion go in the session scratchpad, not in the repository. Real inputs for the
  fuzzer are the repository's own `.tex`, `.bib`, `.log` and `.blg` files.
- A Tauri command that returns `()` arrives as `null`; a mock that returns `undefined` hides the bug.
- **Keyboard layouts on the virtual display:** `DISPLAY=:99 setxkbmap ru` then
  `scripts/xvfb-ui.py key ctrl+Cyrillic_el` presses Ctrl and the physical K key under a Russian layout;
  `setxkbmap us` puts it back. `git stash push <file>` between two runs gives an honest before and after
  (the dev server reloads the page in a few seconds).
- **Probing a pure `.ts` module without Vitest:** `node --experimental-strip-types --no-warnings probe.ts`
  (Node 22) runs it directly, if it has only erasable types and imports `.ts` paths. That is how
  `fuzzy.ts` and `outline.ts` were fed awkward inputs and benchmarked.
- **Reading a gate's exit code:** `pnpm verify > file 2>&1; echo "exit=$?"`, then `grep` the file. A
  background job that starts the gate in a subshell reports the *wrapper's* exit, not the gate's.
- **Looking at what the app wrote:** open a copy of a fixture, make one edit, `xxd` the file. Reading the
  text back through the same decoder shows nothing (that is how `R7.8` hid for months).
- The app's window is 1440x900 in the top-left of the 1600x1100 screen; the tree's rows are 24 px apart
  starting at y=88; crop a screenshot (`PIL.Image.crop`) rather than reading the whole 1600x1100.

## 8. Suggested order for an eighth pass

1. **Byte-level cases through the real app** (§2): a UTF-8 BOM, a lone `\r`, a file with no final newline,
   a 5 MB file, a name with a space, an accent and a `#`, a tab character, an emoji in a heading. Open,
   type, `xxd`. The two worst findings of the seventh pass were this and nothing else.
2. `resolveMergeConflict` with a CRLF file (§4 item 20), and the rest of §4's table that is not a decision.
3. The Rust P3 list: `watcher.rs`, `github.rs`, `templates.rs`, `consent.rs`, `lsp.rs`, the health rules in
   `bibliography.rs`, `compile.rs`, `latexdiff.rs`, `assistant_review.rs`.
4. The remaining components and state modules (§3), then `lsp.ts`, `lsp-protocol.ts`, `lsp-diagnostics.ts`.
5. The 51 `texlog` fixtures, then the non-code items.

**A method that paid off late in the sixth pass:** run the app on the virtual display and read `$TMPDIR/abstract-tex-xvfb.log`
after driving it. Vite forwards the webview's `console.error` there, which is how the typewriter-mode crash (`R6.21`)
was found; photographing the status bar every 120 ms found `R6.22`. Neither shows in a unit test or in a screenshot taken
at rest.

## Appendix: every source file, with a weak signal

Generated by `scripts/review-coverage.py --markdown` on 16 November 2026, after the seventh pass (181 files,
57,771 lines; 49 files, 11,708 lines are named by no review entry, down from 126 and 30,854). **It is a lead list, not a verdict, and it errs both
ways.** *Too few*: a pass can read a file and name it only by what it does ("the watcher"), so files the
fifth pass did read (`relay/live-session.ts`, `relay/awareness.ts`, `PdfPane.svelte`, `fix.ts`,
`math-preview.ts`, `src-tauri/src/synctex.rs`) show zero. *Too many*: a "Still not reviewed" sentence
names files it says were not read (`tectonic.rs`, `incremental.rs`, `draft.rs`), which counts as a
mention. **Authority is §3, which follows the ledger's own statements; use the table for sizes and to
find files nobody has named.** Re-run the script after you add your "Covered:" lines.

| Lines | File named | Crate named | File |
|---:|---:|---:|---|
| 389 | 0 |  | `src-tauri/src/synctex.rs` |
| 282 | 0 |  | `src/components/Drawer.svelte` |
| 255 | 0 |  | `src/lib/drawer.ts` |
| 255 | 0 |  | `src/lib/relay/live-session.ts` |
| 190 | 0 |  | `src/lib/compare.svelte.ts` |
| 184 | 0 |  | `src/components/ShareWindow.svelte` |
| 166 | 0 |  | `src/lib/relay/awareness.ts` |
| 133 | 0 |  | `src/lib/relay/provider.ts` |
| 116 | 0 |  | `src/components/DocumentMap.svelte` |
| 112 | 0 |  | `src/lib/github.svelte.ts` |
| 94 | 0 |  | `src/components/CrashReportWindow.svelte` |
| 90 | 0 |  | `src/components/ActivityBar.svelte` |
| 85 | 0 |  | `src/lib/relay/channel.ts` |
| 84 | 0 |  | `src/lib/pdf/geometry.ts` |
| 55 | 0 |  | `src/lib/pdf/pdf-test-support.ts` |
| 19 | 0 |  | `src/lib/placeholders.svelte.ts` |
| 13 | 0 |  | `src/lib/crash.svelte.ts` |
| 7 | 0 |  | `src/main.ts` |
| 5 | 0 |  | `src-tauri/build.rs` |
| 670 | 0 | 0 | `crates/abstract-tex-synctex/src/lib.rs` |
| 168 | 0 | 0 | `crates/abstract-tex-latexdiff/src/lib.rs` |
| 473 | 0 | 1 | `crates/abstract-tex-includes/src/scan.rs` |
| 278 | 0 | 1 | `crates/abstract-tex-github/src/repos.rs` |
| 251 | 0 | 1 | `crates/abstract-tex-github/src/device.rs` |
| 750 | 0 | 2 | `crates/abstract-tex-assistant/src/provider.rs` |
| 620 | 0 | 2 | `crates/abstract-tex-lsp/src/bridge.rs` |
| 307 | 0 | 2 | `crates/abstract-tex-assistant/src/inspect.rs` |
| 280 | 0 | 2 | `crates/abstract-tex-comments/src/anchor.rs` |
| 247 | 0 | 2 | `crates/abstract-tex-templates/src/instantiate.rs` |
| 205 | 0 | 2 | `crates/abstract-tex-lsp/src/transport.rs` |
| 154 | 0 | 2 | `crates/abstract-tex-lsp/src/server.rs` |
| 152 | 0 | 2 | `crates/abstract-tex-templates/src/render.rs` |
| 142 | 0 | 2 | `crates/abstract-tex-assistant/src/keys.rs` |
| 67 | 0 | 2 | `crates/abstract-tex-assistant/src/client.rs` |
| 9 | 0 | 2 | `crates/abstract-tex-templates/build.rs` |
| 522 | 0 | 6 | `crates/texlog/src/tokenizer.rs` |
| 406 | 0 | 6 | `crates/texlog/src/resolver.rs` |
| 61 | 0 | 6 | `crates/texlog/build.rs` |
| 850 | 0 | 10 | `crates/texbib/src/parse.rs` |
| 494 | 0 | 10 | `crates/texbib/src/acquire/arxiv.rs` |
| 399 | 0 | 10 | `crates/texbib/src/acquire/zotero.rs` |
| 280 | 0 | 10 | `crates/texbib/src/keys.rs` |
| 249 | 0 | 10 | `crates/texbib/src/acquire/doi.rs` |
| 249 | 0 | 10 | `crates/texbib/src/health.rs` |
| 249 | 0 | 10 | `crates/texbib/src/tex_text.rs` |
| 210 | 0 | 10 | `crates/texbib/src/acquire/identify.rs` |
| 210 | 0 | 10 | `crates/texbib/src/render.rs` |
| 174 | 0 | 10 | `crates/texbib/src/value.rs` |
| 48 | 0 | 10 | `crates/texbib/build.rs` |
| 407 | 1 |  | `src-tauri/src/github.rs` |
| 404 | 1 |  | `src-tauri/src/watcher.rs` |
| 327 | 1 |  | `src/components/NewProjectWindow.svelte` |
| 315 | 1 |  | `src/components/PdfPane.svelte` |
| 253 | 1 |  | `src/lib/editor/cite.ts` |
| 240 | 1 |  | `src-tauri/src/lsp.rs` |
| 235 | 1 |  | `src/components/CommandPalette.svelte` |
| 231 | 1 |  | `src/components/AssistantView.svelte` |
| 217 | 1 |  | `src/components/AssistantPayload.svelte` |
| 207 | 1 |  | `src-tauri/src/templates.rs` |
| 203 | 1 |  | `src/components/StatusBar.svelte` |
| 199 | 1 |  | `src/components/AssistantReview.svelte` |
| 195 | 1 |  | `src-tauri/src/git.rs` |
| 193 | 1 |  | `src/lib/pdf/find.ts` |
| 192 | 1 |  | `src/components/ConflictResolver.svelte` |
| 191 | 1 |  | `src/lib/lsp-protocol.ts` |
| 187 | 1 |  | `src/components/ZoteroLink.svelte` |
| 184 | 1 |  | `src/components/BibliographyHealth.svelte` |
| 182 | 1 |  | `src/lib/state.svelte.ts` |
| 180 | 1 |  | `src/components/CommentsView.svelte` |
| 180 | 1 |  | `src/components/QuickOpen.svelte` |
| 169 | 1 |  | `src/lib/editor/placeholders.ts` |
| 168 | 1 |  | `src/lib/lsp-diagnostics.ts` |
| 141 | 1 |  | `src/lib/editor/completion.ts` |
| 138 | 1 |  | `src/components/DiffView.svelte` |
| 132 | 1 |  | `src/components/JoinWindow.svelte` |
| 123 | 1 |  | `src/lib/conflict.ts` |
| 113 | 1 |  | `src/components/Sidebar.svelte` |
| 111 | 1 |  | `src/lib/bibliography.svelte.ts` |
| 110 | 1 |  | `src/lib/editor/focus.ts` |
| 104 | 1 |  | `src-tauri/src/lfs.rs` |
| 104 | 1 |  | `src/lib/relay/invite.ts` |
| 101 | 1 |  | `src/lib/assistant.svelte.ts` |
| 72 | 1 |  | `src/components/SessionCommitOffer.svelte` |
| 72 | 1 |  | `src/lib/assistant.ts` |
| 72 | 1 |  | `src/lib/editor/hover.ts` |
| 72 | 1 |  | `src/lib/templates.svelte.ts` |
| 70 | 1 |  | `src/lib/assistant-review.ts` |
| 68 | 1 |  | `src/lib/editor/definition.ts` |
| 66 | 1 |  | `src/components/TreeItem.svelte` |
| 63 | 1 |  | `src/lib/comments.ts` |
| 58 | 1 |  | `src/lib/pdf/regexp-escape.ts` |
| 57 | 1 |  | `src/lib/assistant-payload.ts` |
| 52 | 1 |  | `src/lib/paths.ts` |
| 51 | 1 |  | `src/lib/editor/diff.ts` |
| 48 | 1 |  | `src/lib/commands.ts` |
| 48 | 1 |  | `src/lib/editor/symbols.ts` |
| 48 | 1 |  | `src/lib/snapshots.svelte.ts` |
| 47 | 1 |  | `src/lib/clone.svelte.ts` |
| 38 | 1 |  | `src/lib/live.svelte.ts` |
| 37 | 1 |  | `src/lib/fix.ts` |
| 31 | 1 |  | `src/lib/editor/positions.ts` |
| 28 | 1 |  | `src/lib/editor/synctex.ts` |
| 25 | 1 |  | `src/lib/diffview.svelte.ts` |
| 23 | 1 |  | `src/lib/keys.ts` |
| 20 | 1 |  | `src/lib/announce.ts` |
| 18 | 1 |  | `src/lib/session-commit.ts` |
| 17 | 1 |  | `src/lib/comments.svelte.ts` |
| 15 | 1 |  | `src/lib/editor/selection.ts` |
| 706 | 1 | 1 | `crates/abstract-tex-includes/src/graph.rs` |
| 520 | 1 | 1 | `crates/abstract-tex-relay/src/lib.rs` |
| 163 | 1 | 1 | `crates/abstract-tex-github/src/lib.rs` |
| 153 | 1 | 1 | `crates/abstract-tex-github/src/store.rs` |
| 20 | 1 | 1 | `crates/abstract-tex-includes/src/lib.rs` |
| 20 | 1 | 1 | `crates/abstract-tex-relay/src/main.rs` |
| 969 | 1 | 2 | `crates/abstract-tex-comments/src/store.rs` |
| 873 | 1 | 2 | `crates/abstract-tex-assistant/src/guard.rs` |
| 813 | 1 | 2 | `crates/abstract-tex-engine/src/tectonic.rs` |
| 356 | 1 | 2 | `crates/abstract-tex-engine/src/draft.rs` |
| 336 | 1 | 2 | `crates/abstract-tex-assistant/src/edit.rs` |
| 298 | 1 | 2 | `crates/abstract-tex-engine/src/incremental.rs` |
| 2474 | 1 | 6 | `crates/texlog/src/rules.rs` |
| 376 | 1 | 6 | `crates/abstract-tex-git/src/branches.rs` |
| 158 | 1 | 6 | `crates/texlog/src/blg.rs` |
| 136 | 1 | 6 | `crates/abstract-tex-git/src/tags.rs` |
| 689 | 1 | 10 | `crates/texbib/src/acquire/isbn.rs` |
| 3269 | 2 |  | `src/lib/controller.svelte.ts` |
| 1371 | 2 |  | `src-tauri/src/commands.rs` |
| 1247 | 2 |  | `src-tauri/src/latexdiff.rs` |
| 502 | 2 |  | `src-tauri/src/paste.rs` |
| 497 | 2 |  | `src-tauri/src/crash.rs` |
| 479 | 2 |  | `src-tauri/src/comments.rs` |
| 384 | 2 |  | `src/lib/git.svelte.ts` |
| 361 | 2 |  | `src-tauri/src/assistant_explain.rs` |
| 329 | 2 |  | `src/lib/editor/diagnostics.ts` |
| 272 | 2 |  | `src/components/CloneWindow.svelte` |
| 266 | 2 |  | `src/lib/editor/math-preview.ts` |
| 233 | 2 |  | `src/lib/lsp.ts` |
| 211 | 2 |  | `src-tauri/src/fragile_command.rs` |
| 205 | 2 |  | `src/lib/editor/paste.ts` |
| 170 | 2 |  | `src-tauri/src/snapshots.rs` |
| 167 | 2 |  | `src/components/SnapshotViewer.svelte` |
| 141 | 2 |  | `src/App.svelte` |
| 133 | 2 |  | `src-tauri/src/consent.rs` |
| 105 | 2 |  | `src/lib/shortcuts.ts` |
| 94 | 2 |  | `src/lib/documents.ts` |
| 76 | 2 |  | `src/lib/pdf/find-controller.ts` |
| 846 | 2 | 2 | `crates/abstract-tex-snapshot/src/lib.rs` |
| 512 | 2 | 2 | `crates/abstract-tex-templates/src/lib.rs` |
| 223 | 2 | 2 | `crates/abstract-tex-sidecar/src/lib.rs` |
| 200 | 2 | 2 | `crates/abstract-tex-assistant/src/explain.rs` |
| 178 | 2 | 2 | `crates/abstract-tex-engine/src/lib.rs` |
| 103 | 2 | 2 | `crates/abstract-tex-assistant/src/lib.rs` |
| 36 | 2 | 2 | `crates/abstract-tex-comments/src/lib.rs` |
| 36 | 2 | 2 | `crates/abstract-tex-lsp/src/lib.rs` |
| 1335 | 3 |  | `src-tauri/src/project.rs` |
| 1081 | 3 |  | `src/components/SourceControl.svelte` |
| 912 | 3 |  | `src/lib/ipc.ts` |
| 648 | 3 |  | `src-tauri/src/assistant.rs` |
| 257 | 3 |  | `src/lib/outline.ts` |
| 238 | 3 |  | `src/lib/editor/setup.ts` |
| 204 | 3 |  | `src/lib/fuzzy.ts` |
| 103 | 3 |  | `src/lib/editor/typewriter.ts` |
| 97 | 3 |  | `src/components/Tabs.svelte` |
| 44 | 3 |  | `src/lib/dialog.ts` |
| 495 | 3 | 2 | `crates/abstract-tex-engine/src/process.rs` |
| 238 | 3 | 2 | `crates/abstract-tex-comments/src/project.rs` |
| 326 | 3 | 3 | `crates/abstract-tex-reconcile/src/lib.rs` |
| 1831 | 4 |  | `src-tauri/src/bibliography.rs` |
| 1393 | 4 |  | `src-tauri/src/compile.rs` |
| 880 | 4 |  | `src-tauri/src/assistant_review.rs` |
| 400 | 4 |  | `src/lib/pdf/viewer.ts` |
| 538 | 4 | 2 | `crates/abstract-tex-engine/src/latexmk.rs` |
| 688 | 4 | 4 | `crates/texwords/src/lib.rs` |
| 380 | 5 |  | `src/lib/document.ts` |
| 3877 | 6 | 6 | `crates/abstract-tex-git/src/lib.rs` |
| 168 | 6 | 6 | `crates/texlog/src/lib.rs` |
| 198 | 7 |  | `src/components/Editor.svelte` |
| 68 | 10 | 10 | `crates/texbib/src/acquire/mod.rs` |
| 64 | 10 | 10 | `crates/texbib/src/lib.rs` |
| 236 | 12 |  | `src-tauri/src/lib.rs` |
| 6 | 12 |  | `src-tauri/src/main.rs` |
