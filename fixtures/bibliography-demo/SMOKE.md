# Manual smoke script — `fixtures/bibliography-demo`

The v0.4 exit demo (`DESIGN.md` §7, `0.1/SPRINTS.md` S8.4): assemble a forty-reference paper
from scratch **without opening a browser once**. `main.tex` starts with no references and
`references.bib` with none. Bring the reference list of a real paper you know (a thesis
chapter, a recent article) — its identifiers are the input; nothing here is a canned list.

Launch:

```
ABSTRACT_TEX_OPEN=fixtures/bibliography-demo pnpm tauri dev
```

Keep a tally as you go: references added by paste, references added through Zotero, and
**anything that went wrong** — every one becomes a ledger entry in `bugs-issues-fixes.md` and,
if it needs code, a loop S8.9 onward. Do not fix anything mid-demo.

## §1 Paste-to-cite, about twenty-five references (S7.4–S7.6)

1. In `main.tex`, where a citation belongs, paste an identifier in any of the forms a reference
   list or a PDF gives you: a DOI (`10.1038/nature14539`, `doi:10.…`, `https://doi.org/10.…`),
   an arXiv id (`1706.03762`, `arXiv:1706.03762v5`), or an ISBN (`978-0-262-03384-8`).
2. The pasted text is replaced by `\cite{key}`, and a new entry appears at the end of
   `references.bib` — one field per line, and **no other line of the file changed** (check with
   `git diff fixtures/bibliography-demo/references.bib` now and then).
3. Paste the **same** DOI a second time, in a different form (bare, then as a URL): a second
   `\cite` with the same key, and still one entry in the `.bib`.
4. Mix all three kinds. Note any identifier that failed and the sentence the app showed for it —
   a failure with a clear sentence is a pass for this step; a raw error or silence is not.

## §2 A linked Zotero collection, about fifteen references (S8.1, S8.2, S8.7)

Needs Zotero with Better BibTeX installed and a collection holding the rest of the references.

1. Status bar: **Detect Zotero** → **Link Zotero collection**. Pick the collection.
2. Within a few seconds `zotero/<collection>.bib` appears in the file tree. If it does not, the
   bibliography count in the status bar shows an issue: open it, and the finding names the file
   and Better BibTeX (S8.6). Its **Unlink** button must work with Zotero closed (S8.7).
3. BibTeX reads only files named in the document, so once you cite from the collection the
   bibliography panel shows an error: the export is linked but not named (S8.8). Its button,
   **Add zotero/<collection> to \bibliography**, edits `main.tex`'s last line; the error and
   the undefined citations behind it both clear. Record whether the sentence alone told you
   what was wrong.
4. Cite the collection's entries with `\cite{` completion: each suggestion shows author, year and
   title, never a bare key (S7.3).

## §3 Health checks clear (S8.3, S8.6)

1. Open the bibliography panel from the status bar. Work through every finding by clicking it —
   each lands on the `.tex` line or `.bib` entry it names.
2. Done when the panel says **No problems found**, or only `never-cited` warnings you chose to
   keep, and nothing is `undefined-citation`.

## §4 The PDF

1. Build. The reference list at the end of the PDF has forty entries (count them), and no
   citation in the text reads `[?]`.
2. The build's diagnostics drawer shows no undefined-citation warning either — the health panel
   and the engine agree.

## Recording the outcome

Write S8.4's outcome paragraph in `0.1/SPRINTS.md` from your tally: how many references came
each way, how long it took, every gap found (with its ledger entry), and whether a browser was
ever opened. Tick S8.4 `[x]` only if all forty compiled with zero undefined citations and the
answer to the last question is no.
