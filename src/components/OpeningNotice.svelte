<script lang="ts">
  // "Opening …" with *Stop*, over everything, while a folder is being read (S17.2). Outside the `inert`
  // shell. It never takes the focus: an Enter pressed twice must not stop what it started.
  import { app } from '../lib/state.svelte';
  import { stopOpening } from '../lib/controller.svelte';
  import { handledByInputMethod } from '../lib/keys';
  import { words } from '../lib/words';

  /** Most folders open before this, and a notice that flashes is only noise. */
  const SHOWN_AFTER_MS = 300;
  let shown = $state(false);

  $effect(() => {
    if (app.opening === null) return;
    const timer = setTimeout(() => (shown = true), SHOWN_AFTER_MS);
    return () => {
      clearTimeout(timer);
      shown = false;
    };
  });

  function onKeydown(event: KeyboardEvent) {
    if (app.opening === null || event.key !== 'Escape' || handledByInputMethod(event)) return;
    event.preventDefault();
    void stopOpening();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- The live region is always in the page: one that appears together with its text is often not read. -->
<div class="place" role="status">
  {#if shown && app.opening !== null}
    <div class="notice">
      <span>{words.opening(app.opening)}</span>
      <button onclick={() => void stopOpening()}>{words.stop}</button>
    </div>
  {/if}
</div>

<style>
  .place {
    position: fixed;
    inset: 30% 0 auto;
    display: flex;
    justify-content: center;
    pointer-events: none;
    z-index: 45;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 16px;
    max-width: min(560px, 90vw);
    padding: 16px 20px;
    font-size: 16px;
    overflow-wrap: anywhere;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.25);
    pointer-events: auto;
  }

  button {
    min-height: 44px;
    padding: 0 18px;
    font-size: 16px;
  }
</style>
