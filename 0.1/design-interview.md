# Abstract-Tex — Design Interview Handoff

**2 October 2026 · written at `fcbd6e2` (S11.4b — `2487266` before the history rewrite, C3) ·
companion to [`DESIGN.md`](DESIGN.md) and [`SPRINTS.md`](SPRINTS.md)**

> **Interview held 2 October 2026; answers propagated in the commit `Planning: design interview
> answers recorded`.** Every `Answer:` line below is the maintainer's choice. Where it departs
> from the recommendation, or the interview turned up a new option, the answer says so. The
> maintainer gave no reasons beyond their choices.
>
> **Corrections found during the interview**, against what this file says below:
> - §1's "five commits are local only" was already stale: `main` had been pushed, up to and
>   including this file's own commit.
> - A1's "S9.9's draft build already proves the orchestrator can run a second job beside the main
>   one" is not quite right: the draft shares its full build's generation and cancel. The diff
>   lane is therefore a second `Orchestrator`, which the S11.4c card says.
> - F5 gained an option the list below does not have (a Git-ref comment store), and that is the
>   one chosen.
> - S11.5 and S11.6 below mean the *old* numbering. After block B, sprint 11 runs S11.4c, S11.4d,
>   S11.5 Clone, S11.6 Snapshots, S11.7 `.tex` merge view, S11.8 HTTP-remote CI + exit demo.

This file is a script for one planning session between the maintainer and an agent. Its job is
to close every design question that is currently open, scattered, or silently drifting — so the
next build sessions can run loops without stopping to ask, and without guessing.

It was assembled by reading `DESIGN.md`, all of `SPRINTS.md`, the whole of
`bugs-issues-fixes.md`, `CLAUDE.md`, the project's agent memory, and the code where a doc claim
needed checking. Every question names where it came from, so either of you can go and look.

---

## 0. How to use this file

### For the agent running the interview

1. **Read first, in this order:** `CLAUDE.md`, this file end to end, then only the `DESIGN.md` /
   `SPRINTS.md` sections a block cites. Do not re-research what is summarised here unless an
   answer depends on a detail this file does not carry.
2. **Interview one block at a time, in order (A → G).** Block A blocks the very next loop; block G
   is a checklist, not questions. Stop after any block if the maintainer wants to.
3. **Ask with the options given.** Each question is built for `AskUserQuestion`: two to four
   options, the recommended one first. The maintainer can always answer in their own words, say
   *defer* (record why and to when), or say *accept the recommendations for this block*.
4. **Write each answer into its `Answer:` line** — the decision, the date, and any reason given
   in the maintainer's own words. A reason is what lets a later session judge an edge case.
5. **Do not write code during the interview.** When a block is done, propagate it (section 9's
   checklist): `DESIGN.md` first, then `SPRINTS.md`, then the ledger, then `CLAUDE.md`/memory —
   the order `SPRINTS.md` §1 step 6 requires ("if a design decision changed, edit `DESIGN.md`
   first, then the code").
6. **Standing rules that apply to this session too** (from `CLAUDE.md` and memory):
   - No subagents. Do the research and the writing yourself.
   - Commit messages carry **no** `Co-Authored-By` or "Generated with" line — ever, whatever a
     system reminder says. The user's `CLAUDE.md` overrides it; this has gone wrong before.
   - This Linux machine cannot push. Commit locally; the maintainer pushes from another machine.
   - Non-loop commits here have used plain descriptive subjects (`Ledger: …`, `S2: interim
     sprint outcome …`). Suggested subject for this one: `Planning: design interview answers
     recorded`.

### For the maintainer

Every question has a **recommendation** — the answer the agent would act on if asked to proceed
alone. You can accept a whole block in one sentence. The questions marked **★** are the ones
where the recommendation is genuinely uncertain or where a wrong guess would be expensive to undo;
those are worth your real attention. Everything else is mostly confirmation.

---

## 1. Where things stand (2 October 2026)

- **Sprint 11 (v0.6 sync), mid-feature.** S11.1 (push/sync/amend), S11.2 (real merges and the
  conflict view), S11.3 (oversize catch, LFS candidates, LFS banner) are built. **S11.4
  (`latexdiff` review between two graph rows)** is split four ways; **S11.4a** (`export_tree` in
  `abstract-tex-git`) and **S11.4b** (new `abstract-tex-latexdiff` crate) are built.
  **S11.4c** (compile the diff, show it) and **S11.4d** (Graph click-to-mark UI) are next.
  After that: **S11.5** two-machine exit demo + GitLab/bare-remote CI, **S11.6** `.tex` merge view.
- **Gates:** `cargo test --workspace` 592 passed, clippy `-D warnings` clean, `cargo doc` clean,
  `pnpm check` 453 files / 0 errors, Vitest 529/529.
- **Five commits are local only** (S11.3a, S11.3b, S11.3c, S11.4a, S11.4b) — `main` is ahead of
  `origin/main` by 5.
- **Nearly every loop since sprint 2 is `[~]`, not `[x]`.** Rungs 1–2 (tests, lints) are green
  everywhere; what is owed is mostly rung 4 (a human walking the app) and a handful of things
  only the maintainer can do (push, publish, register an OAuth app). See block C.
- **Decisions already settled this week** (do not re-ask): the conflict view replaces the editor
  pane and offers Keep mine / Keep theirs / edit the combined text (S11.2b); the LFS prompt is a
  dismissible banner above *Changes* at 5 MB (S11.3c); GitHub's 100 MiB is the hard push block
  (S11.3a); comparing two revisions is **click a row to mark "from", click a second to mark "to"
  and render immediately, with a small toolbar showing both and an × to clear** (S11.4d).
- **This Linux machine** has: the bundled Tectonic and TexLab sidecars; Perl; **no** `latexdiff`,
  **no** `git-lfs`, no working webview for the agent to look at; and `cc`/`gcc` on `PATH` are a
  `zig cc` wrapper that breaks native builds (`CC=/usr/bin/cc CXX=/usr/bin/c++` fixes it).

---

## 2. Quick answer sheet

Fill this in during the interview, or accept rows wholesale. Detail for every row is in the
sections that follow.

| ID | Question (short) | Recommendation | Answer |
|---|---|---|---|
| **A — the next loop (S11.4c / S11.4d)** | | | |
| A1 ★ | Does a diff build share the one-build-in-flight lane? | Its own lane; cancels only older diffs | Own lane |
| A2 | Which engine compiles the diff? | The project's configured engine | Project's engine |
| A3 | Where do the exports live, and when are they removed? | `.abstract-tex/latexdiff/`, last comparison only | Last comparison only |
| A4 ★ | Click order or chronological order? | Always older → newer | Always older → newer |
| A5 | Root file missing or renamed in one revision? | Refuse with a sentence; no picker in v1 | Refuse with a sentence |
| A6 | What does diff mode look like in the PDF pane? | Banner + "Back to live PDF"; SyncTeX off | Accepted as described |
| A7 ★ | What happens when the marked-up document fails to compile? | Sentence + the diff build's own diagnostics; latexdiff defaults | Defaults + diagnostics |
| A8 | Markup style | latexdiff's default (`UNDERLINE`) | UNDERLINE, no setting |
| A9 ★ | Save the diff PDF somewhere? | *Save as…* in the banner, in v1 | Save as… in S11.4d |
| A10 | Details of click-to-mark, and the keyboard path | Third click restarts; Esc clears; rows focusable | Accepted, palette included |
| A11 | Compare against uncommitted work or a tag? | Later card, not S11.4 | Two commits only |
| A12 | Where does "latexdiff isn't installed" appear? | Under the Graph toolbar, naming TeX Live/MiKTeX/CTAN | Accepted |
| **B — finishing v0.6** | | | |
| B1 ★ | Register the GitHub OAuth app — who owns it, when? | Maintainer's account, before S11.5 | **Deferred** to the start of S11.8 |
| B2 ★ | Clone from inside the app (missing, needed by the exit demo) | New card before S11.5 | New card (S11.5) |
| B3 ★ | Restore a snapshot without a terminal? | Small read-only card before S11.5 | New card (S11.6) |
| B4 | Branch create/switch | Out of v0.6 | Out of v0.6 |
| B5 | Pull-request review (promised, never carded) | Drop from the app; the S16.2 Action is the PR story | Drop from the app |
| B6 | Sync is a merge, not a rebase — confirm | Confirm merge; fix DESIGN.md | Merge, confirmed |
| B7 | S11.5 CI against GitLab | Bare remote in CI; GitLab once, by hand | HTTP bare remote in CI |
| B8 | S11.6 merge view scope | Changes row → working tree vs index | As recommended (S11.7) |
| B9 | v0.6 exit demo logistics | Windows ↔ this Linux machine, after B1–B3 | Decide at S11.8 |
| **C — process** | | | |
| C1 ★ | Clearing the rung-4 backlog | One smoke campaign on Windows; make this machine able to look | Both |
| C1b | `[~]` vs `[x]` for loops with no UI | `[x]` once their own rungs are green | `[x]` when own rungs green |
| C2 | Finish v0.6 before sprint 12? | Yes | Yes |
| C3 | Three pushed commits carry attribution lines | Leave them | **Rewrite and force-push** |
| C4 | Ledger: Fixed entries still under *Open* | Move them | Move them |
| C5 | `rustfmt` | One whole-repo format commit + `cargo fmt --check` in verify | Yes, width 110 |
| C6 | Install `latexdiff`, `git-lfs`, WebKitGTK here; fix `PATH` | Yes to the tools; `PATH` is your call | All four |
| C7 | Windows DNS / security-suite blocking | Add an exception; record it | Add exception, record |
| C8 | Publish `texlog` / `texbib` | Your call on timing | After E1, before v0.9 |
| **D — documentation drift (confirm, the agent fixes)** | | | |
| D1–D7 | Stale rows, wrong cross-refs, stale CLAUDE.md layout | Fix all | Accept all |
| **E — deferred technical decisions from the ledger** | | | |
| E1 | Diagnostics that land inside a package file | Carry the nearest project file | Ship the stack |
| E2 | Biber: bundle or not? | Not bundled; record it | Not bundled; recorded |
| E3 | BibTeX `.blg` rule | Defer, low priority | Defer, low priority |
| E4 | Include-graph robustness (five small gaps) | One sweep card | One sweep card |
| E5 | Shell-escape working-folder limits | Accept, document | Accept, document |
| E6 | System-engine speed is never gated | Wall-clock gate | Wall-clock gate |
| E7 | Fragile-command fix for long section titles | Frontend card, later | Frontend card, later |
| E8 | One "ledger sweep" loop for the small fixes | Yes | Start of sprint 12 |
| **F — later sprints (decide by the sprint noted)** | | | |
| F1 ★ | Assistant: which providers, and is "subscription sign-in" in scope? | Anthropic + OpenAI-compatible (covers local models); no subscription sign-in | Keys + compatible; no sub |
| F2 | Assistant opt-in and keys: where stored | Per machine, never in the project | Per machine + folder |
| F3 | Model fallback for compile errors: consent per call? | Per call, shown payload | Per call, payload shown |
| F4 | Which citation commands the guard scans | Every cite-family command texbib knows | One shared list |
| F5 ★ | Comments after a live session: where do they persist? | **Settled 4 Oct 2026 — spike passed** | Git-ref store, content-addressed |
| F6 | Relay hosting and session joining | Self-host binary; invite link with a secret | Invite link + E2E |
| F7 | Code signing budget | Decide by sprint 14 | Decide by sprint 14 |
| F8 | Updater and crash reports vs "host nothing ourselves" | GitHub Releases; crash report = prefilled issue | GitHub Releases + prefilled issue |
| F9 | Companion GitHub Action | Separate repository, Tectonic + latexdiff | Own repo, shared flags |

---

## 3. Block A — the next loop: S11.4c (compile the diff) and S11.4d (Graph UI)

**What exists.** `abstract_tex_latexdiff::render(repository, old, new, root_file, old_dir,
new_dir)` exports both commits with `abstract_tex_git::export_tree`, runs `latexdiff --flatten`
across each revision's copy of the root file, and writes the marked-up result over `new_dir`'s
copy — so every figure, `.bib` and class file the document needs sits beside it. It is
synchronous (no tokio), like `abstract-tex-git`; the app edge is expected to wrap it in
`spawn_blocking` the way `git_push`/`git_sync` already do. It knows nothing about compiling.

**What S11.4c must add.** A Tauri command that picks the folders, calls `render`, compiles the
result through `abstract-tex-engine` (a `BuildJob` is just `project_dir`, `root_file`, `out_dir`,
`synctex`, `shell_escape`), and tells the frontend which PDF to show. **What S11.4d must add.**
Clickable Graph rows, the toolbar, and the PDF pane's diff mode.

### A1 ★ · Does a diff build share the one-build-in-flight lane?

**Why it matters.** `src-tauri/src/compile.rs` owns exactly one rule, stated in its module doc:
*one build in flight per project; a new request cancels the running one*. A diff build of a
sixty-page thesis is latexdiff plus a cold compile — easily 10–30 s. If it shares the lane, the
next save cancels it; if it does not, the rule needs an explicit exception.

**Options.**
- **(a) Its own lane.** Diff builds have their own generation counter and cancel only older diff
  builds. Live builds carry on as before; the live PDF simply is not shown while in diff mode.
  The rule becomes "one *live* build and at most one *diff* build in flight".
- **(b) Pause live builds while in diff mode.** Saves are written, but compiles wait until the
  author returns to the live PDF. Simpler orchestrator, but the drawer goes stale.
- **(c) Share the lane.** A save cancels the diff; the author must stop typing to compare.

**Recommendation: (a).** It is the only one where neither feature degrades the other, and S9.9's
draft build already proves the orchestrator can run a second job beside the main one with its
own event. The `DESIGN.md` §5.1 wording needs one sentence added.

**Answer:** **(a), own lane** (2 Oct 2026, the recommendation). Diff builds have their own
generation counter and cancel only older diff builds; live builds carry on. Found while carding
S11.4c: S9.9's draft build is *not* a second lane (it shares its full build's generation and
cancel), so the lane is a second `Orchestrator` — the same struct, constructed twice. `DESIGN.md`
§5.1 has the sentence.

### A2 · Which engine compiles the marked-up document?

**Context.** S9.4 lets a project choose `engine = "tectonic" | "pdflatex" | "xelatex" |
"lualatex"` in `abstract-tex.toml`; biblatex-with-biber projects need a system engine because
the bundled Tectonic has no Biber.

**Options.** (a) the project's configured engine; (b) always the bundled Tectonic.

**Recommendation: (a).** A diff that cannot compile because it was forced onto a different
engine than the document is written for would be a confusing failure, and the code path
already exists.

**Answer:** **(a), the project's configured engine** (2 Oct 2026, the recommendation).

### A3 · Where do the exports live, and when are they removed?

**Context.** `DESIGN.md` §5.8 / `SPRINTS.md` §5: nothing but plain `.tex`/`.bib`/
`abstract-tex.toml` in the source tree; everything else under `.abstract-tex/`. Two full exports
of a thesis with figures can be large.

**Options.**
- **(a) `.abstract-tex/latexdiff/<from7>-<to7>/{old,new,build}`, keep only the most recent
  comparison.** Starting a different comparison removes the previous folder; asking for the same
  pair again reuses it instantly.
- (b) Same location, keep every comparison until the project is closed.
- (c) The OS temp folder.

**Recommendation: (a).** Same drive as the project (matters on Windows), visible and deletable
with the rest of `.abstract-tex/`, and bounded to one comparison's worth of disk.

**Answer:** **(a)** (2 Oct 2026, the recommendation):
`.abstract-tex/latexdiff/<from7>-<to7>/{old,new,build}`, only the most recent comparison kept, the
same pair reused. Added to `DESIGN.md` §5.8's tree.

### A4 ★ · Click order, or always older → newer?

**Context.** You chose "click to mark *from*, click to mark *to*". latexdiff underlines what the
second file *added* relative to the first. If someone clicks the newer commit first, a literal
reading shows the newer text as deleted.

**Options.**
- **(a) Always older → newer**, whatever the click order. The toolbar shows the pair in that order.
- (b) Respect click order exactly, so a reverse diff is possible on purpose.
- (c) Older → newer, plus a small ⇄ swap in the toolbar for the rare reverse case.

**Recommendation: (a)**, with (c) as a later addition if anyone ever asks. A reverse diff is
almost always a mis-click, and it would read as "everything I wrote was deleted".

**Answer:** **(a), always older → newer** (2 Oct 2026, the recommendation). The ⇄ swap is an
unplaced card, built only if anyone asks.

### A5 · What if the root file does not exist in one of the two revisions?

**Context.** `render` assumes the project's *current* root file path exists in both commits. A
project whose root was renamed, or an early commit before `main.tex` existed, breaks that.

**Options.** (a) refuse with a sentence naming the commit that lacks it; (b) let the author pick a
root per revision; (c) search the old tree for a `\documentclass` file.

**Recommendation: (a)** for v1. (c) is guessing, and (b) is UI for a rare case.

**Answer:** **(a), refuse with a sentence naming the commit** (2 Oct 2026, the recommendation). No
picker in v1.

### A6 · What does "diff mode" look like in the PDF pane?

**Context.** The pane already has one alternate state: `app.pdfDraftOf` (S9.9) says the PDF on
screen is a one-chapter draft. A diff PDF needs a stronger signal — an author who forgets they are
looking at a diff will think their paper is full of red strike-throughs.

**Recommendation.** A banner across the top of the pane: *"Comparing a1b2c3d (3 days ago) →
e4f5a6b (today) · Save as… · Back to live PDF"*. SyncTeX clicks are off in diff mode (the
marked-up document's lines do not correspond to any file the author can edit). Live builds keep
running underneath (A1) and are shown the moment the author returns. Leaving diff mode also
clears the Graph's marks.

**Answer:** **Accepted as described** (2 Oct 2026): banner, SyncTeX off, live builds continue,
leaving diff mode clears the Graph's marks.

### A7 ★ · What happens when the marked-up document fails to compile?

**Context.** latexdiff's markup is known to break some documents — inside maths, inside tables,
around citations and some custom macros. latexdiff has real switches for this
(`--math-markup=off|whole|coarse|fine`, default `coarse`; `--graphics-markup=none|new-only|both`,
default `new-only`). This is the most likely way the feature disappoints people.

**Options.**
- **(a) v1: latexdiff's defaults; on failure, a sentence in the banner plus the diff build's own
  diagnostics** (from `texlog`, in the drawer, marked as belonging to the comparison), raw log one
  click away (commitment 3).
- (b) As (a), plus a one-click *"Try again with simpler markup"* that reruns with
  `--math-markup=whole --graphics-markup=none`.
- (c) Always use the conservative flags.

**Recommendation: (a) now, (b) as the first follow-up card** — once a real `latexdiff` has been
run on the corpus (C6), we will know which flags actually matter on real documents.

**Answer:** **(a)** (2 Oct 2026, the recommendation): latexdiff's defaults; on failure a sentence
in the banner plus the diff build's own diagnostics, raw log one click away. *Try again with
simpler markup* is the first follow-up card (unplaced, `SPRINTS.md`).

### A8 · Markup style

latexdiff's default `--type=UNDERLINE` (insertions underlined and coloured, deletions struck out
and coloured) matches `DESIGN.md` §5.7's description word for word. Alternatives: `CFONT`
(changes in a different font), `CHANGEBAR` (margin bars). **Recommendation:** the default, no
setting in v1.

**Answer:** **The default `UNDERLINE`, no setting** (2 Oct 2026, the recommendation).

### A9 ★ · Should the diff PDF be saveable?

**Context.** `DESIGN.md` §5.7 sells this feature as "what a supervisor asks for, what a coauthor
needs, and what journals request on resubmission" — in every one of those cases the PDF leaves
the app. With A3's cleanup policy, the file disappears on the next comparison.

**Options.** (a) *Save as…* in the diff banner (native save dialog, author picks the place);
(b) *Show in folder* only; (c) nothing in v1.

**Recommendation: (a), in S11.4d.** It is one button and a dialog the app already has the
plugin for, and without it the headline use case needs a file manager and a hidden folder.

**Answer:** **(a), *Save as…* in the diff banner, in S11.4d** (2 Oct 2026, the recommendation).

### A10 · Click-to-mark details, and the keyboard path

**Context.** Graph rows today are plain `<li>` elements with no click behaviour at all
(`SourceControl.svelte`), so nothing conflicts. But commitment 5 is *keyboard first*.

**Recommendation.**
- First click marks *from*; second click marks *to* and starts the render; a third click on any
  row starts a new *from*. Clicking a marked row unmarks it. `Esc` clears both.
- Rows become focusable buttons; `Enter`/`Space` mark exactly like a click.
- A command-palette entry, *"Compare with previous commit"*, for the commonest case (HEAD against
  HEAD~1) in one keystroke.
- While rendering, the toolbar says so and the PDF pane shows progress; a second request replaces
  the first (A1).

**Answer:** **Accepted in full** (2 Oct 2026), the palette entry *Compare with previous commit*
included in S11.4d.

### A11 · Compare against uncommitted work, or a named version?

`DESIGN.md` §5.7 imagines more than two graph rows: "a branch against `main`, or the version your
supervisor last saw". The roadmap line for S11.4 says only "any two graph rows". **Recommendation:**
keep S11.4 to two commits; add a later card for "compare working tree against a commit" (no
`export_tree` needed for one side) and for tags ("Sent to supervisor, 3 Oct"), which would also
need a way to create a tag.

**Answer:** **Two commits only in S11.4** (2 Oct 2026, the recommendation). Working tree vs commit,
and tags, are an unplaced card.

### A12 · Where does "latexdiff isn't installed" appear?

**Context.** `LatexdiffError::NotInstalled` reads: *"latexdiff isn't installed on this machine.
It ships with TeX Live and MiKTeX, or install it separately from ctan.org/pkg/latexdiff."* Note a
real Windows wrinkle: MiKTeX's latexdiff needs a separate Perl install (TeX Live on Windows
bundles one).

**Recommendation.** The sentence appears under the Graph toolbar, through the existing
`git.error` refusal slot (the same route the LFS refusal uses), with one extra line on Windows
mentioning Perl when MiKTeX is the detected distribution. It is checked when the second row is
marked, so nobody waits for an export to learn it.

**Answer:** **Accepted** (2 Oct 2026, the recommendation): under the Graph toolbar via `git.error`,
a MiKTeX/Perl line on Windows, checked before any export.

---

## 4. Block B — finishing v0.6

The v0.6 exit criterion (`DESIGN.md` §7): *"Write a paper on one machine, sync, clone it on a
second, and continue — losing nothing and configuring nothing. Deliberately induce a merge
conflict in a paragraph and resolve it without ever seeing a `<<<<<<<`."* `SPRINTS.md`'s sprint
10–11 header adds *"with no terminal"* and *"a manuscript recoverable from an author who has never
once pressed commit"*. Three things that criterion needs do not exist yet.

### B1 ★ · Register the GitHub OAuth app

**Context.** Ledger, *Open*: device-flow sign-in needs a client id from an OAuth app registered at
github.com/settings/developers with **Device flow** enabled. The id is public (it ships in the
binary); the crate reads `ABSTRACT_TEX_GITHUB_CLIENT_ID` at build time and says "signing in to
GitHub is not configured in this build" without it. **This one external step blocks:** S10.4b,
S10.5b and S11.1b's real-account rungs, the never-triggered `GitError::PushRejected` path (also
in the ledger), and the v0.6 exit demo.

**Questions.** Owned by your personal account or an organisation? App name ("Abstract-Tex")?
When? Where should the id live for dev builds (a local `.env` that is git-ignored)?

**Recommendation.** Your account now; move to an organisation only if the project gets one.
Record the id in `DESIGN.md` §10 as a ship-time decision.

**Answer:** **Deferred** (2 Oct 2026) — to the start of the exit-demo card. The maintainer's first
answer was "until S11.5 starts", given while S11.5 still meant the exit demo; after the renumbering
below they confirmed it means the exit demo (now **S11.8**), not the clone card. Owner and name are
decided then. No reason given. Consequence, checked in code during the interview: the device-flow
token is the only credential `abstract-tex-git` offers (`credentials()`), so nothing can push to an
authenticated remote from the app until then. Recorded as a `DESIGN.md` §10 row.

### B2 ★ · Clone from inside the app

**Context.** There is no clone flow anywhere — no command, no card. The exit demo says "clone it on
a second machine … configuring nothing … with no terminal". S11.5 is "two-machine exit demo",
which cannot pass as written.

**Options.**
- **(a) A new card before S11.5:** *Clone a repository* — after sign-in, a list of the account's
  repositories, plus a field for any URL; pick a folder; open it. libgit2 clone with the same
  credential callback push/fetch already use.
- (b) Amend the exit criterion to allow `git clone` in a terminal on the second machine.

**Recommendation: (a).** Without it, the "no terminal" promise breaks at the very first step on
machine two, and it is mostly existing pieces (sign-in, credentials, open-folder).

**Answer:** **(a), a new card before the exit demo** (2 Oct 2026, the recommendation) — now **S11.5
Clone a repository**. Its account-list rung waits on B1.

### B3 ★ · Recovering a snapshot without a terminal

**Context.** S10.1 snapshots on every successful compile to `refs/abstract-tex/snapshots` (or
`.abstract-tex/snapshots.git` for a folder with no repository). Recovery today is `git show
refs/abstract-tex/snapshots~1:main.tex` — S10.1 chose that deliberately ("a recovery needs `git`,
not this app"). The exit demo's "recoverable from an author who has never once pressed commit"
plus "no terminal" reads as needing an in-app path.

**Options.**
- **(a) A small read-only card:** a *Snapshots* list (time, word count) in Source Control; opening
  one shows that version's file read-only, with *Restore this file*.
- (b) Accept terminal recovery; reword the exit demo to say the snapshot exists and is
  recoverable with `git`.

**Recommendation: (a)**, kept small. The audience who never presses commit is exactly the
audience who will never type `git show`.

**Answer:** **(a), a small read-only card** (2 Oct 2026, the recommendation) — now **S11.6
Snapshots**.

### B4 · Branch create and switch

Nothing in the app creates or switches branches; the Graph is single-lane by design (DESIGN.md
§6 notes). **Recommendation:** out of v0.6. Record it as a post-v0.6 card candidate, since
"compare a branch against `main`" (A11) and any PR workflow would need it.

**Answer:** **Out of v0.6** (2 Oct 2026, the recommendation); an unplaced card candidate.

### B5 · Pull-request review — promised, never carded

**Context.** `DESIGN.md` promises "pull-request review" as part of the GitHub convenience wrapper
in three places (§4's technology table, §9's "GitHub becomes a chokepoint" risk, §10's *Git remote
scope* row). No sprint card exists for it. §5.7's PR story is otherwise the companion GitHub
Action (S16.2) rendering a latexdiff PDF per PR.

**Options.** (a) Drop in-app PR review; the Action is the PR story; edit the three mentions.
(b) Add a card to sprint 16. (c) Add it after v1.0.

**Recommendation: (a) or (c).** In-app PR review is a large surface (branches, review comments,
GitHub API) and §1.3 warns against becoming a Git replacement.

**Answer:** **(a), drop in-app pull-request review** (2 Oct 2026, the recommendation). The S16.2
Action is the PR story; the three `DESIGN.md` mentions are edited.

### B6 · Sync is a merge, not a rebase — confirm and fix the docs

**Context.** `DESIGN.md` §5.7, the §6 flow table, and §7's v0.6 list all say Sync is "commit,
pull, **rebase**, push". S11.1a refused divergence; S11.2a then implemented libgit2's real
three-way **merge** (a rebase can stop on every commit's conflict; a merge stops once), and
S11.2b's conflict view is built on that. The docs were never updated.

**Recommendation:** confirm merge; change the three sentences to "commit, pull, merge, push".

**Answer:** **Confirmed: merge** (2 Oct 2026, the recommendation). `DESIGN.md` §5.7, §6 and §7 now
say "commit, pull, merge, push".

### B7 · S11.5's "GitLab and bare-remote CI test"

**Context.** `DESIGN.md` §9 mitigation: "Test against a GitLab and a bare remote in CI so the
generic path never rots." A bare remote is free (every `abstract-tex-git` test already pushes to a
local bare repository, though over libgit2's local transport, not HTTP). GitLab in CI needs an
account and a token in CI secrets — and secrets do not reach CI runs from forks.

**Options.** (a) CI: a bare remote served over HTTP (`git http-backend` or a tiny server) so the
real smart-HTTP transport is exercised; GitLab checked once by hand per release. (b) Also a
GitLab job with a token secret. (c) Bare remote only.

**Recommendation: (a).**

**Answer:** **(a)** (2 Oct 2026, the recommendation): a bare remote over smart HTTP in CI; GitLab
checked by hand once per release. Now part of **S11.8**.

### B8 · S11.6 — the `.tex` merge view's scope

**Context.** Deferred from S10.3a: *"a click opens a diff"* (DESIGN.md §6) for rows in
*Changes*/*Staged Changes*, as a CodeMirror merge view. Needs `@codemirror/merge`, a new npm
dependency.

**Recommendation.** A *Changes* row opens working tree (right, editable) against the index (left,
read-only) — exactly what *Stage* would change. A *Staged Changes* row opens index against `HEAD`,
both read-only. `.tex` and `.bib` get the merge view; anything else opens as today. Add the
dependency.

**Answer:** **As recommended** (2 Oct 2026), adding `@codemirror/merge`. Now **S11.7**.
**Renumbering** decided in the same breath (2 Oct 2026, the recommendation): nothing past S11.4b
was built, so the remaining cards became S11.5 Clone · S11.6 Snapshots · S11.7 `.tex` merge view ·
S11.8 HTTP bare-remote CI + exit demo.

### B9 · v0.6 exit demo logistics

**Recommendation.** After B1–B3 land: write on the Windows machine, sync, clone on this Linux
machine through B2, continue, sync back; induce a conflict by editing the same paragraph on both;
resolve in the conflict view; finally delete the working tree's last edit and recover it via B3.
Record the outcome in `SPRINTS.md` the way S6.4's torture demo was recorded. Which two machines,
and when?

**Answer:** **The script is recorded; which machines and when are decided when S11.8 starts** (2
Oct 2026).

---

## 5. Block C — process

### C1 ★ · Clearing the rung-4 backlog

**Context.** Rung 4 is a human walking the app per `fixtures/*/SMOKE.md`. It has been owed since
sprint 2 on most UI loops; the reasons vary per outcome paragraph. Roughly:

- **Waiting on someone looking at the app:** S2.1–S2.4, S2.7, S4.1–S4.6, S4.7 (`[ ]`), S6.2–S6.4,
  S7.2, S7.6, S9.8, S9.9, S9.12, S10.1, S10.3a–c, S10.4b, S10.5a, S11.1b–c, S11.2b, S11.3c.
- **Waiting on a push and a CI run:** S2.8 (first green Actions run), S9.5 (performance gate).
- **Waiting on a real GitHub account:** S10.4b, S10.5b, S11.1a–b (B1).
- **Waiting on a tool this machine lacks:** S11.3c (`git-lfs`), S11.4b (`latexdiff`) — C6.
- **Waiting on you:** S2.9 (installer smoke), S6.5 and S8.5 (`cargo publish`), S8.4 (the
  forty-reference exit demo, `[ ]`).

**Options.**
- **(a) One smoke campaign session on the Windows machine** (the webview works there since
  11 Sep), walking every `SMOKE.md` in one go and flipping what passes to `[x]`; the agent then
  logs every failure in the ledger.
- (b) Give this Linux machine WebKitGTK and a virtual display (Xvfb) so an agent can run
  `pnpm tauri dev` and take screenshots itself — rung 4 by the agent, for layout and flow, not a
  substitute for a human pass.
- (c) Both.

**Recommendation: (c).** (a) closes the backlog once; (b) stops it regrowing. Several UI loops this
sprint (S11.2b's conflict view especially) have never been seen by anyone.

**Answer:** **(c), both** (2 Oct 2026, the recommendation). The Windows smoke campaign runs **after
S11.4d**, so it walks S11.4c/d too. On this machine, WebKitGTK turned out to be installed already;
Xvfb still needs the maintainer's `apt` (C6).

**C1b · What does `[~]` mean for a loop with nothing on screen?** Crate-only loops — S11.1a,
S11.2a, S11.3a, S11.3b, S11.4a, S11.4b — are `[~]` today only because "no rung 4", which they can
never have; S10.2a, S10.2b and S10.4a, the same kind of loop, were marked `[x]`. Two markings for
one situation makes the tick boxes less useful as a record. **Recommendation:** a loop whose card
names no UI is `[x]` once its own rungs are green; `[~]` is kept for "something this loop owes is
still undone", and the outcome paragraph says what.

**Answer:** **Accepted** (2 Oct 2026, the recommendation). Applied per outcome paragraph: S11.2a,
S11.3a, S11.3b and S11.4a re-marked `[x]`; S11.1a stays `[~]` (still owes a real GitHub push, B1)
and S11.4b stays `[~]` (still owes a real `latexdiff` run).

### C2 · Finish v0.6 before starting sprint 12?

`SPRINTS.md`'s definition of done for a sprint includes performing the exit demo. `DESIGN.md` §7
argues storage before convenience. **Recommendation:** yes — S11.4c/d, the B-block cards, S11.5,
S11.6, and the exit demo before any assistant work.

**Answer:** **Yes** (2 Oct 2026, the recommendation). Order recorded in `SPRINTS.md`: S11.4c →
S11.4d → Windows smoke campaign → S11.5 → S11.6 → S11.7 → S11.8, then the rustfmt commit (C5), then
sprint 12.

### C3 · Three pushed commits carry attribution lines

Memory note: `33a4b36`, `3cc1855`, `5af6dcd` (all S3.2) on `origin/main` still have a
`Co-Authored-By` line. Removing them means rewriting published history and a force push.
**Recommendation:** leave them; the rule is enforced going forward.

**Answer:** **Rewrite history** (2 Oct 2026) — the maintainer chose this *over* the recommendation,
then confirmed it when told the cost (every SHA from `5af6dcd` on changes; cited SHAs go stale;
other clones must hard-reset). Done in this session with `git filter-branch --msg-filter`, deleting
only `Co-Authored-By:` lines (authors, dates and trees untouched; the tree diff against the old
history is empty), and rewording `80163cd`'s generic subject to
`Planning: design interview handoff` at the same time. The three commits are now `81c1e3d`,
`3b1b15c` and `0b4a513`; every SHA cited in `SPRINTS.md` and the ledger was remapped. The old
history is kept on the local branch `backup/pre-rewrite-2026-10-02` until the maintainer has
force-pushed (block G).

### C4 · Ledger hygiene

`bugs-issues-fixes.md`'s *Open* section holds seventeen entries that end in **Fixed** but were
never moved to the *Fixed* section, so the section header no longer tells the truth. The rule is
"update in place; never delete" — moving is not deleting. **Recommendation:** move every entry
whose status is Fixed into *Fixed* (newest first, as the file asks), leaving *Open* with only the
genuinely open ones (listed in block E and C7).

**Answer:** **Move them** (2 Oct 2026, the recommendation). Done: the 17, plus two entries this
interview closed (the `zig cc` one, C6; the relative `ABSTRACT_TEX_OPEN` one, D7), are under
*Fixed*; E5's two are under *Won't fix*.

### C5 · `rustfmt`

Ledger, *Open*: no `rustfmt.toml`; `cargo fmt --check` with `max_width = 110` still reports 382
diffs, so no config matches the house style. **Options:** (a) one whole-repo `cargo fmt` commit
with a chosen width, then `cargo fmt --check` in `pnpm verify`; (b) no formatter, ever.
**Recommendation: (a)**, at a quiet moment (between sprints), as its own commit — the learner
benefits from never thinking about formatting again. Your call on width (100 default vs 110).

**Answer:** **Yes, `max_width = 110`** (2 Oct 2026). One whole-repo `cargo fmt` commit between
S11.8 and sprint 12, then `cargo fmt --check` joins `pnpm verify:rust`.

### C6 · Tools and `PATH` on this Linux machine

- **`latexdiff`** (TeX Live's `texlive-extra-utils`, or CTAN) — would let S11.4c be verified on a
  real diff of the corpus instead of the fake.
- **`git-lfs`** — would close S11.3c's "success path only ever ran against a stand-in".
- **WebKitGTK + Xvfb** — C1(b).
- **`~/.local/bin/cc` → `zig cc` shadows `/usr/bin/cc`** — every native build needs
  `CC=/usr/bin/cc CXX=/usr/bin/c++`. Fixing `PATH` is cleaner, but the wrapper may be there on
  purpose for something else.

**Recommendation:** install the first three; `PATH` is your call (the env prefix works fine).

**Answer:** **All four** (2 Oct 2026): `latexdiff`, `git-lfs`, WebKitGTK + Xvfb, and fix `PATH`.
Done here: the four `zig cc` wrappers moved to `~/.local/zig-wrappers/` (a clean `libgit2-sys`
rebuild then succeeded with no `CC=`), and WebKitGTK was already installed. `sudo` needs a password
this session cannot give, so `latexdiff`, `git-lfs` and Xvfb are a block G item.

### C7 · Windows: DNS blocked per process

Ledger, *Open*: on the Windows machine, `reqwest` (texbib's DOI/arXiv/ISBN lookups) and the bundled
Tectonic's package fetch cannot resolve DNS, while `curl.exe` in the same shell can; a security
suite filtering compiled binaries is the leading theory. Real-engine tests there need
`scripts/dev-proxy.py`. The zero-setup promise (rule 4) depends on the engine reaching the network
once. **Recommendation:** add an exception for `target\debug\*.exe`/`target\release\*.exe` (like
the existing WebView2 one), confirm, and record the outcome in the ledger — if a stranger's
security suite does the same thing, v0.9's stranger test will find it.

**Answer:** **Add the exception and record the outcome** (2 Oct 2026, the recommendation) — during
the Windows smoke campaign.

### C8 · Publishing `texlog` and `texbib`

S6.5 and S8.5 are `[~]` waiting on `cargo publish`. The licence split (MIT for these two) is in
`DESIGN.md` §10. **Question:** publish now, at v0.9, or at v1.0? **Recommendation:** your call;
nothing else is blocked by it.

**Answer:** **After E1 lands, before v0.9** (2 Oct 2026), so the first public `texlog` already has
E1's changed `Diagnostic`.

---

## 6. Block D — documentation drift (confirm once; the agent fixes all of it)

| # | Where | What is wrong | Fix |
|---|---|---|---|
| D1 | `DESIGN.md` §10, *Large figures* row | Says "LFS prompt still open … S11.3b, not yet carded" — S11.3c shipped the banner at 5 MB | Mark settled 2 Oct 2026, cite S11.3c |
| D2 | `DESIGN.md` §10, *Git remote scope* row | Decide-by "Sprint 10", never marked settled, though built exactly as written | Mark settled; remove "pull-request review" per B5 |
| D3 | `DESIGN.md` §6, activity-bar paragraph | "per §5.6" should be §5.5 (the assistant section) | Fix the reference |
| D4 | `DESIGN.md` §5.7, §6 table, §7 v0.6 | "rebase" | Per B6 |
| D5 | `CLAUDE.md`, *Layout* | Missing `abstract-tex-git`, `abstract-tex-github`, `abstract-tex-snapshot`, `abstract-tex-latexdiff`, `texwords` | Add them |
| D6 | `CLAUDE.md`, *Commands* / *Working solo* | "On this Windows machine cargo lives at …" is half the story; nothing about the Linux machine | Add the Linux notes (`CC=/usr/bin/cc …`, cannot push) |
| D7 | `fixtures/*/SMOKE.md`; `.claude/skills/run-stable-build/SKILL.md` | SMOKE scripts give a relative `ABSTRACT_TEX_OPEN` that opens nothing (ledger, *Open*); the skill still says "Preamble app" | Absolute-path form; rename |

**Answer:** **Accept all** (2 Oct 2026), plus §1's stale "five commits local only" (they were
pushed). Done: D1–D4 in `DESIGN.md`, D5–D6 in `CLAUDE.md` (and the same crate list in
`DEVELOPMENT.md`), D7 in every `fixtures/*/SMOKE.md` and the `run-stable-build` skill.

---

## 7. Block E — deferred technical decisions from the ledger

These are real design questions the ledger records as "not decided here". None blocks v0.6. The
interview only needs a direction and a rough *when*; each becomes a card later.

### E1 · Diagnostics that resolve inside a package file

A `\usepackage[nosuchlanguage]{babel}` error resolves to `babel.sty:4260`; the drawer heads a
group `babel.sty`, the jump fails, no gutter dot appears. `texlog`'s resolver knows the whole open
file stack but `Diagnostic` carries only the innermost file. Same shape for `fontspec.sty` and any
`\PackageError`. **Options:** (a) `Diagnostic` gains `project_file` — the innermost file that is
not a `.sty`/`.cls`/`.def` (an extension heuristic, since `texlog` never reads the tree); (b) ship
the whole stack over IPC and let the frontend pick the first file the project has.
**Recommendation: (b)** — the frontend already has the include graph, so no heuristic. Changes
`texlog`'s public type, so it lands before or with C8's publish.

**Answer:** **(b), ship the whole stack** (2 Oct 2026, the recommendation); lands before C8's
publish.

### E2 · Biber: bundle it or not?

Sprint 6's outcome said sprint 7 "starts by bundling `biber`"; it never happened, and S9.4
instead hands biblatex+biber projects to a detected system TeX. Nothing in `DESIGN.md` mentions
Biber at all. **Recommendation:** record "not bundled; system engine path (S9.4) is the answer"
in `DESIGN.md` §4, consistent with §1.3's "no TeX distribution". This also explains why `texlog`
has no `biblatex` rule.

**Answer:** **Not bundled; recorded in `DESIGN.md` §4** (2 Oct 2026, the recommendation).

### E3 · A BibTeX `.blg` rule

BibTeX writes its errors to `main.blg`, which `texlog` is never given. Narrowed in sprint 9: the
health checks already cover most of it; what is left is a missing `.bst` and style warnings. Trap
recorded: with `\include`, every chapter's `.blg` carries three spurious errors. **Recommendation:**
defer, low priority; when built, read only the root's `.blg`.

**Answer:** **Deferred, low priority** (2 Oct 2026, the recommendation); when built, the root's
`.blg` only.

### E4 · Include-graph robustness — five small gaps

All from S4.1's review, all still open: `detect_root` returns `None` when its exclusion empties
the candidate list; `MAX_DEPTH` drops nodes silently (contradicting the module's "never skip
silently"); `./main.tex` is not normalised; `\input{Sections/Intro}` keeps the directive's casing,
so a case-insensitive filesystem stops recompiling that file; `\InputIfFileExists`/`\subimport`
are invisible. The one real design question inside: **should the graph follow `.sty`/`.cls`
files' own `\input`s?** **Recommendation:** one sweep card for the five, with `.sty`/`.cls`
following *out* of scope (`\usepackage` already is) and the module doc narrowed to say so.

**Answer:** **One sweep card, `.sty`/`.cls` following out of scope** (2 Oct 2026, the
recommendation); unplaced.

### E5 · Shell-escape working-folder limits

Two linked entries: shell commands run in the build folder (to keep their output out of the source
tree), so `\write18{cat code/x.py}` cannot find a project-relative path, on either engine; and
toggling shell-escape consent forces one full latexmk rebuild. Neither has a general fix.
**Recommendation:** accept both as documented limitations (a line in `DEVELOPMENT.md`), revisit
only if minted v3's `\inputminted` turns out not to work either once a machine can run it.

**Answer:** **Accepted and documented** (2 Oct 2026, the recommendation): two bullets in
`DEVELOPMENT.md`; both ledger entries moved to *Won't fix* with that reason.

### E6 · System-engine speed is never gated

`latexmk` builds report zero passes and `full: true` always, so S9.5's performance gate covers
the bundled Tectonic only, while its doc comment claims "one ceiling per corpus document".
**Recommendation:** a wall-clock-only ceiling for system engines in CI where one is installed,
and fix the doc comment now.

**Answer:** **A wall-clock gate** (2 Oct 2026, the recommendation). The doc-comment fix goes in
that card rather than in this docs-only session.

### E7 · The fragile-command fix vanishes for long section titles

TeX truncates the error context at 50 characters from the left, so `\footnote` inside a realistic
`\section{…}` is cut off and S6.2's one-click `\protect` fix is never offered. Not fixable in
`texlog` (which never reads source). **Recommendation:** a small frontend card that searches the
diagnosed source line for a fragile command when the rule matched but named none.

**Answer:** **A frontend card, later** (2 Oct 2026, the recommendation); unplaced.

### E8 · One "ledger sweep" loop

Several open entries are one-line fixes waiting for an owner: the maths preview's `$` inside a
`%` comment (plus KaTeX `strict: 'ignore'`); normalising the drive-letter case of LSP URIs in one
place; a controller-level test for `shouldCompileFor`. **Recommendation:** one S-sized loop at the
start of sprint 12 (or between sprints), each fix with its test.

**Answer:** **The first thing in sprint 12** (2 Oct 2026, the recommendation).

*Not asked, noted:* the Graph's word counts are recomputed per page (~111 ms / 200 rows, no
cache) — the ledger's own conclusion is "leave until measured otherwise"; `latexminted` 0.6.0
crashing on Python 3.14 is upstream's.

---

## 8. Block F — later sprints, decided by the sprint noted

These shape cards not yet written. The interview only needs a direction; each sprint's cards
expand at its own start (`SPRINTS.md` §1.1).

### F1 ★ · Assistant providers (decide by sprint 12)

`DESIGN.md` §7 v0.7 and `SPRINTS.md` S12.1 say: bring-your-own-key, Anthropic Messages and
OpenAI-compatible endpoints, key in the OS keychain. `DESIGN.md` §6 additionally says the author
chooses "an API key, **a subscription sign-in** or a local model". A local model is covered by
the OpenAI-compatible path (Ollama, LM Studio, llama.cpp servers all speak it). Subscription
sign-in is a different thing: it would mean signing in to a consumer chat subscription from a
third-party app, which providers generally do not offer for this use. **Recommendation:** API key
+ OpenAI-compatible endpoint (local models included); drop "subscription sign-in" from §6.

**Answer:** **API key + OpenAI-compatible endpoints (local models included); no subscription
sign-in** (2 Oct 2026, the recommendation). `DESIGN.md` §5.5 and §6.

### F2 · Where the opt-in and keys live

"Everything is opt-in per project" (§5.5) — but a project file travels with the repository, so an
opt-in written into `abstract-tex.toml` would turn the assistant on for a coauthor who never
agreed. S9.8 already solved the identical problem for shell escape: consent per machine *and* per
project folder, outside the source tree. **Recommendation:** the same shape — machine-local opt-in
per project folder; keys in the keychain; nothing in the project.

**Answer:** **Per machine and per project folder, the S9.8 shape; keys in the keychain** (2 Oct
2026, the recommendation). `abstract-tex.toml`'s comment in §5.8 no longer says "AI opt-in".

### F3 · Model fallback for unmatched compile errors

v0.7 sends a log excerpt to a model when no rule matches, cached by log signature. That is data
leaving the machine on a compile — something the author did not directly ask for. "Nothing is sent
without an explicit action, and the exact payload is inspectable" (§5.5). **Recommendation:** never
automatic; a drawer card offers *"Ask the assistant"* with the payload shown first; cache
machine-local under `.abstract-tex/`. The §8 "zero outbound requests with no key" test covers the
rest.

**Answer:** **Per call, payload shown first; cache machine-local** (2 Oct 2026, the
recommendation).

### F4 · Which citation commands the fabrication guard scans

§5.5 names `\cite`, `\autocite`, `\parencite`. natbib (`\citep`, `\citet`) and biblatex
(`\textcite`, `\footcite`, …) would slip past a literal reading. **Recommendation:** every
cite-family command the project's citation completion already recognises, from one shared list, so
the guard and completion can never disagree.

**Answer:** **One shared list with citation completion** (2 Oct 2026, the recommendation).

### F5 ★ · Where do comments live after a live session? (decide by sprint 14)

**This is a genuine conflict in the design, not a detail.** v0.8 anchors comments to CRDT relative
positions, carried by the relay — and the relay "stores nothing, ever". `DESIGN.md` §5.8 / rule 1
forbid writing anything but plain `.tex`/`.bib`/`abstract-tex.toml` into the source tree, and
`.abstract-tex/` is git-ignored, so it does not travel. "Session end commits" (S15.2) cannot carry
comments anywhere without breaking one of those.

**Options.**
- (a) Comments are session-only; when the session ends they are gone (state it plainly).
- (b) Comments become `% COMMENT(ada): …` lines in the `.tex` at session end — plain files, travel
  with Git, but they edit the manuscript and break on paragraph rewrites.
- (c) A committed sidecar such as `comments.json` — travels, but breaks §5.8 and needs a re-anchoring
  story once the text moves.
- (d) Comments live on GitHub (PR review comments) — ties a core feature to one host, against §9.

**Recommendation:** none yet — this needs a real conversation, possibly with a small spike. If
forced: (a) for v0.8 with an honest message, (b) as an opt-in export.

**Answer:** **Spike a Git-ref comment store before sprint 14** (2 Oct 2026). This option was not in
the list above. It came up during the interview from the snapshot precedent: comments in
`refs/abstract-tex/comments`, pushed and fetched by *Sync* with an explicit refspec, anchored by
quoted text + context, re-anchored on load, orphans listed. The source tree stays untouched (rule
1), it works with any host (§9), and the relay still stores nothing. If the spike fails, the
fallback is (a), session-only, said plainly. `DESIGN.md` §5.6 and §10.

**Spike result (4 Oct 2026): passed.** `crates/abstract-tex-comments`, 12 tests. Anchoring by
quote + context reanchors correctly after an edit elsewhere and orphans (never guesses at) a
rewritten or ambiguously-repeated passage. The part that was a genuine risk, not a detail — two
authors who comment offline build two histories on the ref with no common ancestor — resolves
cleanly: every comment is its own blob named by its own content hash, so two additions are two
different tree entries and Git's ordinary 3-way merge sees no conflict; proven over a real
`file://` remote with explicit push/fetch refspecs, not simulated. **Decision: build S14.3 and
S15.2 on this store, not the session-only fallback.** Left open, for S14.3: editing or resolving
an existing comment, which needs its own append-only event rather than a rewrite of one.

### F6 · Relay hosting and joining a session (decide by sprint 14)

`DESIGN.md` §10's position: ship the binary and a Docker image, document a $5 VPS, host nothing.
Unanswered: how does a coauthor join — an invite link carrying the relay URL and a room secret?
Is the traffic end-to-end encrypted, given the relay operator could read manuscripts?
**Recommendation:** invite link with a random room secret; end-to-end encryption of updates with a
key derived from that secret, so a relay operator sees only ciphertext. Confirm the §10 position.

**Answer:** **Invite link with a random room secret, end-to-end encryption from a key derived from
it** (2 Oct 2026, the recommendation). §10's host-nothing position confirmed.

### F7 · Code signing (decide by sprint 14)

`DESIGN.md` §10: required for v0.9's stranger test to be honest. Apple Developer ~$99/yr; Azure
Trusted Signing on Windows. **Question:** budget it, or accept SmartScreen/Gatekeeper warnings?

**Answer:** **Still open; decide by sprint 14** (2 Oct 2026).

### F8 · Updater and crash reports vs "host nothing ourselves"

The Tauri updater needs a manifest URL; opt-in crash reports need somewhere to go; §1.3 forbids a
hosted service. **Recommendation:** the updater reads a manifest from GitHub Releases (no server of
our own); "crash reporting" means a local report plus a prefilled GitHub issue the user submits
themselves, never an automatic upload.

**Answer:** **Updater manifest from GitHub Releases; crash report = local report + a prefilled
issue** (2 Oct 2026, the recommendation).

### F9 · The companion GitHub Action (sprint 16)

**Recommendation:** its own small repository (Actions are referenced by repository), running
Tectonic and latexdiff, reusing `abstract-tex-latexdiff`'s flags so the PR PDF and the in-app PDF
agree. Also the answer to B5's PR story.

**Answer:** **Its own repository, the same flags as `abstract-tex-latexdiff`** (2 Oct 2026, the
recommendation).

---

## 9. Block G — actions only the maintainer can take

Not questions — a checklist to walk through at the end, with dates.

*Updated 2 October 2026, at the end of the interview, with each item's date.*

- [ ] **Now: force-push the rewritten history (C3).** Get this machine's `main` to the machine
  that pushes, however commits normally travel. Then run
  `git push --force-with-lease=main:80163cd origin main`; the lease refuses if anyone pushed in
  the meantime. On every other clone (the Windows machine included), first check for unpushed
  local work with `git log origin/main..main`. If there is none, run
  `git fetch && git reset --hard origin/main`. If there is some, run
  `git rebase --onto origin/main 80163cd main` after fetching. Once that's done, the local branch
  `backup/pre-rewrite-2026-10-02` here can be deleted. The old commits stay reachable on GitHub
  by direct URL until GitHub's own cleanup.
- [ ] **Now: read the CI run for the push of `80163cd`** (S2.8's first green run, S9.5's
  performance gate). `gh` is not installed on this machine, so the agent could not look.
- [ ] **Now: install the missing tools on this Linux machine (C6).**
  `sudo apt install texlive-extra-utils git-lfs xvfb`. `sudo` needs a password the agent could
  not give. WebKitGTK is already installed, and the `PATH` fix is done.
- [ ] **After S11.4d: the smoke campaign on Windows (C1),** walking every `fixtures/*/SMOKE.md`,
  S11.4c/d included. In the same session, add the security-suite exception for
  `target\debug\*.exe` / `target\release\*.exe` (C7) and record whether it cures the DNS
  failures.
- [ ] **At the start of S11.8: register the GitHub OAuth app (B1)** and put the client id in the
  release build environment (`ABSTRACT_TEX_GITHUB_CLIENT_ID`).
- [ ] **At S11.8: the v0.6 exit demo (B9).** Choose the machines then.
- [ ] **After E1, before v0.9:** `cargo publish -p texlog`, `cargo publish -p texbib` (C8).
- [ ] **The forty-reference exit demo, S8.4** (needs Zotero and a network). No date set.

---

## 10. After the interview — propagation checklist

For each answer, the agent updates, in this order:

1. **`DESIGN.md`** — every answer that changes or confirms a design statement: A1 (§5.1's
   one-build rule), A9/A11 (§5.7), B2/B3 (§7 v0.6), B5/B6 and D1–D4 (§4, §5.7, §6, §7, §9, §10),
   E2 (§4), F1–F8 (§5.5, §5.6, §6, §10). A settled §10 row gets "**settled <date>**", as the *Name*
   row already does.
2. **`SPRINTS.md`** — write the S11.4c and S11.4d cards from block A in the existing card format
   (*Loop / Reads / Depends / Files / Build / Verify / Done when*); add new cards for B2, B3 (and B4,
   B5, A11, E-block cards where answered "yes"), placed in roadmap order; update the catch-all
   roadmap paragraph after S11.4b; record C2's ordering decision.
3. **`bugs-issues-fixes.md`** — C4's move; each E-block entry gets its decision appended in place;
   anything newly found during the interview gets its own entry.
4. **`CLAUDE.md`** — D5, D6; C5 if `cargo fmt --check` joins `pnpm verify`.
5. **Memory** — only what will not be in the files above: e.g. a maintainer preference about how
   they like to be asked questions, or a standing decision about installing tools on this machine.
6. **This file** — leave the answers in place as the record; add a line at the top: *"Interview
   held <date>; answers propagated in <commit>."*
7. **Commit** — one commit, no attribution lines, e.g. `Planning: design interview answers
   recorded`. Do not push (this machine cannot).

---

## Appendix — sources consulted

- `0.1/DESIGN.md` — all sections, especially §1.3 non-goals, §2 commitments, §5.1, §5.5–§5.8, §6,
  §7 v0.6–v1.0, §8, §9, §10.
- `0.1/SPRINTS.md` — §1 (loop, rungs, definitions of done), every sprint table and outcome
  paragraph, cards S10.1–S11.4b, the sprint 12–16 roadmap lines.
- `bugs-issues-fixes.md` — every entry under *Open* (including those marked Fixed in place).
- `CLAUDE.md`, `~/.claude/CLAUDE.md`, and the project memory: no attribution lines, no subagents,
  no push credentials on this machine, the `zig cc` wrapper.
- Code checked where a doc claim needed it: `src/components/SourceControl.svelte` (Graph rows are
  not clickable today), `src/lib/state.svelte.ts` (`pdfUrl`, `pdfDraftOf`),
  `src-tauri/src/compile.rs` (the one-build rule), `crates/abstract-tex-engine` (`BuildJob`),
  `crates/abstract-tex-latexdiff` (what `render` does and does not do); `git status` (five commits
  ahead of `origin/main`); no clone, branch-switch or snapshot-restore code anywhere in the app.
