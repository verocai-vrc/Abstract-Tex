# Manual smoke script — `fixtures/torture`

Rung 4 of the verification ladder (`0.1/SPRINTS.md` §1), and the v0.3 exit demo
(`DESIGN.md` §7): a purpose-built twenty-error torture document, every error resolving to the
correct file and line with a plain-language explanation, and no raw log shown by default
anywhere. `crates/abstract-tex-engine/tests/torture.rs` is this same walk, automated, against the
real engine; `captures/` are its recorded logs. This script is the walk by hand, in the app,
where the criterion is finally about what an author *sees*.

Launch:

```
pnpm tauri dev                          # then Ctrl+O and pick fixtures/torture
ABSTRACT_TEX_OPEN=fixtures/torture pnpm tauri dev
```

The engine halts at the first `!` error (`crates/texlog/fixtures/README.md`), so the twenty
mistakes are met one at a time: fix one, the build reruns, the next appears. The two warnings
(chapters 04 and 05) never halt anything and stay in the drawer until they too are fixed.

## §1 The first error, and the raw log staying hidden (S6.3, S6.4)

1. Open the project. The build fails; the drawer opens itself with one error and no warnings.
   The status bar says the build failed. **Nowhere on screen is a line of TeX's own log** — not
   in the drawer, not in a notice, not in the status bar.
2. The single card's heading is `sections/01-undefined-control-sequence.tex` — with the `.tex`,
   although `main.tex` writes `\input{sections/01-undefined-control-sequence}` without it (the
   engine echoes the name as written; the drawer completes it against the include graph, S6.4).
3. Click the card. The chapter opens as a tab and the cursor lands on line 3, the line with
   `\textbold`. The gutter dot is on this tab, on line 3, and on no other tab.
4. Click the card's **Raw log**. Now, and only now, TeX's own words appear, scrolled to and
   highlighting `Undefined control sequence.` Close the raw view.

## §2 The walk: twenty mistakes, one file and line each

For every row: the card names this file, clicking it lands on this line, the explanation is
sentences (no `!`, no `l.NN`, no TeX jargon left unexplained), and the fix column says what the
card's button offers. Make the correction (by button where there is one, by hand otherwise),
stop typing, wait for the rebuild, and move to the next row. Fixing chapter N must remove its
card; nothing from an earlier chapter may reappear.

| # | File (`sections/`) | Line | The card explains | Fix button |
|---|---|---|---|---|
| 01 | `01-undefined-control-sequence.tex` | 3 | `\textbold` is not a command TeX knows | — |
| 02 | `02-missing-dollar.tex` | 3 | a `_` outside maths | **Escape as `\_`** |
| 03 | `03-unbalanced-braces.tex` | 3 | a `}` with no matching `{` | — |
| 04 | `04-undefined-reference.tex` | 3 | warning: `sec:resolver` is not defined anywhere | — |
| 05 | `05-undefined-citation.tex` | 3 | warning: `nobody2026` is not in any bibliography | — |
| 06 | `06-misplaced-alignment-tab.tex` | 3 | a `&` outside a table | **Escape as `\&`** |
| 07 | `07-extra-alignment-tab.tex` | 7 | more `&` in the row than the table has columns | — |
| 08 | `08-undefined-environment.tex` | 3 | there is no environment called `itemise` | — |
| 09 | `09-mismatched-environment.tex` | 6 | `enumerate` opened on line 3, closed by `\end{itemize}` | — |
| 10 | `10-missing-item.tex` | 5 | text inside `itemize` before any `\item` (TeX names the `\end`) | — |
| 11 | `11-illegal-unit-of-measure.tex` | 4 | `\vspace{12}` has no unit | **Add pt** |
| 12 | `12-fragile-command-in-moving-argument.tex` | 1 | something in the section title cannot be moved; try `\protect` | — (see below) |
| 13 | `13-caption-outside-float.tex` | 3 | `\caption` with no `figure`/`table` around it | — |
| 14 | `14-display-math-wrong-delimiter.tex` | 4 | `\[` closed with `$` | **Close with `\]`** |
| 15 | `15-verb-unterminated.tex` | 3 | `\verb\|` never closed on its line | **Close with `\|`** |
| 16 | `16-undefined-color.tex` | 3 | no colour called `signalred` | — |
| 17 | `17-image-not-found.tex` | 5 | `figures/drawer.png` does not exist | — |
| 18 | `18-file-not-found.tex` | 3 | `sections/appendix-b.tex` does not exist | — |
| 19 | `19-preamble-only-command.tex` | 3 | `\usepackage` only works before `\begin{document}` | — |
| 20 | `20-include-cannot-be-nested.tex` | 3 | `\include` inside an `\include`d file | **Change to `\input`** |

Row 12 has no button on purpose: the title is longer than TeX prints of a context line, so the
log no longer contains the `\footnote` a fix would need (`bugs-issues-fixes.md`, Open, S6.4).
The explanation still says what to do. Row 20's fix makes `sections/appendix.tex` appear as a
last section, which is the intended outcome, not a new error.

## §3 The end state, and the drawer's own controls along the way

1. After row 20 the build succeeds: the PDF shows the title, twenty short sections and the
   appendix, and the drawer closes itself (a clean build has nothing to say).
2. At any point during §2 with several cards showing (rows 06–19 keep chapters 04 and 05's
   warnings until those are fixed): the **errors / warnings** filter hides without reordering
   the file headings; **this file** shows only the active tab's cards; every count in the header
   is live.
3. Press Ctrl-Z after a button fix: the change comes back, exactly like an undone keystroke, and
   the rebuild reports the mistake again.
