---
name: run-stable-build
description: >
  Launch the actual Preamble app (Tauri window, not just a test command) at the last known-good
  commit, for a person to click through by hand. Use this whenever the user wants to "user test",
  "try it out", "click around", "see it working", "run the app", or "give me a build" — as opposed
  to running automated tests. In this project a plain `git log -1` or the current working tree can
  be mid-loop and unstable (commits like "mid loop commit" are checkpoints, not finished work), so
  "last stable" means the newest commit that actually completed a loop, per the `S<sprint>.<loop>:`
  commit convention in CLAUDE.md — not just whatever HEAD happens to be. This skill launches that
  version without disturbing the user's current uncommitted work, using a separate git worktree
  when HEAD isn't already stable.
---

# Run the last stable build

`CLAUDE.md`'s loop process only lets a commit happen after build, verify, and record (steps 3–7
in `0.1/SPRINTS.md` §1) — so a commit whose message follows the `S<sprint>.<loop>: <what changed>`
convention represents a loop that passed `pnpm verify` before it was committed. A commit that
doesn't follow that convention (e.g. "mid loop commit", "wip", "checkpoint") is a mid-loop
snapshot the maintainer made for their own reasons — it hasn't necessarily been verified, and it
may not even build. So "the last stable build" is **the newest commit on the current branch whose
message matches `^S\d+\.\d+:`**, not necessarily `HEAD`.

Do this inline in the current session — don't spawn a subagent (see "Working solo" in
`CLAUDE.md`). This skill only launches the app; it doesn't run `cargo test` or `pnpm verify`
itself (that's what the `run-newest-loop` skill is for). If the user also wants proof the code is
correct, not just something to click through, point them at that skill too.

## Steps

1. **Find the target commit.** From the repo root:
   ```
   git log --oneline -20 | grep -E '^[0-9a-f]+ S[0-9]+\.[0-9]+:'
   ```
   The first line printed is the target — the newest loop-convention commit. If `git log`
   doesn't reach one in the first ~20 commits, widen the search; it should be rare for many
   commits in a row to be non-conforming.

2. **Decide whether you can run in place or need a worktree.**
   - Run `git status --porcelain` and compare `HEAD` to the target commit from step 1.
   - If the working tree is clean **and** `HEAD` *is* the target commit, just run the app directly
     in the repo root (skip to step 4) — this is the common case right after finishing a loop.
   - Otherwise (uncommitted changes, or commits sitting on top of the target like a "mid loop
     commit"), do **not** touch the user's working tree — no `git checkout`, `git stash`, or
     `git reset` against it. Uncommitted work or a WIP commit may be the maintainer's in-progress
     work and is not yours to move or discard. Use a separate git worktree instead (step 3).

3. **Set up (or reuse) a worktree for the stable commit.** Pick a fixed sibling path next to the
   repo so repeat invocations reuse it instead of piling up new directories, e.g. for a repo at
   `/path/to/Abstract-Tex` use `/path/to/Abstract-Tex.stable-build`.
   - First time: `git worktree add <path> <target-commit>` (detached HEAD; this checks out a
     read-only snapshot without touching the main working tree at all).
   - Already exists from a previous run: `git -C <path> checkout <target-commit>` to move it
     forward to the current stable commit (cheap — it's just updating a detached checkout, not
     the user's branch).
   - To speed up the Rust build, point the worktree's Cargo at the main repo's build cache:
     `export CARGO_TARGET_DIR="<repo-root>/target"` before building in the worktree. It's the
     same workspace at a different commit, so sharing the cache is safe and avoids a from-scratch
     recompile.
   - Install frontend deps and the sidecar binaries inside the worktree (both are gitignored per
     `CLAUDE.md`, so a fresh worktree starts without them):
     ```
     cd <path>
     pnpm install
     pnpm fetch-sidecars
     ```
     pnpm's content-addressable store makes this fast even for a "fresh" install.

4. **Launch the app:** `pnpm tauri dev` (from the repo root if running in place, or from the
   worktree otherwise). This needs a real display (X11/Wayland/macOS/Windows) — it's a native
   window, not something that runs headless. Run it with `run_in_background: true` since it's a
   long-lived interactive process the user will click around in, not something with a finish line
   to wait for.

5. **Tell the user what they're looking at:** which commit/loop this build is (id and one-line
   title from the commit message), whether it's running in place or in a separate worktree (and
   the worktree's path, so they know it's a second checkout, not their main one), and point them
   at `fixtures/paper/SMOKE.md` for the manual smoke script that pairs with this rung of the
   verify ladder (`0.1/SPRINTS.md` §1, rung 4) if they want a guided pass rather than free-form
   clicking. Mention `fixtures/thesis`, `fixtures/broken`, and `fixtures/torture` as other real
   documents worth opening depending on what they're testing (multi-file navigation, a project
   that fails to build, or the error-explanation rule catalog).

## What this skill must never do

- Never run `git checkout`, `git stash`, `git reset`, or any other history-rewriting or
  tree-mutating command against the user's main working tree. All of that stays confined to the
  separate worktree this skill creates.
- Never spawn a subagent for any part of this (see "Working solo" in `CLAUDE.md`).
- Never delete the worktree automatically after launching — the app keeps running out of it, and
  the user may come back to it. If they say they're done testing, then `git worktree remove
  <path>` is fine.
- Never treat this as a substitute for `run-newest-loop` or `pnpm verify` — a build that launches
  and looks fine is not the same claim as "the tests pass."
