#!/usr/bin/env python3
"""Which source files do the ledger's review entries never name? A lead list for the next review pass.

    scripts/review-coverage.py              # every source file, the least-named first, as a table
    scripts/review-coverage.py --unnamed    # only the files no review entry names
    scripts/review-coverage.py --markdown   # the same as a Markdown table (what REVIEW-HANDOFF.md embeds)

The ledger (`bugs-issues-fixes.md`) is the only record of what a review pass read: each pass's entry
names the files or areas it covered. This script counts how many times each source file is named from
the first review entry onward ("file"), and, for a file in a crate, how many times its crate is named
("crate": a pass that says "the rest of abstract-tex-git" covered the crate's files without listing
them). It proves nothing by itself. A pass can read a file and name it only by what it does ("the
watcher", "the PDF pane"), and an entry can name a file it only glanced at. Read a zero as "no one
has said they read this", and a count as "someone looked", then check `git log` and the entry.

For the count to stay honest, name every file you read in the entry you write ("Covered: ...").
A name that is the same in many places (`lib.rs`, `main.rs`) is counted by its crate's directory name.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "bugs-issues-fixes.md")
SKIP_DIRS = {"node_modules", "target", ".git", "dist", "binaries", "tests", "worktrees"}
AMBIGUOUS = {"lib.rs", "main.rs", "mod.rs"}


def review_text() -> str:
    """The ledger from the first agentic-review entry on (the entries before it are bug reports)."""
    with open(LEDGER, encoding="utf-8") as handle:
        text = handle.read()
    start = text.find("Agentic review of the E1")
    return text[start:] if start >= 0 else text


def source_files():
    for directory, directories, files in os.walk(ROOT):
        directories[:] = [d for d in directories if d not in SKIP_DIRS and not d.startswith(".")]
        relative_dir = os.path.relpath(directory, ROOT)
        in_scope = relative_dir.startswith(("crates", "src-tauri", "src"))
        if not in_scope:
            continue
        for name in files:
            is_rust = name.endswith(".rs") and not name.startswith("fake_")
            is_frontend = name.endswith((".ts", ".svelte")) and not name.endswith((".test.ts", ".d.ts"))
            if is_rust or is_frontend:
                yield os.path.join(relative_dir, name)


def needle_for(path: str) -> str:
    name = os.path.basename(path)
    if name not in AMBIGUOUS:
        return name
    parts = path.split(os.sep)
    # crates/<crate>/src/lib.rs -> the crate's directory name; src-tauri/src/lib.rs -> "src-tauri".
    return parts[1] if parts[0] == "crates" else parts[0]


def main() -> None:
    text = review_text()
    rows = []
    for path in source_files():
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as handle:
            lines = sum(1 for _ in handle)
        parts = path.split(os.sep)
        crate_named = len(re.findall(re.escape(parts[1]), text)) if parts[0] == "crates" else None
        rows.append((path, lines, len(re.findall(re.escape(needle_for(path)), text)), crate_named))
    if "--unnamed" in sys.argv:
        rows = [row for row in rows if row[2] == 0]
    rows.sort(key=lambda row: (row[2], row[3] if row[3] is not None else -1, -row[1], row[0]))

    if "--markdown" in sys.argv:
        print("| Lines | File named | Crate named | File |")
        print("|---:|---:|---:|---|")
        for path, lines, named, crate_named in rows:
            print(f"| {lines} | {named} | {'' if crate_named is None else crate_named} | `{path}` |")
    else:
        print("file crate  lines  path")
        for path, lines, named, crate_named in rows:
            print(f"{named:4d} {'' if crate_named is None else crate_named!s:>5} {lines:6d}  {path}")
    unnamed = [row for row in rows if row[2] == 0]
    print(
        f"\n{len(rows)} files, {sum(r[1] for r in rows)} lines; {len(unnamed)} files "
        f"({sum(r[1] for r in unnamed)} lines) are named by no review entry.",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
