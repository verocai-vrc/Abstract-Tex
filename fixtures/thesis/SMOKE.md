# Manual smoke script — `fixtures/thesis`

Rung 4 of the verification ladder (`0.1/SPRINTS.md` §1). This is the v0.2 exit demo
(`DESIGN.md` §7): navigate a six-file thesis entirely from the keyboard, and click any
paragraph in the PDF to land on the right line of the right file. Run after any loop that
touches the include graph (S4.1), the document map (S4.2), or the command palette (S4.3).

Launch:

```
pnpm tauri dev                        # then Ctrl+O and pick fixtures/thesis
PREAMBLE_OPEN=fixtures/thesis pnpm tauri dev    # opens it on start (bash / CI)
$env:PREAMBLE_OPEN='fixtures/thesis'; pnpm tauri dev   # PowerShell
```

On the maintainer's machine Tectonic needs the DNS workaround the first time a package is
fetched: run `python scripts/dev-proxy.py` and set `HTTPS_PROXY=http://127.0.0.1:3128`.

## §1 The tree and the include graph (S4.1)

1. The tree shows all six files: `main.tex` (with a `root` badge), `preamble.tex`, and the
   four chapters under `sections/` — `introduction.tex`, `background.tex`, `results.tex`,
   `conclusion.tex`.
2. Open `main.tex`: the editor shows `\input{preamble}` followed by four `\include`s. The
   PDF builds and lands on `main.pdf`, nine pages, no drawer (no errors, no warnings).
3. Open `sections/background.tex` and change a word in the "Distributing the bucket"
   paragraph. Stop typing. Within about a second the status bar shows `Compiling…` and the
   PDF rebuilds — a change to an `\include`d file, not just the root, triggers the build.
4. Open `preamble.tex` and change a word in the `\sysname` command's expansion (e.g. rename
   it). Stop typing: this also triggers a rebuild, and the renamed term now appears
   throughout the built PDF (it is `\input`, not `\include`, but the watcher treats it the
   same way).

## §2 Document map (S4.2)

1. With `sections/results.tex` active, open the document map panel. It lists
   `Experimental setup` and `Convergence after a traffic shift` as sections, the table
   (`tab:throughput`) and the figure (`fig:latency`) with their captions, and the labels
   defined in this file (`ch:results`, `tab:throughput`, `fig:latency`) — nothing from
   another chapter, since the map is scoped to the active file.
2. Click the `Convergence after a traffic shift` entry: the editor scrolls to that
   `\section` and places the cursor on its line.
3. Switch tabs to `sections/introduction.tex`: the document map updates to that file's own
   structure — `Contributions` and `Why a coordinator is the wrong default` as sections,
   no figures or tables, since this chapter has none.
4. Switch to `sections/background.tex`: the map shows the nested structure — `The token
   bucket` as a section with `Distributing the bucket` indented under it as a subsection.

## §3 Command palette (S4.3)

1. `Ctrl K` opens the palette with the input focused. With nothing typed, open tabs come
   first, then other project actions.
2. Type `conclusion`: `sections/conclusion.tex` rises to the top with the matched letters
   bold. `Enter` opens it in a new tab and closes the palette.
3. Type `convergence` (no file has that name, only a section title): the palette's
   fuzzy match still surfaces `Convergence after a traffic shift` in
   `sections/results.tex` from the document map's own section list. `Enter` opens that
   file with the cursor on the section line.
4. `Esc` closes the palette without navigating anywhere. Clicking outside the palette
   also closes it.

## §4 Cross-file navigation and SyncTeX (S3.3c, S3.4/S3.5)

1. In `sections/results.tex`, `Ctrl`-click (or your platform's go-to-definition chord) on
   `\ref{sec:intro-contributions}` in the opening paragraph. `sections/introduction.tex`
   comes to the front in its own tab and the cursor lands on the `\label{sec:intro-
   contributions}` line under "Contributions" — a cross-file reference, not a
   same-file one.
2. Similarly, `Ctrl`-click `\ref{sec:token-bucket}` in `sections/introduction.tex`'s
   "Contributions" section: `sections/background.tex` comes forward with the cursor on
   the token-bucket section's label.
3. In the PDF pane, scroll to the table in the results chapter (page showing
   "Table~1: Median and 99th-percentile decision latency…") and click inside it. Focus
   moves to the editor, `sections/results.tex`'s tab comes to the front (creating one if
   it is not already open), and the cursor lands on the `\begin{table}` line — not just
   the right file, the right line.
4. Click a sentence in the conclusion chapter's "Limitations" section in the PDF: the
   cursor lands on that sentence's line in `sections/conclusion.tex`, confirming the
   click-to-source direction works this deep into a multi-`\include` document, not only on
   the first chapter.
