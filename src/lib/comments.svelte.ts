// State for the Comments view (S14.3c). Only the controller writes to it.

import type { CommentRow } from './ipc';

class CommentsState {
  /** `null` until asked. */
  rows = $state.raw<CommentRow[] | null>(null);
  /** A sentence from Rust, e.g. that the folder is not a Git repository. Shown in place of the list. */
  error = $state<string | null>(null);
  /** A comment being written: the selection it is about, taken when *Comment* was chosen so that
   * moving the cursor while typing cannot change what it is attached to. */
  draft = $state.raw<{ file: string; from: number; to: number; text: string; quote: string } | null>(null);
  body = $state('');
  saving = $state(false);
}

export const comments = new CommentsState();
