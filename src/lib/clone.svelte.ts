// The Clone window's state (S11.5b, design interview B2): the address to clone, where the account's
// repositories are listed from, and what the last attempt said.
//
// Only the controller writes to `clone`, as with `git` and `github`. The filter is a plain
// function so `clone.test.ts` exercises it with literals and no Svelte runtime.

import type { GitHubRepository } from './ipc';

/**
 * The repositories whose name contains every word of `query`, in the order GitHub gave them.
 *
 * Words rather than a phrase, so `ada thesis` finds `ada/my-thesis`; case-insensitive; an empty
 * query keeps everything. Substring and not the fuzzy matcher the palette uses: a repository
 * list is read by name, and a subsequence match would offer `ada/phd-thesis` for "ath".
 */
export function filterRepositories(repositories: readonly GitHubRepository[], query: string): GitHubRepository[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...repositories];
  return repositories.filter((repository) => {
    const name = repository.fullName.toLowerCase();
    return words.every((word) => name.includes(word));
  });
}

class CloneState {
  visible = $state(false);

  /** The address to clone. Typed, or filled in by choosing a row. */
  url = $state('');
  /** The folder to make inside the one the author picks; empty means "named after the
   * repository", which Rust decides. */
  folderName = $state('');

  /** The account's repositories. `null` until asked, `[]` for an account with none. Replaced
   * whole, never edited, like the other lists. */
  repositories = $state.raw<GitHubRepository[] | null>(null);
  listing = $state(false);
  listError = $state<string | null>(null);
  filter = $state('');

  /** True from picking the folder until the clone has finished, so *Clone* cannot be pressed twice. */
  cloning = $state(false);
  /** Why the last clone did not happen, as a sentence for the window. */
  error = $state<string | null>(null);
}

export const clone = new CloneState();
