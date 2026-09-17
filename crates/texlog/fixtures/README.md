# texlog fixtures

Each directory holds a `main.log` captured from a real engine run, the `main.tex` that produced
it, and — for the fixtures that exercise the rule catalog — an `expected.json` with the
diagnostics [`rules::diagnostics`] must produce for that log. The parser is the most fragile code
in the project (DESIGN.md §8), so every rule ships with one of these.

**A directory with both `main.log` and `expected.json` gets a test for free.** `build.rs` (S5.3)
scans this folder at build time and generates one `#[test]` per such directory into
`tests/fixtures.rs`; adding a fixture is enough to add its test; nothing in `src/` or `tests/`
needs editing. A fixture used only by `tokenizer.rs` or `resolver.rs` (below the rule catalog, and
already covered by their own hand-written tests) carries no `expected.json` and is skipped by the
generator, not treated as an error.

Write a new `expected.json` by first dropping in an empty `[]`: the generated test's failure
message pretty-prints the diagnostics the parser actually produced, in the exact JSON shape
`expected.json` needs, ready to paste in as the real file.

Hand-written log excerpts test the matcher against *our idea of* TeX. These test it against TeX.
Three of the five S2.6 rules were wrong until the captures below existed — see the notes column.

**Tectonic halts at the first `!` error, always.** Unlike a classic engine's batch mode, which
skips an error and keeps going so a log can hold several, this project's bundled Tectonic 0.17.0
stops the run the moment one `!` error occurs — confirmed across every fixture below that
contains one: not a single one has a second. A log can still hold several *warnings* before that
point (`torture` has two), and warnings alone never halt a run at all (`undefined-reference`,
`duplicate-label`), but "many `!` errors in one real log" is not a shape this engine ever
produces. S6.4's twenty-error torture document will need twenty separate compiles, not one.

| Fixture | Engine | What it exercises |
|---|---|---|
| `broken-underscore` | Tectonic 0.17.0 (XeTeX) | `! Missing $ inserted.` with an `l.5` marker whose context line is wrapped at 79 columns and truncated with a leading `...`. Source is `fixtures/broken/main.tex`; Tectonic halts at the first error, so the other two mistakes there do not appear. |
| `undefined-control-sequence` | Tectonic 0.17.0 (XeTeX) | `! Undefined control sequence.` with the offending `\textbold` at the end of the `l.4` context line. |
| `unbalanced-braces` | Tectonic 0.17.0 (XeTeX) | An unclosed `\emph{`. **`Runaway argument?` is printed with no leading `!`**, so it never reaches a scanner that keys on `!`; the real message is `! File ended while scanning use of \emph .` and it carries **no `l.NN` marker at all**. |
| `missing-package` | Tectonic 0.17.0 (XeTeX) | A `\usepackage` of something that does not exist. **Tectonic doubles the bang**: `! ! LaTeX Error: File \`nosuchpackage.sty' not found..` The `l.3` marker points at `\begin{document}`, not at the `\usepackage` on line 2. |
| `undefined-reference` | Tectonic 0.17.0 (XeTeX) | A `\ref` and a `\cite` with nothing behind them. Both are **warnings, not `!` errors**, and carry their line number in prose (`on input line 4`) rather than as a marker. The same log also holds `There were undefined references.` and an `Empty \`thebibliography' environment` warning, which must *not* be reported as separate problems. |
| `wrapped-file-open` | Tectonic 0.17.0 (XeTeX) | Used by `tokenizer.rs` (S5.1), not by the rule catalog. An `\input` of a file under a deliberately long directory name so the `(` line TeX prints when opening it exceeds 79 columns and hard-wraps mid-word: `...the-open-paren-li` / `ne/chapter` — confirms the exact wrap width this project's bundled engine uses and that a wrapped file-open path survives unwrapping intact. |
| `space-in-path` | Tectonic 0.17.0 (XeTeX) | Used by `resolver.rs` (S5.2). An `\input` of a file under a directory name with a space in it (this project's own working directory has one). TeX does not quote filenames, so the printed `(sub dir with spaces/chapter one)` has no delimiter marking where the path ends — and, separately, no `.tex` extension either (see `bare-input-no-extension`), so the resolver's only signal here is the `/`. |
| `bare-input-no-extension` | Tectonic 0.17.0 (XeTeX) | Used by `resolver.rs` (S5.2). An `\input{plainchapter}` with no extension given at the call site: Tectonic echoes it as `(plainchapter)`, lexically identical to an incidental parenthetical (`(HO)`, `(rerunfilecheck)`). Pins a real, irreducible limitation rather than a bug: text alone cannot tell these apart. |
| `overfull-hbox` | Tectonic 0.17.0 (XeTeX) | Used by `resolver.rs` (S5.2). A deliberately unbreakable word forces `Overfull \hbox (48.75pt too wide) in paragraph at lines 3--4` — a very common line whose own parens are not a file boundary and must not perturb the stack. |
| `too-many-closing-braces` | Tectonic 0.17.0 (XeTeX) | A stray `}` in running text: `! Too many }'s.` — the real-engine capture the existing `unbalanced-braces` rule's "too many" direction only had a hand-written log for before now. |
| `missing-closing-brace` | Tectonic 0.17.0 (XeTeX) | `$x^{2$` — the closing brace never comes before maths mode ends, so TeX inserts one and reports `! Missing } inserted.`, with an `l.NN` marker (unlike `unbalanced-braces`' own capture, where the equivalent message carries none). The message has no `\command` in it, so `explain_unbalanced_braces` falls to its generic "a brace was never closed" wording rather than naming a command — a real log exercising that fallback branch for the first time. |
| `misplaced-alignment-tab` | Tectonic 0.17.0 (XeTeX) | A bare `&` outside any tabular/align environment: `! Misplaced alignment tab character &.` — a DESIGN.md §5.2 category with no rule until S6.1's `misplaced-alignment-tab` rule closed it. |
| `emergency-stop` | Tectonic 0.17.0 (XeTeX) | A document with no `\end{document}` at all: `! Emergency stop` / `*** (job aborted, no legal \end found)`. No `l.NN` marker, and every file TeX had open is cleanly closed before this line prints — the one captured error with nothing on the resolver's stack at all when it fires. |
| `font-substitution-warning` | Tectonic 0.17.0 (XeTeX) | `\fontfamily{nonexistentfont}`: `LaTeX Font Warning: Font shape ... undefined`, followed by `... using ... instead`. Neither line starts with `Reference ` or `Citation `, so — like `duplicate-label` below — this must produce **no** diagnostic; a different real trigger for the same boundary. |
| `tikz-unknown-key` | Tectonic 0.17.0 (XeTeX) | An unrecognised `tikz`/`pgfkeys` option: `! Package pgfkeys Error: I do not know the key '/tikz/nosuchoption' and I am go` / `ing to ignore it...` — genuinely wraps mid-word at 79 columns the way DESIGN.md §5.2 warns about, and `quick_errors` (unlike `tokenizer.rs`) does not unwrap, so today's real `raw_message` is the truncated half-word. Lands in the long tail, same as `misplaced-alignment-tab`. |
| `undefined-environment` | Tectonic 0.17.0 (XeTeX) | `\begin{nosuchenv}`: `! LaTeX Error: Environment nosuchenv undefined.` — a very common real-world mistake, closed by S6.1's `undefined-environment` rule. |
| `duplicate-label` | Tectonic 0.17.0 (XeTeX) | Two `\label{sec:intro}` calls: `LaTeX Warning: Label \`sec:intro' multiply defined.` "Multiply defined" is not "undefined", so this must produce **no** diagnostic — a real capture of the boundary `other_latex_warnings_are_not_swept_up` only tested by hand before now. |
| `missing-included-chapter` | Tectonic 0.17.0 (XeTeX) | `\input{chapters/intro}` where `chapters/intro.tex` does not exist: `! ! LaTeX Error: File \`chapters/intro.tex' not found..` (doubled bang, like `missing-package`) but with a correct `l.3` marker this time, and the *non*-package branch of `explain_file_not_found` — `rule_4_treats_a_missing_chapter_differently_from_a_missing_package` only had a hand-written log for this before now. |
| `nested-include` | Tectonic 0.17.0 (XeTeX) | Used by `resolver.rs` (S5.4), not the rule catalog. `main.tex` → `\input{outer}` (no extension given, so — like `bare-input-no-extension` — not recognised as a file) → `\input{chapters/inner}` (a `/`, so recognised despite also having no extension) → an undefined control sequence, three files deep. The resolver's file stack correctly holds `main.tex` under `chapters/inner`, skipping straight past the unrecognised `outer` in between — proof that an unrecognised wrapper in the middle does not break resolution of the real file nested inside it. |
| `torture` | Tectonic 0.17.0 (XeTeX) | The nearest thing to a multi-error capture this engine allows (see the halting note above): an undefined citation and an undefined reference — both warnings, so both survive — followed by an undefined control sequence that halts the run. Three diagnostics from one real log, more than any fixture before it; `diagnostics()` groups them by *scan* (the `!` error first, both warnings after), not by the order they appeared in the document, which this fixture is the first real capture to pin rather than assume. |
| `extra-alignment-tab` | Tectonic 0.17.0 (XeTeX) | A `tabular{cc}` row with three `&`-separated entries instead of two: `! Extra alignment tab has been changed to \cr.` — the "too many columns" direction, distinct from `misplaced-alignment-tab`'s "no table at all". S6.1. |
| `mismatched-environment` | Tectonic 0.17.0 (XeTeX) | `\begin{itemize}` closed by `\end{enumerate}`: `! LaTeX Error: \begin{itemize} on input line 3 ended by \end{enumerate}.` — the message carries *both* the opening line (in prose) and the closing line (as `l.NN`), so this rule reads both rather than just the `l.NN` claim like every other rule. S6.1. |
| `missing-begin-document` | Tectonic 0.17.0 (XeTeX) | Text before `\begin{document}`: `! LaTeX Error: Missing \begin{document}.` S6.1. |
| `illegal-unit-of-measure` | Tectonic 0.17.0 (XeTeX) | `\vspace{1}`, a number with no unit: `! Illegal unit of measure (pt inserted).` S6.1. |
| `missing-number-treated-as-zero` | Tectonic 0.17.0 (XeTeX) | `\vspace{}`, an empty argument where a number was expected: `! Missing number, treated as zero.` S6.1. |
| `double-subscript` | Tectonic 0.17.0 (XeTeX) | `$x_1_2$`: `! Double subscript.` S6.1. |
| `double-superscript` | Tectonic 0.17.0 (XeTeX) | `$x^1^2$`: `! Double superscript.` — the same rule function as `double-subscript`, told apart by which message matched. S6.1. |

Capture a new one with the engine flags Preamble itself uses:

```
tectonic --outdir .preamble/build --keep-logs --keep-intermediates --synctex --chatter minimal main.tex
```
