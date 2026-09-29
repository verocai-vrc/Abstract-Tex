# `shell-escape` — what a build with shell escape on may touch (S9.12)

One small project, built twice by `crates/abstract-tex-engine/tests/latexmk.rs` against a real
TeX Live: once with `BuildJob::shell_escape` off, once on.

`latexmk` has no equivalent of Tectonic's `-Z shell-escape-cwd`, so with shell escape on the
engine is run from the build folder instead of from here — otherwise `\write18` writes into the
author's source tree, which DESIGN.md §5.8 forbids. Every other file here exists to prove the
move did not take a search away:

| file | what it pins |
|---|---|
| `main.tex` | two `\write18` commands: where they run, and what they can no longer read |
| `housestyle.sty` | a package beside the manuscript — `TEXINPUTS` |
| `sections/sub.tex` | `\input` from a subfolder — `TEXINPUTS` |
| `chapters/one.tex` | `\include`, which writes its own `.aux` under the build folder |
| `figures/dot.png` | `\includegraphics` through `\graphicspath` — `TEXINPUTS`. A 1×1 PNG, 69 bytes: the smallest thing `graphicx` will accept |
| `refs.bib` | BibTeX's own search — `BIBINPUTS`, a different variable from `TEXINPUTS` |

The test asserts the source tree is byte-for-byte the same list of files after each build.
