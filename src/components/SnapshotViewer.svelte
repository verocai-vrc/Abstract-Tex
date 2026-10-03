<script lang="ts">
  // One snapshot, read-only (S11.6, design interview B3): pick a file, read it as it was, and put
  // it back with *Restore this file*. Restoring keeps the version it replaces — the sentence after
  // it says so, and the same list holds it — so the button is safe to press without being sure.
  import { snapshots, snapshotLabel } from '../lib/snapshots.svelte';
  import { closeSnapshot, restoreSnapshotFile, showSnapshotFile } from '../lib/controller.svelte';

  const now = Date.now();

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeSnapshot();
  }
</script>

{#if snapshots.selected}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeSnapshot()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="An earlier version" tabindex="-1" onkeydown={onKeydown}>
      <header>
        <span>{snapshotLabel(snapshots.selected, now)}</span>
        <button class="ghost" onclick={closeSnapshot} aria-label="Close">✕</button>
      </header>

      {#if snapshots.files.length > 1}
        <div class="files" role="tablist" aria-label="Files in this version">
          {#each snapshots.files as path (path)}
            <button
              class="file"
              class:active={snapshots.filePath === path}
              role="tab"
              aria-selected={snapshots.filePath === path}
              onclick={() => void showSnapshotFile(path)}
            >
              {path}
            </button>
          {/each}
        </div>
      {/if}

      <div class="body">
        {#if snapshots.viewError}
          <p class="error">{snapshots.viewError}</p>
        {:else if snapshots.files.length === 0 && snapshots.text === null}
          <p class="muted">Opening…</p>
        {:else if snapshots.filePath === null}
          <p class="muted">This version has no .tex or .bib files.</p>
        {:else if snapshots.text === null}
          <p class="muted">Reading…</p>
        {:else}
          <pre>{snapshots.text}</pre>
        {/if}
      </div>

      <footer>
        {#if snapshots.restored}<p class="restored">{snapshots.restored}</p>{/if}
        <span class="spacer"></span>
        <button class="ghost" onclick={closeSnapshot}>Close</button>
        <button
          class="primary"
          disabled={snapshots.restoring || snapshots.filePath === null || snapshots.text === null}
          onclick={() => void restoreSnapshotFile()}
        >
          {snapshots.restoring ? 'Restoring…' : 'Restore this file'}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgba(0, 0, 0, 0.18);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 6vh;
  }
  .dialog {
    width: min(760px, 94vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 6px 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .file {
    padding: 3px 8px;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    font: inherit;
    font-size: 12px;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .file.active {
    color: var(--fg);
    border-bottom-color: var(--accent);
  }
  .body {
    flex: 1;
    min-height: 120px;
    overflow: auto;
  }
  pre {
    margin: 0;
    padding: 10px 12px;
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .muted,
  .error {
    margin: 0;
    padding: 12px;
    font-size: 13px;
  }
  .muted {
    color: var(--fg-muted);
  }
  .error {
    color: var(--error);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
  footer .spacer {
    flex: 1;
  }
  .restored {
    margin: 0;
    font-size: 12px;
    color: var(--ok);
  }
</style>
