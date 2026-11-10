// What a screen reader is told about the build (S16.4). The status bar's visible text changes
// every second while compiling; this changes only when the state does, so a live region carrying
// it speaks once per build stage, not once per tick.

import type { CompilePhase } from './state.svelte';

export function compileAnnouncement(phase: CompilePhase, durationMs: number, errorCount: number): string {
  switch (phase) {
    case 'running':
      return 'Compiling';
    case 'ok':
      return `Build finished in ${(durationMs / 1000).toFixed(1)} seconds`;
    case 'error':
      return errorCount === 1 ? 'Build failed with 1 error' : `Build failed with ${errorCount} errors`;
    case 'failed':
      return 'Could not build';
    case 'idle':
      return '';
  }
}
