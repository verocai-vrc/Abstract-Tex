<script lang="ts">
  // The strip of icons that hosts the left pane (S10.3a, DESIGN.md §6). VS Code's layout and
  // VS Code's two shortcuts, deliberately: it is the chrome a large share of this audience
  // already has muscle memory for, and there is nothing to gain from a novel one.
  //
  // Four icons, two of which do something. Assistant is v0.7 and Settings comes later; they are
  // drawn and disabled rather than left out, so that the icons above them never move when they
  // start working — which is the whole reason the design asks for all four now.
  //
  // This component owns no state: it reads which view is current and calls the controller. The
  // views themselves are `Sidebar.svelte` (Files) and `SourceControl.svelte`.
  import { showActivityView } from '../lib/controller.svelte';
  import { git } from '../lib/git.svelte';
  import { app } from '../lib/state.svelte';
</script>

<nav class="activity-bar" aria-label="Views">
  <button
    class="activity-item"
    class:active={app.activityView === 'files'}
    aria-current={app.activityView === 'files'}
    title="Files (Ctrl Shift E)"
    onclick={() => showActivityView('files')}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M9.5 1.5H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5.5z" />
      <path d="M9.5 1.5v4h3.5" />
    </svg>
    <span class="sr-only">Files</span>
  </button>

  <button
    class="activity-item"
    class:active={app.activityView === 'source-control'}
    aria-current={app.activityView === 'source-control'}
    title="Source Control (Ctrl Shift G)"
    onclick={() => showActivityView('source-control')}
  >
    <!-- VS Code's graph icon: a branch leaving a trunk. -->
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="4.5" cy="3" r="1.75" />
      <circle cx="4.5" cy="13" r="1.75" />
      <circle cx="11.5" cy="6" r="1.75" />
      <path d="M4.5 4.75v6.5" />
      <path d="M11.5 7.75c0 2-2 2.4-4.2 2.9" />
    </svg>
    <span class="sr-only">Source Control</span>
    {#if git.changedCount > 0}
      <!-- The badge counts changed *paths*, not rows: see `changedFileCount`. -->
      <span class="badge">{git.changedCount}</span>
    {/if}
  </button>

  <button class="activity-item" disabled title="Assistant — arrives in v0.7">
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect x="2.5" y="5" width="11" height="8" rx="2" />
      <path d="M8 5V2.5" />
      <circle cx="6" cy="9" r="1" />
      <circle cx="10" cy="9" r="1" />
    </svg>
    <span class="sr-only">Assistant</span>
  </button>

  <button class="activity-item" disabled title="Settings — arrives in a later version">
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="2.25" />
      <circle cx="8" cy="8" r="5.5" stroke-dasharray="2 2.2" />
    </svg>
    <span class="sr-only">Settings</span>
  </button>
</nav>
