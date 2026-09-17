# texbib

Parses a BibTeX or BibLaTeX `.bib` file into the items it is made of — entries, `@string`
macros, `@preamble`, `@comment`, and the free text between them — with a byte span on every
one. Text in, data out: it never reads a file, never makes a request, and knows nothing about
any editor, so it fits behind a GUI, a script or a CI check alike.

```rust
let source = std::fs::read_to_string("references.bib")?;
let bib = texbib::parse(&source);

for entry in bib.entries() {
    let title = entry.field("title").map(|f| bib.resolve(&f.value)).unwrap_or_default();
    println!("{} ({}): {}", entry.key, entry.entry_type, title);
}
for error in bib.errors() {
    println!("could not parse {:?}: {}", error.span.text(&source), error.message);
}
```

## What it keeps

The `.bib` file is the author's, often shared with Zotero, JabRef or a co-author, so the
parser keeps what it finds instead of normalising it:

- **Comments and free text** between items are items too. Every item's span is recorded, the
  spans are contiguous, and concatenating them reproduces the input byte for byte — the
  property that lets a tool append one entry, or change one field, and touch nothing else.
- **Values as written.** `{braced}`, `"quoted"`, a bare number, a macro name, or several
  joined with `#` (`journal = ieee # " Trans."`). `Bibliography::resolve` substitutes the
  file's own `@string`s and BibTeX's twelve month macros on request; nothing is rewritten.
- **Names as written.** Entry types, field names and macro names compare ignoring case
  (`@Article` is `@article`) but are stored as typed. Citation keys are stored exactly;
  BibTeX compares them ignoring case and Biber does not, and only the caller knows which.
- **Duplicate fields.** BibTeX takes the first, Biber the last; a health check wants to see
  both, so both are kept.

## What it does on bad input

`parse` never fails. A malformed item — a value that is never closed, a missing key, a field
with no `=` — becomes a `ParseError` item with a one-sentence message and the byte offset it
was seen at. Parsing resumes at the next `@` that starts a line, so a broken entry costs
exactly that entry and the file's other entries are all still there.

## BibLaTeX

Same syntax, more entry types (`@online`, `@set`, `@xdata`) and field names (`date`,
`journaltitle`); it all parses. What BibLaTeX *means* by `crossref`, `xdata` and `ids` —
inheritance between entries — is not resolved here; the fields come through as written for
whoever builds an index on top.

## Fixtures are the tests

`fixtures/<name>/main.bib` holds a file in the shape a real tool writes — Better BibTeX,
JabRef, doi.org's content negotiation, a hand-typed file, a deliberately broken one — with an
`expected.json` of the items the parser must produce. `build.rs` turns each directory into a
`#[test]`, and every fixture is also checked for the byte-for-byte round trip above.

## Licence

MIT. Extracted from [Preamble](https://github.com/verocai-vrc/preamble), a local-first LaTeX
editor whose application code is AGPL; this crate and its sibling `texlog` are published
separately so the wider TeX ecosystem can use them.
