# texbib

Parses a BibTeX or BibLaTeX `.bib` file into the items it is made of — entries, `@string`
macros, `@preamble`, `@comment`, and the free text between them — with a byte span on every
one. Text in, data out: the parser never reads a file, never makes a request, and knows
nothing about any editor, so it fits behind a GUI, a script or a CI check alike. Network access
exists only behind the opt-in `acquire` feature, described below.

It is a parser and a few tools built on its output. It is not a bibliography manager: it keeps
no database, does not deduplicate a library, and does not format citations — that is BibTeX's,
Biber's or CSL's job.

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

## Built on the parser

- `render_entry` / `append_entry` write an entry back as text, one aligned field per line the way
  Better BibTeX does, and append it after a file's last item — the rest of the file is kept byte
  for byte, which the round trip above is what guarantees.
- `unique_key` makes a `surnameYEARfirstword` key, ASCII-folded, with `a`, `b`, … appended when it
  collides with a key already in use.
- `health::check` reports two problems a single `.bib` file can show on its own: a required field
  missing for the entry's type (`@article` without `journal`), and a page range written with a
  hyphen (`12-15`) where BibTeX wants an en-dash (`12--15`). Each finding is a sentence, a
  severity, the entry's key and a span (the field's, or the whole entry's). Nothing is fixed automatically.

## Optional: fetching entries (`acquire` feature)

```toml
texbib = { version = "0.1", features = ["acquire"] }
```

Adds `texbib::acquire`, which uses blocking `reqwest` and only the identifier's own public
endpoint — no API key, no intermediary service:

- `identify` says whether a pasted string is a DOI, an arXiv id or an ISBN, or none of them.
- `doi::fetch_doi` asks `https://doi.org/<doi>` for `application/x-bibtex` and parses the reply
  with this crate, so the entry is exactly what the publisher registered.
- `arxiv::fetch_arxiv` (the arXiv Atom API) and `isbn::fetch_isbn` (OpenLibrary) build a BibLaTeX
  entry from replies that are not BibTeX.
- `zotero` talks to Better BibTeX's JSON-RPC endpoint on a local Zotero (`127.0.0.1:23119`):
  `detect` says whether Zotero and Better BibTeX are running, `list_libraries` reads the collection
  tree, and `add_autoexport` asks Better BibTeX to keep a collection exported to a `.bib` path.
  That last call is the only write, and it configures an export; it never edits the library.

Every call has a typed error that reads as a sentence. The tests use recorded replies, so
`cargo test --features acquire` needs no network; the live checks are `#[ignore]`d.

## Fixtures are the tests

`fixtures/<name>/main.bib` holds a file in the shape a real tool writes — Better BibTeX,
JabRef, doi.org's content negotiation, a hand-typed file, a deliberately broken one — with an
`expected.json` of the items the parser must produce. `build.rs` turns each directory into a
`#[test]`, and every fixture is also checked for the byte-for-byte round trip above.

## Licence

MIT. Extracted from [Preamble](https://github.com/verocai-vrc/preamble), a local-first LaTeX
editor whose application code is AGPL; this crate and its sibling `texlog` are published
separately so the wider TeX ecosystem can use them.
