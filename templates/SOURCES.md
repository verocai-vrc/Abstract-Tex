# Where each starter template came from

One row per folder under `templates/`. The catalog test (`abstract-tex-templates`) checks each
template's own `template.toml` for a licence on the allowed list; this file is the human-readable
record of the same facts, plus what was changed when a template was adapted from elsewhere.
Rules: `0.1/DESIGN.md` §10 "Templates".

| Template | Source | Licence | Notes |
|---|---|---|---|
| `blank` | Written for Abstract-Tex | CC0-1.0 | Standard `article` class, no packages. |
| `essay` | Written for Abstract-Tex | CC0-1.0 | Standard `article` class, no packages. |
| `report` | Written for Abstract-Tex | CC0-1.0 | Standard `report` class, no packages. |
| `letter` | Written for Abstract-Tex | CC0-1.0 | Standard `letter` class, no packages. |
| `paper` | Written for Abstract-Tex | CC0-1.0 | `article` and BibTeX `plain` style, no packages. `references.bib` cites two published books (Knuth, *The TeXbook*; Lamport, *LaTeX: A Document Preparation System*) as bibliographic facts. |

Previews are rendered from a real build by `node scripts/template-previews.mjs`.
