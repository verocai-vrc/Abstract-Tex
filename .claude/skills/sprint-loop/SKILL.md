---
name: sprint-loop
description: Run one SPRINTS.md loop through the four-agent team — architect (card) → builder (code + tests) → reviewer (verdict) → scribe (record + debrief) — then commit. Usage: /sprint-loop S2.7, or /sprint-loop with no id to take the next unblocked loop.
---

You are the orchestrator. You do not write code, review code, or write the record yourself —
you hand each agent a self-contained brief, pass outputs between them verbatim, and commit at
the end. Your own context should stay small; the agents' definitions in `.claude/agents/`
carry the role knowledge, so briefs are short.

## Steps

1. **Pick.** If `$ARGUMENTS` names a loop, use it. Otherwise read the current sprint's table in
   `0.1/SPRINTS.md` §3 and take the lowest-numbered `[ ]` loop whose `Depends` are `[x]`/`[~]`.
   Run `git status`; if the tree is dirty, stop and ask — one loop per branch.

2. **Architect** (`subagent_type: architect`, foreground). Brief: the loop id and this line:
   "Expand this loop into a full §1.1 card and check its dependencies." Receive the card.
   If it says a dependency is unmet or the loop must be split, show the maintainer and stop.

3. **Builder** (`subagent_type: builder`, foreground). Brief: the card verbatim, plus
   "Implement this loop per your definition. Report in your report format." If another loop is
   running in parallel, pass `isolation: worktree` and note the worktree path.
   Receive the report.

4. **Reviewer** (`subagent_type: reviewer`, foreground). Brief: the card and the builder's
   report verbatim, plus the worktree path if any. Receive the verdict.

5. **Fix round** (at most two). If `FIX REQUIRED`: `SendMessage` the *same* builder agent with
   the `required` findings verbatim — resuming keeps its context; a new spawn would re-read
   everything. Then re-run the reviewer with the builder's fix report. If still `FIX REQUIRED`
   after two rounds, stop and show the maintainer the open findings.

6. **Scribe** (`subagent_type: scribe`, foreground). Brief: the card, the final builder report,
   the final reviewer output, and today's date. Receive the debrief.

7. **Commit.** Check `git diff --stat` includes `0.1/SPRINTS.md`. Run `pnpm verify` once more
   yourself. Stage the changed files by name, commit with the scribe's `commit-msg.txt`.
   If in a worktree, tell the maintainer the branch name; do not merge without being asked.

8. **Debrief.** Show the scribe's debrief to the maintainer unchanged. Nothing else.

## Token rules

- Pass cards and reports verbatim; never paste conversation history or file contents the
  agent can read itself.
- Resume with `SendMessage` for follow-ups; spawn fresh only for a new role or a new loop.
- Do not read `DESIGN.md` yourself. The architect does.
- Parallel loops only when the architect confirms their `Files` rows are disjoint.
