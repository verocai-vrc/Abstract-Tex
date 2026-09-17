# texbib fixtures

Each directory holds a `main.bib` and an `expected.json` of the items `texbib::parse` must
produce for it. `build.rs` generates one `#[test]` per directory, and the harness
(`tests/fixtures.rs`) also checks that every fixture's item spans concatenate back to the
input byte for byte. Start a new fixture with an `expected.json` of `[]`: the failing test
prints the JSON the parser actually produced, ready to paste in once checked by eye.

**These are written by hand in the shape each tool produces, not captured from the tool.**
`texlog`'s fixtures are real engine captures because TeX's log format is undocumented and
surprised us three times out of five; `.bib` syntax is small and stable, and the shapes below
are the ones the tools are known to write. If a real export from one of them ever parses
differently, that export replaces the hand-written file here and the note says so.

| Fixture | Shape | What it exercises |
|---|---|---|
| `hand-typed` | A person at a keyboard | Free-text comment at the top; `@string`; `"quoted"`, `{braced}` and bare-number values; `#` concatenation; a built-in month macro; `@book(...)` with parentheses; a trailing comma; a space after the opening brace before the key. |
| `better-bibtex` | Zotero + Better BibTeX export | Leading blank line; BibLaTeX types and fields (`@online`, `date`, `journaltitle`, `urldate`, `langid`); doubled braces `{{DNA}}` for case protection; a multi-line abstract; a `file` field with spaces and a full path. |
| `jabref` | JabRef | `% Encoding: UTF-8` header; capitalised `@Article`; aligned `=`; every value braced, including `year = {2019}`; `@Comment{jabref-meta: ...}` items, one multi-line with escaped semicolons. |
| `doi-negotiation` | `https://doi.org/<doi>` with `Accept: application/x-bibtex` | The whole entry on one line with no trailing newline; upper-case field names (`ISSN`, `DOI`); `month=sep` unquoted; a Unicode en dash in `pages`. |
| `broken-middle` | A file with two mistakes in it | Entry two has a value whose `}` is on the next line, so the entry runs into the following `@`; entry four has no key. Both become `error` items with a sentence and a span, and entries one, three and five parse untouched. Also proves recovery skips the `@` in an `email` field. |
| `biblatex-inheritance` | BibLaTeX-only constructs | `@set` with `entryset`; `@xdata`; `xdata`, `crossref` and `ids` fields; a `date` range. All parse as plain entries and fields — the parser resolves none of it. |
| `strings-and-preamble` | A journal's own `.bib` conventions | `@preamble` with two concatenated parts; `@STRING`, `@String(...)` and `@string` in three spellings; a macro defined in terms of another; `jan` redefined by the file, which then wins over the built-in month. |
