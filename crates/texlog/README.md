# texlog

Parses a TeX `.log` file into diagnostics a person can act on: which file, which line, a
plain-language explanation of what went wrong, and — when the correction cannot be wrong — a
fix. Text in, data out. The crate never reads a file, never runs an engine, and knows nothing
about any editor, so it can sit behind a GUI, an LSP, a CI check or a shell script alike.

```rust
let log = std::fs::read_to_string("main.log")?;
for d in texlog::diagnostics(&log) {
    println!(
        "{}:{} [{:?}] {} — {}",
        d.file.as_deref().unwrap_or("?"),
        d.line.map_or("?".to_string(), |l| l.to_string()),
        d.severity,
        d.title,
        d.explanation,
    );
    if let Some(fix) = d.fix {
        println!("  fix: {} (replace `{}` with `{}`)", fix.description, fix.find, fix.replace);
    }
}
```

Each `Diagnostic` carries the rule that recognised it (`rule: Some("missing-dollar")`) or
`rule: None` when nothing in the catalog matched — the message still appears, in TeX's own
words, because an unexplained error the author can see beats a silent one. `raw_message` is
always TeX's original text, for a "show me the log" affordance.

## What it does

Three layers, each usable on its own:

1. **`tokenizer`** undoes the transcript writer's 79-column hard wrap (the one that splits
   file names and messages mid-word) and classifies every logical line: `! error`, `l.NN`
   source context, `Warning:` banner, or plain text — with the position of every `(` and `)`.
2. **`resolver`** walks those parens as a stack to answer "which file was open when this line
   was printed", which is the whole problem in resolving a message to a real `file:line`. The
   rule it uses, and the real captures that overturned every simpler one, are in its module doc.
3. **`rules`** is the catalog: 36 rules covering the mistakes people actually make — undefined
   control sequences, maths outside `$`, unbalanced braces, misplaced `&`, missing packages and
   images, undefined references and citations, over/underfull boxes, `\verb` at end of line,
   `\include` nesting, and package errors from `babel`, `hyperref`, `tikz`, `xcolor`, `fontspec`
   and `amsmath`. Eight of them offer a `Fix`: a literal `find`/`replace` pair
   on the reported line, never a byte offset, since the crate has no source text to offset into.

## What it does not do

- **It never guesses.** An unmatched error is reported with `rule: None`, not force-fitted.
- **It does not read `.tex` files.** A diagnostic inside a bare `\input{chapter}` (no extension)
  resolves to the *parent* file, because TeX echoes `(chapter` and a text-only parser cannot
  tell that from a package's incidental `(rerunfilecheck)`. A caller with the real file tree
  can complete the name; the resolver's doc says exactly where the limit is.
- **It does not run `biber`,** so `biblatex`'s own errors have no rule yet.
- **It is tested against one engine so far:** Tectonic 0.17.0 (XeTeX). The wrap width and the
  shapes it recognises are the classic ones shared by pdfTeX and LuaTeX, but every fixture in
  `fixtures/` is a Tectonic capture. Logs from other engines that parse wrongly are exactly the
  bug reports this crate wants.

## Fixtures are the tests

Every rule ships with a real log captured from a real engine run, under `fixtures/<rule>/`,
with the `main.tex` that produced it and an `expected.json` of what the parser must return.
`build.rs` turns each such directory into a `#[test]` at build time, so adding a fixture adds a
test with no Rust edited. Hand-written log excerpts test the parser against *our idea of* TeX;
these test it against TeX — three of the first five rules were wrong until their captures
existed. `fixtures/README.md` records what each capture taught.

## Licence

MIT. Extracted from [Abstract-Tex](https://github.com/verocai-vrc/Abstract-Tex), a local-first LaTeX
editor whose application code is AGPL; this crate is published separately so the wider TeX
ecosystem can use the part most worth sharing.
