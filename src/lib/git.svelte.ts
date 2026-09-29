// The project's repository as the Source Control view sees it (S10.3a): one reactive snapshot of
// what `abstract_tex_git` reported, plus the pure functions that turn a path and a `ChangeKind`
// into the row DESIGN.md §6 asks for — `name · dir · M/U/A/D/R`.
//
// Kept apart from `state.svelte.ts` for the reason `bibliography.svelte.ts` is: `app` is what the
// three panes render, and this is data one view consults. Only the controller writes to `git`,
// the same rule `app` follows. The helpers take their input as arguments rather than reading the
// store, so `git.test.ts` exercises them with literals and no Svelte runtime.

import type { ChangeKind, FileChange, GitStatus } from './ipc';

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
}

export const git = new GitState();
