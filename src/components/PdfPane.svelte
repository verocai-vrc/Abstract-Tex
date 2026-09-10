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
  {#if !app.pdfUrl}
    <div class="placeholder">
      {#if loadError}
        <p>{loadError}</p>
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
