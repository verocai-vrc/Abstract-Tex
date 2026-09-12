<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { PdfViewer } from '../lib/pdf/viewer';

  let container: HTMLElement | undefined = $state();
  let viewer: PdfViewer | null = null;
  let loadError = $state<string | null>(null);

  $effect(() => {
    if (!container) return;
    viewer = new PdfViewer(container);
    return () => {
      void viewer?.destroy();
      viewer = null;
    };
  });

  // Reload whenever a new successful build publishes a URL. A failed build leaves `pdfUrl`
  // untouched, so the last good PDF stays on screen.
  $effect(() => {
    const url = app.pdfUrl;
    if (!url || !viewer) return;
    loadError = null;
    viewer.load(url).catch((error: unknown) => {
      console.error('[pdf] load failed', url, error);
      loadError = String(error);
    });
  });

  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey || !viewer || !app.pdfUrl) return;
    event.preventDefault();
    viewer.setZoom(viewer.getZoom() * (event.deltaY < 0 ? 1.1 : 0.9));
    void viewer.load(app.pdfUrl);
  }
</script>

<section class="pdf-pane" bind:this={container} onwheel={onWheel}>
  <!-- A load failure must show even once a URL exists: before, `loadError` was rendered
       only inside `{#if !app.pdfUrl}`, so a PDF that failed to load left the pane silently
       blank — indistinguishable from "no build yet". -->
  {#if loadError}
    <div class="placeholder">
      <p>{loadError}</p>
    </div>
  {:else if !app.pdfUrl}
    <div class="placeholder">
      {#if app.compile.phase === 'running'}
        <p>Compiling…</p>
      {:else if app.project}
        <p>The PDF will appear here after the first build.</p>
      {:else}
        <p>PDF preview</p>
      {/if}
    </div>
  {/if}
</section>
