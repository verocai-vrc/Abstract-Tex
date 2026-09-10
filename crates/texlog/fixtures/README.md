# texlog fixtures

Each directory holds a `main.log` captured from a real engine run, the `main.tex` that produced
it, and (from S5.3) an `expected.json` with the diagnostics the parser must produce. The parser
is the most fragile code in the project (DESIGN.md §8), so every rule ships with one of these.

Hand-written log excerpts test the matcher against *our idea of* TeX. These test it against TeX.
Three of the five S2.6 rules were wrong until the captures below existed — see the notes column.

| Fixture | Engine | What it exercises |
|---|---|---|
| `broken-underscore` | Tectonic 0.17.0 (XeTeX) | `! Missing $ inserted.` with an `l.5` marker whose context line is wrapped at 79 columns and truncated with a leading `...`. Source is `fixtures/broken/main.tex`; Tectonic halts at the first error, so the other two mistakes there do not appear. |
| `undefined-control-sequence` | Tectonic 0.17.0 (XeTeX) | `! Undefined control sequence.` with the offending `\textbold` at the end of the `l.4` context line. |
| `unbalanced-braces` | Tectonic 0.17.0 (XeTeX) | An unclosed `\emph{`. **`Runaway argument?` is printed with no leading `!`**, so it never reaches a scanner that keys on `!`; the real message is `! File ended while scanning use of \emph .` and it carries **no `l.NN` marker at all**. |
| `missing-package` | Tectonic 0.17.0 (XeTeX) | A `\usepackage` of something that does not exist. **Tectonic doubles the bang**: `! ! LaTeX Error: File \`nosuchpackage.sty' not found..` The `l.3` marker points at `\begin{document}`, not at the `\usepackage` on line 2. |
| `undefined-reference` | Tectonic 0.17.0 (XeTeX) | A `\ref` and a `\cite` with nothing behind them. Both are **warnings, not `!` errors**, and carry their line number in prose (`on input line 4`) rather than as a marker. The same log also holds `There were undefined references.` and an `Empty \`thebibliography' environment` warning, which must *not* be reported as separate problems. |

Capture a new one with the engine flags Preamble itself uses:

```
tectonic --outdir .preamble/build --keep-logs --keep-intermediates --synctex --chatter minimal main.tex
```
