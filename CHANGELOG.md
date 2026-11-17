# Changelog

Versions name the last milestone whose features are all in (see [README → Versions](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#versions)).

## v0.8.0 — 16 November 2026

The first tagged release. **Source only and pre-release:** there are no installers yet, so build
it as described in the README's [Quick start](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#quick-start). It contains milestones v0.1
to v0.8, the eight releases planned before installers, with the bugs from seven review passes
fixed.

### What is in it

- **It compiles (v0.1).** A bundled Tectonic engine builds the PDF beside the editor after 700 ms
  without typing, with no TeX installed. An edit cancels a build in progress.
- **It navigates (v0.2).** TexLab completion, hover and go-to-definition; SyncTeX in both
  directions; multi-file projects with the root document detected; a document map, command palette
  and quick open.
- **It explains itself (v0.3).** Every error and warning is traced to its file and line and
  explained in plain language, with one-click fixes where the fix is unambiguous. The raw log is one
  click away, never the default.
- **It cites (v0.4).** `\cite` completion with author, year and title; paste a DOI, arXiv ID or ISBN
  to get an entry; Zotero with Better BibTeX; continuous bibliography checks.
- **It's fast (v0.5).** Warm rebuilds run a single TeX pass. A project can build with a system
  `pdflatex`, `xelatex` or `lualatex` instead of the bundled engine.
- **It syncs (v0.6).** A snapshot after every successful build, a Source Control view, one-button
  Sync with any Git remote, GitHub sign-in and clone, conflicts shown as two paragraphs side by
  side, `latexdiff` comparisons between versions, and starter templates.
- **It assists (v0.7).** An optional assistant, off by default, using your own Anthropic or
  OpenAI-compatible key. Suggestions arrive as diffs you accept hunk by hunk, and a guard keeps
  any citation key that is not in your bibliography out of the document.
- **It shares, live (v0.8).** Real-time co-editing through `abstract-tex-relay`, a relay you host
  yourself that stores nothing and cannot read the session. Offline edits merge on reconnect, and
  comments are anchored to quoted text.
- **From v0.9, already in:** local crash reports that you submit yourself, a first accessibility
  pass, and contributor documentation and issue templates.

### What it does not do yet

- **The v0.5 speed target is not met** on the bundled engine: a warm rebuild of the sixty-page
  test thesis takes about 3.4 s, against a target of 1.2 s. The preamble alone takes about
  1.9 s, so no change to how the app schedules builds can reach the target. Conference-length
  papers rebuild in about 0.7 s.
- **The v0.6 two-machine demo has not been performed.** That is: write, sync, clone on a second
  machine, carry on. Each step is tested against a real Git remote, but not the whole sequence
  across two computers.
- No installers, no auto-update and no code signing. That is v0.9's work.
- The remaining known limitations (Biber, shell escape, `fontspec` font names, GitHub sign-in on a
  source build, the screen reader) are listed in the README under
  [Known limitations](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#known-limitations).

### Bundled components

Tectonic 0.17.0 and TexLab 5.26.0, fetched by `pnpm fetch-sidecars`.
