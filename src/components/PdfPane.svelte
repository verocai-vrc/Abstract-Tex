<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { compare } from '../lib/compare.svelte';
  import { clearComparison, comparisonTitle, saveComparisonAs, syncTexInverse } from '../lib/controller.svelte';
  import { PdfViewer } from '../lib/pdf/viewer';

  let container: HTMLElement | undefined = $state();
  let viewer: PdfViewer | null = null;
  let loadError = $state<string | null>(null);

  $effect(() => {
    if (!container) return;
    const created = new PdfViewer(container);
    // Not in diff mode: no line of the marked-up document is one the author can edit (A6).
    created.onInverseSearch((page, x, y) => {
      if (!compare.mode) void syncTexInverse(page, x, y);
    });
    viewer = created;
    return () => {
      void viewer?.destroy();
      viewer = null;
    };
  });

  // SyncTeX forward search (S3.4): scroll to and highlight wherever the editor last asked for.
  $effect(() => {
    const request = app.syncTexScrollRequest;
    if (request && viewer) viewer.scrollToPosition(request.page, request.x, request.y);
  });

  // Reload whenever a new successful build publishes a URL. A failed build leaves `pdfUrl`
  // untouched, so the last good PDF stays on screen.
  // Diff mode (S11.4d) puts the comparison's PDF in the pane instead of the live one. The live
  // build keeps running underneath and `app.pdfUrl` keeps following it, so leaving diff mode shows
  // the newest live PDF with no further work.
  const shownUrl = $derived(compare.mode ? compare.view.pdfUrl : app.pdfUrl);
  const bannerTitle = $derived(compare.mode ? comparisonTitle(Date.now()) : '');

  $effect(() => {
    const url = shownUrl;
    if (!viewer) return;
    if (!url) {
      // Nothing to show (another project just opened, or a comparison is still being built):
      // the pages of whatever was here before must not stay on screen under the placeholder.
      void viewer.clear();
      return;
    }
    loadError = null;
    viewer.load(url).catch((error: unknown) => {
      console.error('[pdf] load failed', url, error);
      loadError = String(error);
    });
  });

  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey || !viewer || !shownUrl) return;
    event.preventDefault();
    viewer.setZoom(viewer.getZoom() * (event.deltaY < 0 ? 1.1 : 0.9));
    void viewer.load(shownUrl);
  }
</script>

<div class="pdf-column">
  {#if compare.mode}
    <!-- Louder than the draft's status-bar note on purpose: an author who forgets they are looking
         at a comparison will think the paper is full of red strike-throughs. -->
    <div class="diff-banner" role="status">
      <strong>
        {#if compare.view.phase === 'building'}
          Building the comparison…{compare.view.progress ? ` ${compare.view.progress}` : ''}
        {:else if compare.view.phase === 'failed'}
          {bannerTitle} — it could not be built.
        {:else}
          {bannerTitle}
        {/if}
      </strong>
      {#if compare.view.phase === 'failed' && compare.view.message}<span class="diff-note">{compare.view.message}</span>{/if}
      <span class="spacer"></span>
      {#if compare.saveError}<span class="diff-error">{compare.saveError}</span>{/if}
      {#if compare.view.phase === 'ready'}
        <button class="ghost" onclick={() => void saveComparisonAs()}>Save as…</button>
      {/if}
      <button class="ghost" onclick={clearComparison}>Back to live PDF</button>
    </div>
  {/if}
<section class="pdf-pane" bind:this={container} onwheel={onWheel}>
  <!-- A load failure must show even once a URL exists: before, `loadError` was rendered
       only inside `{#if !app.pdfUrl}`, so a PDF that failed to load left the pane silently
       blank — indistinguishable from "no build yet". -->
  {#if loadError}
    <div class="placeholder">
      <p>{loadError}</p>
    </div>
  {:else if !shownUrl}
    <div class="placeholder">
      {#if compare.mode}
        <p>{compare.view.phase === 'failed' ? 'There is no comparison to show.' : 'The comparison will appear here.'}</p>
      {:else if app.compile.phase === 'running'}
        <p>Compiling…</p>
      {:else if app.project}
        <p>The PDF will appear here after the first build.</p>
      {:else}
        <p>PDF preview</p>
      {/if}
    </div>
  {/if}
</section>
</div>

<style>
  /* The grid's cell: banner on top, the pane (which scrolls, and which pdf.js owns) below. */
  .pdf-column {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .pdf-column > :global(.pdf-pane) {
    flex: 1;
    min-height: 0;
  }
  .diff-banner {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--accent);
    color: var(--bg-editor);
    font-size: 13px;
  }
  .diff-banner .spacer {
    flex: 1;
  }
  .diff-banner .diff-note {
    font-size: 12px;
  }
  .diff-banner .diff-error {
    font-size: 12px;
  }
  .diff-banner button {
    color: inherit;
    border: 1px solid currentColor;
  }
</style>
