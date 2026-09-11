# Manual smoke script — `fixtures/paper`

Rung 4 of the verification ladder (`0.1/SPRINTS.md` §1). Run after any loop that touches the
compile loop, the editor, or the PDF pane. Each section is a flow from `DESIGN.md` §6.

Launch:

```
pnpm tauri dev                       # then Ctrl+O and pick fixtures/paper
PREAMBLE_OPEN=fixtures/paper pnpm tauri dev    # opens it on start (bash / CI)
$env:PREAMBLE_OPEN='fixtures/paper'; pnpm tauri dev   # PowerShell
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

1. In another editor, change a word in `main.tex` and save. The change appears in Preamble's
   editor without the cursor jumping, and a rebuild starts.
2. Type in Preamble and, within 0.7 s, save a different change from the other editor. A red
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
5. Delete `.preamble/` (or use a document with a package not yet cached) so Tectonic fetches
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

## §5 Diagnostics drawer (S2.7)

Open `fixtures/broken` instead. The drawer explains the underscore in a sentence (once S2.6
lands); until then it shows TeX's message and line 5.

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
