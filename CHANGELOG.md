# Changelog

Versions name the last milestone whose features are all in (see [README → Versions](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#versions)),
with one exception, v0.9.0, explained in its entry.

## Unreleased

### A start screen

Written for someone who has only used Word. Nothing in the files or the engine changes.

- **Home** comes up when the app starts with no document: *Carry on* with the last one (Enter does it),
  *Start something new*, and the recent documents with a picture of their first page. One that has been
  moved or is on a drive that is not there says so and can be found again. `Ctrl+Shift+H` shows it over
  an open document, and Esc goes back.
- **New** asks for a title and not for a folder: documents go in `Documents/Abstract-Tex`, or where you
  say once. Your name is remembered.
- **Open** takes a `.tex`, a folder, a `.zip`, or a Git address. A chapter opens its thesis. Downloads
  and Desktop are never opened as if they were a document; a `.tex` among other things is copied out
  with the files it uses. An archive that would write outside its folder is refused. A Word file is
  told apart and explained.
- **Save PDF…** copies the newest PDF to a place you choose, then *Open it* or *Show in folder*.
- **Text and colours**: 100 to 200%, light, dark, and two with high contrast, kept for next time.
- **Help** answers the first eight questions.
- Where three panes do not fit, one is shown at a time, so 200% works in a 1366 × 768 window.
- The first PDF's wait says what is being fetched, and a first build with no internet says that rather
  than showing a failure of the engine.

### Also

- Opening a folder no longer stops the window, and can be stopped. Nothing is saved into the wrong
  folder while it happens.
- On screen it is a *document*. The assistant, Git and live-session panes still say *project*.
- *New…*, *Open folder…* and *Clone…* are gone from the toolbar. The first two are pages of the start screen,
  with the shortcuts they had; *Clone…* is now a button on its Open page.
- A new document starts with no permission that was given to another folder at the same address.

Tried on Linux; not yet on Windows or macOS.

## v0.9.0 — 17 November 2026

**Source only and pre-release.** There are still no installers; build it as described in the
README's [Quick start](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#quick-start).
It is v0.8.0 plus the 28 commits of the eighth review pass, all of them fixes.

### The exception to the version rule

v0.9 was planned as "It ships": signed installers with auto-update, a companion GitHub Action, and a
stranger installing the app on a clean machine. **None of those three is built.** They need a
code-signing decision, a second repository, and a person who is not the maintainer, and they move
to v1.0 together with the internationalisation scaffold. What v0.9.0 contains of v0.9 is the part
that needed none of those, and was already in v0.8.0: local crash reports that you submit yourself,
a first accessibility pass, and the contributor guide and issue templates. The number names that
milestone as cut, not as complete.

### What changed since v0.8.0

Fixes only; no new feature.

- **Your files stay yours.**
  - A file starting with a byte-order mark no longer lets the caret sit in front of it, which had
    broken the first build with "Missing \begin{document}".
  - Resolving a merge conflict by hand in a file with Windows line endings no longer inserts a blank
    line.
  - A link inside `.abstract-tex/` (carried by a cloned repository or brought by a sync) can no longer
    make the app delete, overwrite or create files outside the project. Three places were affected:
    the comparison folder, the draft preview and the build folder.
  - A comparison between two versions refuses a file name in an old commit that would write outside
    the export folder.
  - The manuscript sent to a model as context leaves out any file that is a link out of the project.
- **Opening and cloning.**
  - A project opens when the system's file-watch limit has run out, and says what it will miss.
  - A build of the project you just left no longer lands on the one you opened.
  - A clone cannot be named something Windows reserves or silently changes (`con`, `nul`, a trailing
    dot).
  - A new project whose title is a Windows device name (`Aux`) gets a folder Windows can keep, and
    a control character pasted into a title (Word's `U+000B`) no longer breaks the first build.
- **Citations and the bibliography.**
  - Paste-to-cite only acts in a LaTeX document, not inside a `.bib` file; it writes to a `.bib` that
    exists and says so when none does.
  - A paste is a DOI only if it is one word shaped like one, so "10. Conclusion" is no longer
    swallowed. A bare number is an ISBN only if its check digit adds up, and `1250.5` is no longer
    an arXiv id.
  - Duplicate-DOI findings are whole sentences, and DOIs are compared case-insensitively. A remote
    bibliography is called remote, not missing; `\cite{#1}` in a macro is not an undefined citation;
    BibLaTeX's field names count in the required-field check.
- **Messages.** A forward search that finds nothing says why. A package error's own line break no
  longer cuts its message short. The "not a text file" sentence no longer offers Latin-1.
- **The keychain.** The commands that read or write it no longer run on the window's thread, so a
  keychain prompt cannot freeze the window. Two changes to the assistant's settings made at once can
  no longer lose one.

### What it does not do yet

- **No installers, no auto-update, no code signing, no pull-request Action, and no stranger test.**
  These are the 1.0 work.
- **No internationalisation scaffold.** Also 1.0.
- **The v0.5 speed target is not met** (3.4 s against 1.2 s for the sixty-page test thesis; see
  v0.8.0 below).
- **The v0.6 two-machine demo has not been performed.**
- **Screen-reader support is unverified:** the accessibility pass was checked by reading the markup,
  not with a screen reader.
- The remaining known limitations are listed in the README under
  [Known limitations](https://github.com/verocai-vrc/Abstract-Tex/blob/main/README.md#known-limitations).

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
