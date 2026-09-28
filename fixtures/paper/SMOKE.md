# Manual smoke script — `fixtures/paper`

Rung 4 of the verification ladder (`0.1/SPRINTS.md` §1). Run after any loop that touches the
compile loop, the editor, or the PDF pane. Each section is a flow from `DESIGN.md` §6.

Launch:

```
pnpm tauri dev                       # then Ctrl+O and pick fixtures/paper
ABSTRACT_TEX_OPEN=fixtures/paper pnpm tauri dev    # opens it on start (bash / CI)
$env:ABSTRACT_TEX_OPEN='fixtures/paper'; pnpm tauri dev   # PowerShell
```

On the maintainer's machine Tectonic needs the DNS workaround the first time a package is
fetched: run `python scripts/dev-proxy.py` and set `HTTPS_PROXY=http://127.0.0.1:3128`.

## §1 First run and the write loop (S1.10, v0.1 exit)

1. The tree shows `main.tex` with a `root` badge; the editor shows it highlighted; the status
   bar shows `Tectonic 0.17.0`.
2. Within a few seconds the PDF appears on the right. Status bar: `Built in N.Ns`.
3. Change a word in the abstract. Stop typing. The status bar shows `Compiling…` after about
   0.7 s and the PDF updates without flashing or jumping to the top.
4. Scroll the PDF to page 3, edit again: the PDF stays on page 3 after the rebuild.

## §2 External change and conflict (S2.1)

1. In another editor, change a word in `main.tex` and save. The change appears in Abstract-Tex's
   editor without the cursor jumping, and a rebuild starts.
2. Type in Abstract-Tex and, within 0.7 s, save a different change from the other editor. A red
   bar appears: *changed on disk while you had unsaved edits*. Nothing was merged.
3. *Load from disk* replaces the buffer with the disk version; *Keep mine* writes the buffer
   over the disk version. Either way the bar goes away and a build runs.

## §3 Compile feedback (S2.2)

1. Delete the `\end{document}` line's closing brace. Within a second the drawer opens with
   `1 error stopped the build`, a line number, and the message. The PDF pane still shows the
   last good PDF.
2. Click the error: the cursor lands on that line.
3. *Raw log* shows the log; *Hide raw output* hides it. The raw log is never shown by default.
4. Restore the brace: the drawer closes on the next successful build.
5. Delete `.abstract-tex/` (or use a document with a package not yet cached) so Tectonic fetches
   packages on the next build. While it runs, the status bar's `Compiling… N.Ns` grows a
   `· Downloading <package>` suffix that changes as each package arrives — never a spinner
   frozen on the same text for the whole fetch.

## §4 Keyboard (S2.4)

1. `Ctrl S` writes immediately (dirty dot disappears). `Ctrl B` and `F5` save and build. `Ctrl O`
   opens the folder picker. `Ctrl Z` undoes your edit but never an external change.
2. Each of those works with the editor focused, with the file tree focused (click a row first),
   and with the PDF pane focused. Pressing `Ctrl B` once inside the editor starts exactly one
   build — watch the status bar's generation not tick twice.
3. `Ctrl P` opens the file list with the input focused. With nothing typed, open tabs come first.
   Type `res` in a project with `sections/results.tex`: it rises to the top with the matched
   letters bold. `↓` `↑` move the selection, `Enter` opens the file in a tab and closes the list,
   `Esc` or `Ctrl P` again closes it without opening anything. Clicking outside also closes it.

## §5 Diagnostics drawer (S2.7, S6.2, S6.3)

Open `fixtures/broken` instead.

1. The build fails and the drawer opens with **`_ used outside maths`** under a `main.tex`
   heading, marked ERROR, with two sentences about subscripts and maths mode — not `Missing $
   inserted`. The status bar says `1 error`. TeX's own words appear only after clicking *Raw log*.
2. A red dot sits in the gutter beside line 5; hovering it shows the title. Type a blank line
   above it: the dot moves down with its line.
3. Click the card: the cursor lands on line 5. Open a second tab, click the card again: the root
   file's tab comes to the front first, then the cursor moves.
4. Click the card's own *Raw log*: the drawer switches to the transcript, scrolled to
   `! Missing $ inserted.` with those words highlighted, not to the top of the file. *Back to
   explanations* returns to the card. Fix nothing, press `Ctrl B`, and while the raw view is still
   open watch it change to the new build's log (the date line at the top is the easiest tell).
5. Click **Escape as `\_`** (S6.2): line 5 changes to `\_`, the build reruns and succeeds, and
   `Ctrl Z` puts the `_` back as a single undo step.
6. Back in `fixtures/paper`, add `\cite{nosuch}` and build: the PDF still appears, the drawer
   stays closed (the build succeeded), and the status bar shows `Built in N.Ns · 1 warning`.
   Clicking it opens the drawer with an amber card marked WARNING: **`nosuch` is cited but not in
   the bibliography**.
7. Trigger an error the catalog does not know (e.g. `\hspace{99999pt}`): the card has a dashed
   border and says Abstract-Tex has no explanation yet, quoting TeX — still never the raw log.
8. Grouping and filtering (S6.3) need more than one file: open a fresh copy of `fixtures/thesis`
   (no `.abstract-tex/build` yet), add a line with a bare `_` to the end of
   `sections/background.tex`, and build. The error halts the run before the pass that resolves
   cross-references, so on a first build every `\ref` is also an undefined-reference warning —
   six of them, four in `background` and two in `introduction`; a build directory that already
   holds `.aux` files from a clean build shows fewer or none, which is TeX, not the drawer. The
   drawer shows a `sections/background.tex` heading first (its ERROR card, then that chapter's
   WARNING cards by line), then a `sections/introduction.tex` heading. The header chips read
   `All 7 · 1 error · 6 warnings · This file`. Click `1 error`: only the one card remains, the
   headings keep their order. Click `6 warnings`: the error card goes, and `background` is still
   above `introduction`. With `sections/introduction.tex` as the active tab click *This file*:
   only its heading remains; switch to `preamble.tex` and the body says `Nothing reported in
   preamble.tex. 7 problems are in other files.` with a *Show all* button.
9. Gutter routing (S6.3): with the thesis build above, `sections/introduction.tex`'s tab shows
   amber dots on its own two lines and no red one; `sections/background.tex`'s shows the red dot
   on its last line; `main.tex` shows none.
10. With the drawer closed, `Ctrl K` → `Show raw log` opens it straight onto the transcript;
    `Toggle diagnostics` opens and closes the drawer from the keyboard.

## §6 Multi-document tabs (S2.3)

`fixtures/paper` is one file, so use the `+` in the sidebar to create a second one (e.g.
`notes.tex`) — any project with two or more files works the same way.

1. With `main.tex` open, open the second file from the tree: a second tab appears next to it,
   and the second file becomes active.
2. Type in the second tab's buffer without waiting for the 700 ms save, then click back to the
   first tab: the second tab still shows a dirty dot, and switching to it and back does not
   reset or reload its text — it never touched disk.
3. With unsaved edits in *both* tabs, press `Ctrl B`: the status bar shows `Compiling…`, and both
   files land on disk (check the second file's mtime, or open it in another editor) even though
   only one of them was the active tab.
4. Click a tab's `×` to close it while it has an unsaved edit: the edit is written to disk first
   (no silent loss), the tab disappears, and the remaining tab becomes active.
