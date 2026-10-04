// S14.2c: the Share and Join windows' state, and the live status of whichever open document has
// a session right now. Only the controller writes here, as with `clone` and `newProject`.
//
// The `LiveSession` instances themselves are not kept here: like `OpenDocument`, a session is a
// plain, non-reactive class (`relay/live-session.ts`) that reports changes through callbacks —
// `controller.svelte.ts` is what mirrors those callbacks into the plain state below, the same
// split `OpenDocument.onDirtyChange` already makes for `app.dirtyPaths`.

import type { LiveStatus } from './relay/live-session';
import type { LocalPresence } from './relay/awareness';

class LiveState {
  relayAddress = $state('');
  displayName = $state('');

  shareVisible = $state(false);
  shareInvite = $state<string | null>(null);
  shareError = $state<string | null>(null);
  sharing = $state(false);

  joinVisible = $state(false);
  joinInviteText = $state('');
  joinError = $state<string | null>(null);
  joining = $state(false);

  /** Path → status, for whichever open documents currently have a session. A plain reactive
   * `Map`, replaced wholesale on every change, like `app.dirtyPaths`. */
  status = $state<Map<string, LiveStatus>>(new Map());
  error = $state<Map<string, string | null>>(new Map());
  peers = $state<Map<string, LocalPresence[]>>(new Map());
}

export const live = new LiveState();
