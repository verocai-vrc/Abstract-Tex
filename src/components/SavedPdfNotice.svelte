<script lang="ts">
  // What *Save PDF…* did (S17.7): above the editor, or on the start screen when that is up. The region
  // is always in the page, because one that appears together with its text is often not read out.
  import { app } from '../lib/state.svelte';
  import { dismissSavedPdf, openSavedPdf, showSavedPdf } from '../lib/controller.svelte';
  import { savedPdfSentence } from '../lib/start.svelte';
  import { words } from '../lib/words';
</script>

<div role="status">
  {#if app.savingPdf}
    <div class="saved"><span>{words.savingPdf}</span></div>
  {:else if app.savedPdf}
    <div class="saved">
      <span>{savedPdfSentence(app.savedPdf)}</span>
      <div class="actions">
        <button onclick={() => void openSavedPdf()}>{words.openIt}</button>
        <button onclick={() => void showSavedPdf()}>{words.showInFolder}</button>
        <button class="close" aria-label={words.dismiss} onclick={dismissSavedPdf}><span aria-hidden="true">×</span></button>
      </div>
    </div>
  {/if}
</div>

<style>
  .saved {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    padding: 8px 12px;
    font-size: 16px;
    overflow-wrap: anywhere;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    border-left: 4px solid var(--accent);
  }
  .saved > span {
    flex: 1 1 200px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  button {
    min-height: 44px;
    padding: 0 14px;
    font-size: 16px;
  }
  .close {
    min-width: 44px;
  }
</style>
