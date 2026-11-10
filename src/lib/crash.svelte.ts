// State for the crash-report window (S16.3). Only the controller writes to it.

import type { CrashReport } from './ipc';

class CrashState {
  /** The report the author is being asked about; `null` when there is none, which is nearly always. */
  report = $state.raw<CrashReport | null>(null);
  /** Set after *Copy report*, so the button can say so. */
  copied = $state(false);
  error = $state<string | null>(null);
}

export const crash = new CrashState();
