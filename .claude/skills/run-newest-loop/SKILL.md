---
name: run-newest-loop
description: >
  Find the newest completed, testable loop of work in this project and run its verification
  command. Use this whenever the user asks to run/test/verify "the latest", "the newest",
  "the current", or "the most recent" version, loop, feature, or piece of work — even if they
  don't name a file or a specific test command. Also use it for "what can I test right now",
  "is the latest loop actually working", or "run what's been built so far". This project tracks
  work as numbered loops (S<sprint>.<loop>, e.g. S7.4) in 0.1/SPRINTS.md rather than as loose
  commits, so "newest testable version" means the highest-numbered loop marked done there, not
  necessarily HEAD or the last commit message.
---

# Run the newest loop

This project (`0.1/SPRINTS.md`) breaks all work into numbered **loops** inside sprint sections.
Each loop is a row with a checkbox and, for expanded sprints, a fenced card underneath with a
`Verify` field — the exact command that proves that loop works. "The newest testable version" is
the highest-numbered loop that is both **checked off** and **has an expanded card**, because
that's the newest unit of work with a real pass/fail bar. It is deliberately *not* the same thing
as `git log -1` — a commit can be mid-loop (see commit messages like "mid loop commit") or touch
scaffolding with no verify step of its own.

Do this inline in the current session. Do not spawn a subagent for this — `CLAUDE.md`'s "Working
solo" section asks for the whole project to run in one session so context isn't re-derived across
spawns, and this task is small enough that splitting it off would only add overhead.

## Steps

1. **Read `0.1/SPRINTS.md` in full** (or at least every `### Sprint` section and every loop table
   and card — it's long, so grep for `^### Sprint`, `| \[` and `^Loop ` first to orient, then read
   the relevant ranges).

2. **Find every loop row and its checkbox state.** Rows look like:
   ```
   | [x] | S7.1 texbib parser crate: ... | L | — |
   | [~] | S7.2 .bib watcher and project-wide index ... | M | S7.1, S4.1 |
   | [ ] | S7.3 \cite completion ... | M | S7.2, S3.3 |
   ```
   `[x]` = done, `[~]` = partly done (the outcome paragraph says what remains — don't treat this
   as testable), `[ ]` = not started.

3. **Pick the candidate: the highest sprint.loop number marked `[x]`.** Compare numerically
   (S7.4 > S7.1 > S6.5), not by table order or file position — later sprints can have earlier
   loops finish out of order.

4. **Find that loop's expanded card**, a fenced block later in the same sprint section shaped like:
   ```
   Loop      S7.4 · DOI content negotiation · S
   Reads     ...
   Depends   ...
   Files     ...
   Build     ...
   Verify    cargo test -p texbib --features acquire (recorded replies as fixtures);
             cargo test -p texbib --features acquire -- --ignored (one live request)
   Done when the three pasted forms resolve to the same entry and the fixture replies round-trip.
   ```
   Per `SPRINTS.md` §1.1, only sprints that have started expansion get full cards — a later
   sprint may show only a one-line title in the prose summary. **If the newest `[x]` loop has no
   expanded card, step down to the next-newest `[x]` loop that does have one**, and tell the user
   you skipped a gap (name which loop and why).

5. **Run the card's `Verify` command(s) exactly as written**, via Bash, one at a time. A `Verify`
   field can list more than one command (semicolon- or newline-separated) — run all of them, since
   the loop isn't proven until each does.

6. **Also run `pnpm verify`** (cargo test + clippy + svelte-check + vitest — the project-wide gate
   from `CLAUDE.md`) as a sanity check that the newest loop didn't break anything else. Report it
   separately from the loop's own `Verify` command; it's a supplementary gate, not the primary
   thing being tested.

7. **Report results plainly:**
   - Which loop you ran and why (its id, title, and the sprint it's in).
   - Pass/fail for each command actually run, with enough of the failure output to act on it if it
     failed.
   - Quote the card's `Done when` clause and say whether the result actually satisfies it — a
     green test run doesn't always mean the qualitative bar in `Done when` is met (e.g. "every
     error → correct file, line, plain-language explanation" needs a human or a closer look at
     output, not just an exit code).
   - If you had to skip a gap in step 4, mention it here too.

## What this skill must never do

- Never tick, untick, or otherwise edit `0.1/SPRINTS.md` — this is a read-and-run task, not a
  record-keeping one.
- Never commit anything, even if all commands pass.
- Never spawn a subagent for any part of this (see "Working solo" in `CLAUDE.md`).
- Never treat a `[~]` (partly done) loop as the newest testable one, even if its number is higher
  than the newest `[x]` — partial loops don't have a trustworthy pass bar yet.
