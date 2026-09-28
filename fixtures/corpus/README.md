# The golden corpus

Eight documents from `DESIGN.md` §8, built by `crates/abstract-tex-engine/tests/corpus.rs`
(S9.1). All of them must keep compiling after every change; the broken one must keep producing
exactly the diagnostics in its `expected.json`. They are also what the performance work in
sprint 9 times.

| Document | Shape | On the bundled engine |
|---|---|---|
| `conference` | IEEE two-column paper, BibTeX, one TikZ figure | builds, 2 pages |
| `thesis` | report class, six `\include`d chapters, TikZ + pgfplots, biblatex, cleveref | builds, 62 pages |
| `beamer` | Madrid theme, overlays, TikZ | builds, 13 pages |
| `tikz-figures` | six heavy figures: 3-D surface, trees, matrices, computed colours | builds, 3 pages |
| `minted` | code listings through Pygments | **does not build** (see below) |
| `non-latin` | fontspec + polyglossia; Greek, Cyrillic and Hebrew | builds, 1 page |
| `broken` | one error after three warnings | fails, with the four diagnostics in `expected.json` |
| `pathological-preamble` | ~45 packages, tcolorbox `most`, pgfplots, xparse, a real `\qty` clash | builds, 1 page |

Prose is generated (`lipsum`) wherever only length matters. The structure and the preambles
are what a build spends its time on, so those are real.

Run it (the engine fetches packages on the first run):

```
cargo test -p abstract-tex-engine --test corpus -- --ignored --nocapture
ABSTRACT_TEX_RECORD_CORPUS=1 cargo test -p abstract-tex-engine --test corpus -- --ignored   # re-record broken/
```

On the maintainer's Windows machine the engine cannot resolve DNS by itself; run
`python scripts/dev-proxy.py` and set `HTTPS_PROXY=http://127.0.0.1:3128` first
(`bugs-issues-fixes.md`).

## What building it found

- **`minted` cannot build on the bundled engine.** It needs `-shell-escape` and a `pygmentize`
  on `PATH`. The app runs Tectonic without `-Z shell-escape` on purpose, since shell-escape runs
  arbitrary commands from the document, and this machine has no Pygments anyway. This is the gap
  `DESIGN.md` §9 names; the route through it is S9.4's system-TeX switch. The test requires the
  build to fail, so the day it starts building, the test says so.
- **Fonts in Tectonic's bundle are found by file name, not family name.**
  `\setmainfont{DejaVu Serif}` fails with "font cannot be found"; `\setmainfont{FreeSerif.otf}`
  works. An author following any fontspec tutorial hits this first. It's worth a texlog rule
  whose explanation says so (not yet written).
- **`physics` and `siunitx` both define `\qty`.** Whichever loads last wins, and the loser's
  calls fail with "Missing $ inserted" far from the cause. The pathological preamble keeps the
  clash and avoids the call. That is a candidate rule too.
- **The thesis runs BibTeX once per chapter.** `\include` gives every chapter its own `.aux`,
  and Tectonic runs BibTeX on each (seven runs), then reruns TeX "because bibtex was run", on
  every build, even a warm one with nothing changed. A warm no-change rebuild took about 12 s
  from the command line against a 1.2 s target (21–26 s with SyncTeX on, as the app runs it).
  S9.2 fixed this: a warm build is now one `--pass tex` reading the previous `.aux`/`.bbl`, and
  only a changed citation brings BibTeX back. The thesis's warm build is now a 3.3 s median.

## Timings

`cargo test -p abstract-tex-engine --test corpus -- --ignored --nocapture warm_build_timings`
writes `target/corpus-report.json`. On the maintainer's Windows machine, five runs each, 28 Sep 2026:

| Document | Cold | Warm (comment edit), median | Steps |
|---|---:|---:|---|
| conference | 2.3 s | 0.69 s | 1 pass |
| thesis | 14.0 s | 3.36 s (prose edit: 3.29 s) | 1 pass |
| beamer | 3.7 s | 1.72 s | 1 pass |
| tikz-figures | 7.0 s | 3.42 s | 1 pass |
| non-latin | 1.6 s | 0.76 s | 1 pass |
| pathological-preamble | 5.2 s | 2.46 s | 1 pass |
