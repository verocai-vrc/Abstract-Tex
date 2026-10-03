<script lang="ts">
  // One Changes row, side by side (S11.7, design interview B8): what *Stage* would add, or what the
  // next commit would hold. CodeMirror's merge view owns everything inside the host element;
  // Svelte mounts a container and stops. Read-only — *Open file* is the way to edit (editor/diff.ts
  // says why).
  import type { MergeView } from '@codemirror/merge';
  import { createDiffView } from '../lib/editor/diff';
  import { diffView, sideLabels } from '../lib/diffview.svelte';
  import { closeDiff, openDiffedFile } from '../lib/controller.svelte';

  let host: HTMLDivElement | undefined = $state();

  // Built when both the container and the texts exist, destroyed when either goes. The view is held
  // in a local and never read reactively, so this effect writes nothing it depends on
  // (MEMORY: effect_update_depth_exceeded).
  $effect(() => {
    const sides = diffView.sides;
    if (!host || !sides) return;
    const created: MergeView = createDiffView(host, sides.before, sides.after);
    return () => created.destroy();
  });

  const labels = $derived(sideLabels(diffView.open?.staged ?? false));

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeDiff();
  }
</script>

{#if diffView.open}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeDiff()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Changes in {diffView.open.path}" tabindex="-1" onkeydown={onKeydown}>
      <header>
        <span>{diffView.open.path}</span>
        <button class="ghost" onclick={closeDiff} aria-label="Close">✕</button>
      </header>

      <div class="labels">
        <span>{labels.before}</span>
        <span>{labels.after}</span>
      </div>

      <div class="body">
        {#if diffView.error}
          <p class="error">{diffView.error}</p>
        {:else if diffView.sides === null}
          <p class="muted">Reading…</p>
        {:else if diffView.sides.before === diffView.sides.after}
          <p class="muted">The two sides are the same.</p>
        {/if}
        <!-- Always present, so the effect has a container; empty and invisible until there is a view. -->
        <div class="merge" bind:this={host} hidden={diffView.sides === null || diffView.sides.before === diffView.sides.after}></div>
      </div>

      <footer>
        <span class="spacer"></span>
        <button class="ghost" onclick={closeDiff}>Close</button>
        <button class="primary" disabled={diffView.open.deleted} onclick={() => void openDiffedFile()}>
          Open file
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
    padding-top: 5vh;
  }
  .dialog {
    width: min(1100px, 96vw);
    height: 86vh;
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
  .labels {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 4px 12px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
    border-bottom: 1px solid var(--border);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .merge[hidden] {
    display: none;
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
</style>
