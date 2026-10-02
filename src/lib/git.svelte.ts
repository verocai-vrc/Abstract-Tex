// The project's repository as the Source Control view sees it (S10.3a): one reactive snapshot of
// what `abstract_tex_git` reported, plus the pure functions that turn a path and a `ChangeKind`
// into the row DESIGN.md §6 asks for — `name · dir · M/U/A/D/R`.
//
// Kept apart from `state.svelte.ts` for the reason `bibliography.svelte.ts` is: `app` is what the
// three panes render, and this is data one view consults. Only the controller writes to `git`,
// the same rule `app` follows. The helpers take their input as arguments rather than reading the
// store, so `git.test.ts` exercises them with literals and no Svelte runtime.

import type { BranchState, ChangeKind, CommitRow, FileChange, GitStatus, LargeFile, ProseSummary, SyncOutcome } from './ipc';

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
 * Whether the *Sync Changes ↑n ↓m* button belongs on screen (S11.1b, DESIGN.md §6: "whenever the
 * branch is ahead or behind").
 *
 * `[0, 0]` — an upstream the branch agrees with — is not "ahead or behind" and hides the button,
 * the same distinction `syncArrows` already draws for its own empty string. No upstream at all
 * (`null`) hides it too: there is nothing here yet for the one-verb path to do.
 */
export function hasSyncWork(branch: BranchState | null): boolean {
  const drift = branch?.aheadBehind;
  return !!drift && (drift[0] > 0 || drift[1] > 0);
}

/**
 * Whether the Commit dropdown's *Amend* item belongs on screen (S11.1c, the design's own
 * guardrail: "Amend is hidden once the commit is pushed").
 *
 * Reads the same `ahead_behind` [`hasSyncWork`] does, because the two questions share an answer:
 * `ahead > 0` or no upstream at all means `HEAD` has never reached a remote, and amending it costs
 * nothing a `git push` would ever notice; `ahead === 0` against a real upstream means `HEAD` is
 * already there, and rewriting it would diverge local history from a remote someone may have
 * already pulled. Hidden rather than disabled: an item that only sometimes does what it says is
 * worse than one that is not there.
 */
export function canAmend(branch: BranchState | null): boolean {
  const drift = branch?.aheadBehind;
  return !drift || drift[0] > 0;
}

/**
 * The sentence under the Sync button once it has run (S11.1b) — what `abstract_tex_git::sync`
 * decided, in words, rather than a generic "Synced."
 *
 * Singular/plural spelled out rather than a trailing "(s)", for the same reason `relativeTime`
 * never shows "1 minutes ago": a sentence a person reads should read like one.
 */
export function syncOutcomeSentence(outcome: SyncOutcome): string {
  switch (outcome.kind) {
    case 'upToDate':
      return 'Already up to date.';
    case 'pushed':
      return outcome.ahead === 1 ? 'Pushed 1 commit.' : `Pushed ${outcome.ahead} commits.`;
    case 'fastForwarded':
      return outcome.behind === 1 ? 'Pulled 1 commit.' : `Pulled ${outcome.behind} commits.`;
  }
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

/**
 * A word count in the words a commit message can carry (S10.3c).
 *
 * ASCII, and that is deliberate: this ends up inside a commit message that `git log`, GitHub and
 * someone's terminal will all render, and a typographic minus sign there is a small act of
 * vandalism. `''` for no change, so a caller can leave it out entirely.
 */
export function wordDeltaLabel(delta: number): string {
  if (delta === 0) return '';
  return `${delta > 0 ? '+' : '-'}${Math.abs(delta)} words`;
}

/**
 * The commit message the box is pre-filled with — DESIGN.md §6's *"Revised §3.2 Methods, +240
 * words"*, built from the outline and the diff with no model involved (rule 6).
 *
 * The section *names* and not §3.2: numbering a section correctly needs the whole document walked
 * in `\input` order with `\appendix` and `\section*` accounted for, and a wrong number in a
 * commit message is a lie that outlives the commit. `texwords::section_of` carries the same note
 * on the Rust side.
 *
 * `''` when there is nothing true to say, which leaves the box empty rather than filling it with
 * a sentence about nothing.
 */
export function suggestedMessage(summary: ProseSummary | null): string {
  if (!summary) return '';
  const delta = summary.wordsAfter - summary.wordsBefore;
  const words = wordDeltaLabel(delta);
  const subject = subjectOf(summary);
  if (!subject) return '';
  return words ? `${subject}, ${words}` : subject;
}

/** The "what" half of the suggestion: a section if the change sat in one, a file otherwise. */
function subjectOf(summary: ProseSummary): string {
  const { sections, paths, addedPaths } = summary;
  // A brand-new file is news in itself, and its sections are all new too — naming the file says
  // more than naming a section nobody has seen before.
  if (addedPaths.length === 1 && paths.length === 1) return `Added ${addedPaths[0]}`;
  if (addedPaths.length > 1 && addedPaths.length === paths.length) return `Added ${addedPaths.length} files`;
  if (sections.length === 1) return `Revised ${sections[0]}`;
  if (sections.length === 2) return `Revised ${sections[0]} and ${sections[1]}`;
  if (sections.length > 2) return `Revised ${sections.length} sections`;
  // No section could be attributed: a preamble, a file with no headings, or a deletion.
  if (paths.length === 1) return `Revised ${paths[0]}`;
  if (paths.length > 1) return `Revised ${paths.length} files`;
  return '';
}

/**
 * A byte count as the LFS banner shows it (S11.3c) — one decimal place, because "5 MB" and
 * "5.0 MB" would read as two different precisions for the same threshold the banner tests against.
 */
export function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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

  /** Why the last *verb* was refused, as a sentence for the panel: a commit with no identity, a
   * `git init` inside an existing repository, a discard that failed. `null` when nothing has
   * been refused.
   *
   * Separate from [`readError`] because the two have different lifetimes, and conflating them
   * was a bug (found in S10.5a, ledger): every verb is followed by a status refresh, and a
   * successful refresh clearing one slot meant the refusal it was reporting vanished a moment
   * after appearing. A verb's refusal is cleared when the next verb starts, and by nothing else. */
  error = $state<string | null>(null);

  /** Why the last *read* failed — a repository Git itself cannot read. Cleared as soon as a read
   * succeeds, because unlike a refusal it describes a condition rather than an event. */
  readError = $state<string | null>(null);

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

  /** True while a commit is in flight, so the button cannot be pressed twice. Also true for the
   * whole of *Commit & Push* and *Commit & Sync* (S11.1b): the Commit box is what the author sees
   * as busy, whichever of the three they chose. */
  committing = $state(false);

  /** True while the standalone *Sync Changes* button's own call is in flight (S11.1b) — separate
   * from [`committing`] because this one is not waiting on a message in the box. */
  syncing = $state(false);

  /** What the last sync decided, as the sentence `syncOutcomeSentence` built — "Pushed 2
   * commits.", "Already up to date." `null` before the first one, and cleared like
   * [`lastDiscard`]: when the next verb starts, and by nothing else. */
  lastSync = $state<string | null>(null);

  /** Whether the box still holds *our* sentence rather than the author's (S10.3c).
   *
   * This is the whole of "it is never overwritten under them": while it is true a new suggestion
   * may replace what is in the box, and the first keystroke in the box turns it false for good —
   * until the commit lands and there is a new change to describe. */
  messageIsSuggested = $state(true);

  /** The numbers the suggestion was built from, kept so the panel can show the word count even
   * when the author has replaced the sentence. `null` before the first answer. */
  prose = $state.raw<ProseSummary | null>(null);

  /** Net words of prose added since the last commit — the one number a writer checks. */
  wordsSinceCommit = $derived(this.prose ? this.prose.wordsAfter - this.prose.wordsBefore : 0);

  /** True while `git init` is running (S10.5a), so the button cannot be pressed twice. */
  initialising = $state(false);

  /** The URL of this project's `origin`, or `null` when it has no remote yet (S10.5b). A project
   * that already has one is not a project to offer publishing to, whatever else is true of it. */
  originUrl = $state<string | null>(null);

  /** Set when the repository was made but Git has no identity to sign the first commit with:
   * the sentence names the two commands that fix it, and the tree is staged and waiting. */
  needsIdentity = $state(false);

  /** Whether this repository already ignores `.abstract-tex/`. `null` where the question does
   * not arise — no project, or no repository. False is the state S10.2a's note left open: a
   * repository older than this app, whose build folder the panel filters out rather than
   * relying on a line for. */
  ourFolderIsIgnored = $state<boolean | null>(null);

  /** How many of the newest rows are not on the upstream yet. */
  outgoing = $derived(outgoingCount(this.branch));

  /** Whether the *Sync Changes* button belongs on screen right now (S11.1b). */
  showSync = $derived(hasSyncWork(this.branch));

  /** Whether the Commit dropdown's *Amend* item belongs on screen right now (S11.1c). */
  showAmend = $derived(canAmend(this.branch));

  /** Large-file candidates for the "track with Git LFS" banner (S11.3c), read alongside the
   * three lists on every refresh. */
  largeFiles = $state.raw<LargeFile[]>([]);

  /** Paths the author has dismissed from the banner this session. Not persisted anywhere and
   * not pruned when a path leaves [`largeFiles`] — the same throwaway lifetime `commitMenuOpen`
   * has in the component, since a stale entry sitting unused in the set costs nothing. */
  dismissedLargeFiles = $state<Set<string>>(new Set());

  /** What the banner actually shows: candidates minus whatever was dismissed. */
  visibleLargeFiles = $derived(this.largeFiles.filter((file) => !this.dismissedLargeFiles.has(file.path)));

  /** True while a *Track with Git LFS* call is in flight. */
  trackingLfs = $state(false);
}

export const git = new GitState();
