// The GitHub sign-in as the window sees it (S10.4b): who is signed in, and what a sign-in in
// progress has to show. Kept apart from `state.svelte.ts` for the reason `bibliography.svelte.ts`
// and `git.svelte.ts` are — `app` is what the three panes render, and this is one panel's data.
//
// There is nothing here about a token, and there is nowhere to put one: the backend never sends
// it (`src-tauri/src/github.rs`), so this module cannot hold it even by accident.
//
// Only the controller writes to `github`. The pure helper below takes its input as an argument,
// so `github.test.ts` exercises it with a literal and no Svelte runtime.

import type { GitHubAccount, GitHubRepository, SignInEvent } from './ipc';

/** What the panel is doing. `idle` covers both "nobody has signed in" and "signed in already" —
 * which of those it is depends on `account`, not on a fourth state that could disagree with it. */
export type SignInStage = 'idle' | 'starting' | 'waiting';

/** The code and where to type it, for as long as a sign-in is waiting. */
export interface PendingCode {
  userCode: string;
  verificationUri: string;
  /** When the code stops working, as a timestamp — a duration would be stale a second later. */
  expiresAt: number;
}

/**
 * A repository name GitHub will take, from a project folder's name (S10.5b).
 *
 * GitHub replaces anything that is not a letter, digit, `.`, `-` or `_` with `-`, so doing it
 * here means the field shows what will actually be created rather than something GitHub silently
 * renames. A folder called `My Thesis (final)` becomes `My-Thesis-final`.
 */
export function suggestedRepositoryName(folderName: string): string {
  const cleaned = folderName
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-{2,}/g, '-');
  // A name GitHub will reject outright is worse than a dull one.
  return cleaned || 'paper';
}

/**
 * How long is left on a code, in the words a person waiting would use.
 *
 * Minutes while there are minutes, then seconds, then nothing at all: a code that has expired is
 * replaced by the offer to start again, so this never has to say "0 seconds".
 */
export function timeLeft(expiresAt: number, now: number): string {
  const seconds = Math.max(0, Math.round((expiresAt - now) / 1000));
  if (seconds === 0) return 'expired';
  if (seconds < 60) return `${seconds}s left`;
  return `${Math.ceil(seconds / 60)} min left`;
}

class GitHubState {
  /** Who is signed in, or `null`. `undefined` until the first answer, so the panel can stay
   * quiet rather than flashing "sign in" at somebody who already has. */
  account = $state.raw<GitHubAccount | null | undefined>(undefined);

  stage = $state<SignInStage>('idle');

  /** The code on screen, while one is. */
  code = $state.raw<PendingCode | null>(null);

  /** Why the last attempt did not work, as a sentence for the panel. `null` when there is
   * nothing to say — including after a cancellation, which is not a failure. */
  error = $state<string | null>(null);

  /** The repository just created (S10.5b), for the sentence that says where it went. Cleared
   * when a project opens: it describes one action on one project. */
  created = $state.raw<GitHubRepository | null>(null);

  /** True while the create call is in flight, so the button cannot be pressed twice — a second
   * press would leave a second empty repository on somebody's account. */
  creating = $state(false);

  /** Why creating one failed, as a sentence for the panel. */
  createError = $state<string | null>(null);

  /** Whether this build can sign in at all. False when no OAuth app is configured
   * (`abstract_tex_github::client_id`), which is a sentence rather than a broken button. */
  signedInIsPossible = $derived(this.error === null || !this.error.includes('not configured'));
}

export const github = new GitHubState();

/** Fold one `github:sign-in` event into the store. Exported for the controller, which is the
 * only caller — and for the test, which is what makes the three stages worth checking. */
export function applySignInEvent(event: SignInEvent, now: number): void {
  switch (event.stage) {
    case 'code':
      github.stage = 'waiting';
      github.error = null;
      github.code = {
        userCode: event.userCode,
        verificationUri: event.verificationUri,
        expiresAt: now + event.expiresInSeconds * 1000,
      };
      break;
    case 'signedIn':
      github.stage = 'idle';
      github.code = null;
      github.error = null;
      github.account = { login: event.login };
      break;
    case 'failed':
      github.stage = 'idle';
      github.code = null;
      // A cancellation is a decision, not a problem: the panel puts itself away and says nothing.
      github.error = event.cancelled ? null : event.message;
      break;
  }
}
