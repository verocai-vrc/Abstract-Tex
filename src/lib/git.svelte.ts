// The project's repository as the Source Control view sees it (S10.3a): one reactive snapshot of
// what `abstract_tex_git` reported, plus the pure functions that turn a path and a `ChangeKind`
// into the row DESIGN.md §6 asks for — `name · dir · M/U/A/D/R`.
//
// Kept apart from `state.svelte.ts` for the reason `bibliography.svelte.ts` is: `app` is what the
// three panes render, and this is data one view consults. Only the controller writes to `git`,
// the same rule `app` follows. The helpers take their input as arguments rather than reading the
// store, so `git.test.ts` exercises them with literals and no Svelte runtime.

import type { BranchState, ChangeKind, CommitRow, FileChange, GitStatus } from './ipc';

/** An empty answer: what the view shows before the first refresh, and after a project closes. */
export const NO_CHANGES: GitStatus = { staged: [], unstaged: [], conflicted: [] };

/** One row, with the three things it displays already worked out. */
export interface ChangeRow {
  /** Project-relative path, which is also the key every verb is called with. */
  path: string;
  /** The file's own name — `intro.tex` out of `sections/intro.tex`. */
  name: string;
  /** The folder it sits in, shown dim after the name. `''` for a file at the project root. */
  dir: string;
  /** The letter in the row shape: `M`/`U`/`A`/`D`/`R`, or `!` for a conflict. */
  letter: string;
  kind: ChangeKind;
  renamedFrom: string | null;
}

/**
 * The letter DESIGN.md §6's row shape puts at the end of a row.
 *
 * `U` is untracked, as in `git status --short`. A conflict gets `!` rather than a letter of its
 * own: the row shape has no letter for it, because §6 and S11.2 say a conflict is eventually
 * shown as two paragraphs and not as a row at all — but it must still be *visible* now, so it
 * gets the one mark that reads as "this one is different".
 */
export function letterFor(kind: ChangeKind): string {
  switch (kind) {
    case 'modified':
      return 'M';
    case 'added':
      return 'A';
    case 'deleted':
      return 'D';
    case 'renamed':
      return 'R';
    case 'untracked':
      return 'U';
    case 'conflicted':
      return '!';
  }
}

/** `'sections/intro.tex'` → `['intro.tex', 'sections']`; a root file has no directory. */
export function splitPath(path: string): [name: string, dir: string] {
  const slash = path.lastIndexOf('/');
  if (slash === -1) return [path, ''];
  return [path.slice(slash + 1), path.slice(0, slash)];
}

export function rowsOf(changes: readonly FileChange[]): ChangeRow[] {
  return changes.map((change) => {
    const [name, dir] = splitPath(change.path);
    return { path: change.path, name, dir, letter: letterFor(change.kind), kind: change.kind, renamedFrom: change.renamedFrom };
  });
}

/**
 * What the activity bar's badge counts.
 *
 * Paths, not rows. A file that is staged and then edited again is two rows — one in each list,
 * with a different letter — and a badge reading 2 for one changed file would be wrong in the
 * only place the author is counting. `abstract_tex_git::Status::changed_file_count` is the same
 * rule on the Rust side; this is not derived from it because the badge needs it from the same
 * snapshot the lists were drawn from, not from a second call that could answer differently.
 */
export function changedFileCount(status: GitStatus): number {
  const paths = new Set<string>();
  for (const change of [...status.staged, ...status.unstaged, ...status.conflicted]) paths.add(change.path);
  return paths.size;
}

/** How many commits one page of the Graph section holds — DESIGN.md §6: "a single-lane list of
 * the first 200 commits with lazy loading". */
export const GRAPH_PAGE = 200;

/**
 * The branch, as the status bar says it (S10.3b, DESIGN.md §6: branch name at the left).
 *
 * `''` when there is no repository, so the status bar shows nothing at all rather than a word
 * about Git in a project that has none. An unborn branch says so: its name exists and is worth
 * showing, but "main" on its own would look like a branch with a history behind it.
 */
export function branchLabel(branch: BranchState | null): string {
  if (!branch) return '';
  if (branch.unborn) return `${branch.name ?? 'main'} · no commits yet`;
  return branch.name ?? 'detached HEAD';
}

/**
 * The sync arrows next to it — `↑2 ↓1`, and the whole of what §5.7's `Sync Changes ↑n ↓m` is
 * drawn from.
 *
 * `''` in the two cases that are not the same thing and both look like nothing: no upstream at
 * all (`aheadBehind === null`), and an upstream this branch agrees with. The first gets its
 * sentence when there is something to say about a remote, in S10.4; the second is simply quiet.
 * A side that is zero is left out, because `↑2 ↓0` reads as if something were behind.
 */
export function syncArrows(branch: BranchState | null): string {
  const drift = branch?.aheadBehind;
  if (!drift) return '';
  const [ahead, behind] = drift;
  return [ahead > 0 ? `↑${ahead}` : '', behind > 0 ? `↓${behind}` : ''].filter(Boolean).join(' ');
}

/** How many commits are not on the upstream yet — the number of rows the *Outgoing changes*
 * header sits above. Zero when there is no upstream: nothing is outgoing if there is nowhere for
 * it to go. */
export function outgoingCount(branch: BranchState | null): number {
  return branch?.aheadBehind?.[0] ?? 0;
}

/**
 * A commit's age in words, from the epoch seconds Rust sends and the caller's idea of now.
 *
 * `now` is an argument and not `Date.now()` so this is a function of its inputs and the test can
 * pick a moment. Past a week it gives up on "ago" and shows the date, because "63 days ago" is
 * arithmetic the reader has to do and a date is not.
 */
export function relativeTime(seconds: number, now: number): string {
  const elapsed = Math.max(0, Math.round((now - seconds * 1000) / 1000));
  if (elapsed < 60) return 'just now';
  const minutes = Math.round(elapsed / 60);
  if (minutes < 60) return minutes === 1 ? '1 minute ago' : `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? '1 hour ago' : `${hours} hours ago`;
  const days = Math.round(hours / 24);
  if (days <= 7) return days === 1 ? 'yesterday' : `${days} days ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

class GitState {
  /** Whether the open project is inside a Git repository. False before the first answer, and for
   * a folder nobody has run `git init` in — which is a sentence in the view, not an error, and
   * gets its "create a repository" button in S10.5. */
  isRepository = $state(false);

  /** The three lists, replaced whole on every refresh like `bibliography.index` and for the same
   * reason: nothing edits a row in place, so deep reactivity would be cost for nothing. */
  status = $state.raw<GitStatus>(NO_CHANGES);

  stagedRows = $derived(rowsOf(this.status.staged));
  unstagedRows = $derived(rowsOf(this.status.unstaged));
  conflictedRows = $derived(rowsOf(this.status.conflicted));

  /** The badge on the Source Control icon. */
  changedCount = $derived(changedFileCount(this.status));

  /** Why the last Git call failed, as a sentence for the panel — a repository Git itself cannot
   * read, or a verb that could not be carried out. `null` when everything is answering. */
  error = $state<string | null>(null);

  /** What the last discard did, as a line the panel shows until the next refresh. Worth saying
   * out loud because the two outcomes are not equally recoverable. */
  lastDiscard = $state<string | null>(null);

  /** Which branch, and how far it has drifted (S10.3b). `null` before the first answer and where
   * there is no repository. */
  branch = $state.raw<BranchState | null>(null);

  /** The Graph section's rows, oldest page last. Replaced whole on a refresh and appended to by
   * *Show more*, so the author never loses the pages they had already asked for. */
  commits = $state.raw<CommitRow[]>([]);

  /** Whether the last page came back full, which is the only honest reason to offer *Show more*:
   * a short page means the history ended. */
  mayHaveMore = $state(false);

  /** The commit message being written. Lives here rather than in the component so that switching
   * to Files and back does not throw away half a sentence. The box binds to it directly — the
   * one piece of `git` state a component writes, the same way the status bar opens the Zotero
   * picker by writing `app.zoteroLinkVisible`. */
  message = $state('');

  /** True while a commit is in flight, so the button cannot be pressed twice. */
  committing = $state(false);

  /** How many of the newest rows are not on the upstream yet. */
  outgoing = $derived(outgoingCount(this.branch));
}

export const git = new GitState();
