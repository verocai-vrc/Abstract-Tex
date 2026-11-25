<script lang="ts">
  // Where the PDF will be, until there is a first one (S17.14): what is being waited for, or why it
  // cannot be made now. The sentence is in a region that is always there, so that a screen reader says it
  // when it comes; the count and the clock are outside it, or they would be read out four times a second.
  import { triggerCompile } from '../lib/controller.svelte';
  import { app } from '../lib/state.svelte';
  import { words } from '../lib/words';

  const { active, otherwise }: { active: boolean; otherwise: string } = $props();

  // Not for a document that had a PDF before: its first build of the day may need one new part, which the
  // status bar names. "The first time… once" would be false.
  const fetching = $derived(active && !app.hasPdf && app.compile.phase === 'running' && app.compile.partsFetched > 0);
  const offline = $derived(active && app.compile.phase === 'failed' && app.compile.reason === 'needsInternetOnce');

  let now = $state(Date.now());
  $effect(() => {
    if (!fetching) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  /** The button is gone once the build runs, so the focus goes to the pane it is in. */
  function tryAgain(event: MouseEvent & { currentTarget: HTMLElement }) {
    event.currentTarget.closest<HTMLElement>('.pdf-column')?.focus();
    void triggerCompile();
  }
  const seconds = $derived(Math.max(0, Math.floor((now - app.compile.startedAt) / 1000)));
</script>

<div role="status">
  {#if fetching}
    <p class="first">{words.firstPdf}</p>
  {:else if offline}
    <p class="first">{words.needsInternetOnce}</p>
  {:else if otherwise}
    <p>{otherwise}</p>
  {/if}
</div>
{#if fetching}
  <p class="so-far">{words.firstPdfSoFar(app.compile.partsFetched, seconds)}</p>
{:else if offline}
  <button class="again" onclick={tryAgain}>{words.tryAgain}</button>
{/if}

<style>
  .first,
  .so-far,
  .again {
    font-size: 16px;
  }
  .first {
    max-width: 32em;
    margin: 0 24px;
    color: var(--fg);
  }
  .so-far {
    margin: 12px 24px 0;
    font-variant-numeric: tabular-nums;
  }
  .again {
    min-height: 44px;
    margin-top: 16px;
    padding: 0 20px;
    color: var(--fg);
  }
</style>
