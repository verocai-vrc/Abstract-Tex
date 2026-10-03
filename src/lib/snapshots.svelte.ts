// The Snapshots list and viewer (S11.6, design interview B3): what the hidden snapshot ref holds,
// shown to an author who has never once pressed commit.
//
// Only the controller writes to `snapshots`. The two helpers are plain functions so
// `snapshots.test.ts` exercises them with literals and no Svelte runtime.

import type { SnapshotRow } from './ipc';
import { relativeTime } from './git.svelte';

/** How many snapshots the section shows. A snapshot is taken on every successful compile, so the
 * list is long by nature; the newest ones are what anyone is looking for. */
export const SNAPSHOT_PAGE = 20;

/**
 * `4 minutes ago · 3,210 words`: when, and how much — the author recognises "the one with about
 * 3,000 words" long before a clock time. `now` is an argument for the reason `relativeTime`'s is.
 */
export function snapshotLabel(row: SnapshotRow, now: number): string {
  const words = row.words === 1 ? '1 word' : `${row.words.toLocaleString('en-US')} words`;
  return `${relativeTime(row.time, now)} · ${words}`;
}

/** Which file to show first: the project's root file when the snapshot has it, otherwise the first
 * one, otherwise nothing. */
export function defaultFile(files: readonly string[], rootFile: string | null): string | null {
  if (rootFile && files.includes(rootFile)) return rootFile;
  return files[0] ?? null;
}

class SnapshotsState {
  /** The newest snapshots. `null` until asked; `[]` for a project never compiled. */
  rows = $state.raw<SnapshotRow[] | null>(null);
  listError = $state<string | null>(null);

  /** The snapshot open in the viewer, and what it holds. */
  selected = $state.raw<SnapshotRow | null>(null);
  files = $state.raw<string[]>([]);
  filePath = $state<string | null>(null);
  /** That file as it was; `null` while it is being read. */
  text = $state<string | null>(null);
  viewError = $state<string | null>(null);

  restoring = $state(false);
  /** The sentence after a restore, shown in the viewer. */
  restored = $state<string | null>(null);
}

export const snapshots = new SnapshotsState();
