<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { compare } from '../lib/compare.svelte';
  import {
    clearComparison,
    comparisonTitle,
    openPdfLink,
    retryComparisonWithSimplerMarkup,
    saveComparisonAs,
    syncTexInverse,
  } from '../lib/controller.svelte';
  import { describeFind, searchSeed, type FindStatus } from '../lib/pdf/find';
  import { PdfViewer } from '../lib/pdf/viewer';
  import { tick, untrack } from 'svelte';

  let container: HTMLElement | undefined = $state();
  let viewer: PdfViewer | null = null;
  let loadError = $state<string | null>(null);

  // Searching the text of the PDF. The searching is pdf.js's (`lib/pdf/find.ts`); this is the bar.
  let findOpen = $state(false);
  let findQuery = $state('');
  let findCaseSensitive = $state(false);
  let findWholeWord = $state(false);
  let findStatus = $state<FindStatus>({ kind: 'idle' });
  let findInput: HTMLInputElement | undefined = $state();
  let column: HTMLElement | undefined = $state();

  $effect(() => {
    if (!container) return;
    // A link in the PDF that goes to a web page opens in the person's browser, never in this window.
    const created = new PdfViewer(container, { onExternalLink: (url) => void openPdfLink(url) });
    // Not in diff mode: no line of the marked-up document is one the author can edit (A6).
    created.onInverseSearch((page, x, y) => {
      if (!compare.mode) void syncTexInverse(page, x, y);
    });
    created.onFindStatus((status) => (findStatus = status));
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

  // The shortcut and the palette ask through `app.pdfFindRequest`; a request already there when the pane
  // appeared was answered by whoever made it, so only a newer one opens the bar.
  let answeredFindRequest = untrack(() => app.pdfFindRequest?.nonce ?? 0);
  $effect(() => {
    const request = app.pdfFindRequest;
    if (!request || request.nonce === answeredFindRequest) return;
    answeredFindRequest = request.nonce;
    void openFind();
  });

  /** Show the bar with the cursor in it. Words selected in the PDF become the search; otherwise the
   * last search is still in the box, as in a browser, and is run again. */
  async function openFind() {
    const selection = window.getSelection();
    const selectedInPdf =
      selection && container && selection.anchorNode && container.contains(selection.anchorNode)
        ? searchSeed(selection.toString())
        : '';
    const barWasOpen = findOpen;
    const queryBefore = findQuery;
    if (selectedInPdf) findQuery = selectedInPdf;
    findOpen = true;
    await tick();
    findInput?.focus();
    findInput?.select();
    // Ctrl F on a bar that is already searching for these words only returns the cursor to the box:
    // searching again would move on to the next match.
    if (!barWasOpen || findQuery !== queryBefore) searchAgain();
  }

  function closeFind() {
    findOpen = false;
    viewer?.closeFind();
    // Back to the PDF, so the keyboard does not fall to the page behind.
    column?.focus();
  }

  function searchAgain() {
    viewer?.find(findQuery, { caseSensitive: findCaseSensitive, wholeWord: findWholeWord });
  }

  function onFindKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.shiftKey) viewer?.findPrevious();
      else viewer?.findNext();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      closeFind();
    }
  }

  /** Ctrl F with focus anywhere in the PDF pane opens the bar; Escape closes it. (Ctrl F elsewhere is
   * the editor's own search.) */
  function onColumnKeydown(event: KeyboardEvent) {
    const modifier = event.ctrlKey || event.metaKey;
    if (modifier && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      void openFind();
    } else if (event.key === 'Escape' && findOpen) {
      event.preventDefault();
      closeFind();
    }
  }

  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey || !viewer || !shownUrl) return;
    event.preventDefault();
    viewer.setZoom(viewer.getZoom() * (event.deltaY < 0 ? 1.1 : 0.9));
  }
</script>

<!-- tabindex -1: a click on the PDF puts the keyboard here, so Ctrl F finds the PDF's text while the
     reader is looking at it. -->
<div class="pdf-column" bind:this={column} tabindex="-1" role="presentation" onkeydown={onColumnKeydown}>
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
      {#if compare.view.retryable}
        <!-- A7: latexdiff's markup breaks some documents; this asks again with less of it. -->
        <button class="ghost" onclick={() => void retryComparisonWithSimplerMarkup()}>Try again with simpler markup</button>
      {/if}
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
  {#if findOpen}
    <!-- Over the top corner of the pane, not above it: opening it must not shift the pages. -->
    <div class="find-bar" role="search">
      <input
        bind:this={findInput}
        bind:value={findQuery}
        type="text"
        placeholder="Find in the PDF"
        aria-label="Find in the PDF"
        spellcheck="false"
        autocomplete="off"
        oninput={searchAgain}
        onkeydown={onFindKeydown}
      />
      <span class="find-status" role="status" aria-live="polite">{describeFind(findStatus)}</span>
      <button class="ghost" title="Previous match (Shift Enter)" aria-label="Previous match" onclick={() => viewer?.findPrevious()}>↑</button>
      <button class="ghost" title="Next match (Enter)" aria-label="Next match" onclick={() => viewer?.findNext()}>↓</button>
      <button
        class="ghost option"
        title="Match case"
        aria-label="Match case"
        aria-pressed={findCaseSensitive}
        onclick={() => {
          findCaseSensitive = !findCaseSensitive;
          searchAgain();
        }}>Aa</button>
      <button
        class="ghost option"
        title="Whole words only"
        aria-label="Whole words only"
        aria-pressed={findWholeWord}
        onclick={() => {
          findWholeWord = !findWholeWord;
          searchAgain();
        }}>ab</button>
      <button class="ghost" title="Close (Escape)" aria-label="Close the search" onclick={closeFind}>✕</button>
    </div>
  {/if}
</div>

<style>
  /* The grid's cell: banner on top, the pane (which scrolls, and which pdf.js owns) below. */
  .pdf-column {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    outline: none;
  }
  .find-bar {
    position: absolute;
    top: 8px;
    right: 24px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
    font-size: 13px;
  }
  .find-bar input {
    width: 180px;
    padding: 3px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .find-status {
    min-width: 6.5em;
    padding: 0 4px;
    color: var(--fg-muted);
    white-space: nowrap;
  }
  .find-bar button {
    padding: 2px 7px;
  }
  .find-bar button.option[aria-pressed='true'] {
    background: var(--bg-selected);
    border-color: var(--accent);
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
