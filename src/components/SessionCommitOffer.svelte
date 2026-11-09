<script lang="ts">
  // The card offered when a live session ends and the shared file has uncommitted changes
  // (S15.2). One click commits that file and nothing else; the message is editable and pre-filled.
  // Dismissing it changes nothing: the work stays an ordinary uncommitted change.
  import { acceptCommitOffer, dismissCommitOffer } from '../lib/controller.svelte';
  import { live } from '../lib/live.svelte';

  const offer = $derived(live.commitOffer);

  function focusOnCreate(node: HTMLElement) {
    node.focus();
  }
</script>

{#if offer}
  <div class="offer" role="dialog" aria-label="Commit this session's changes">
    <p class="title">Commit this session's changes to {offer.path}?</p>
    <input
      type="text"
      use:focusOnCreate
      value={offer.message}
      aria-label="Commit message"
      oninput={(event) => {
        if (live.commitOffer) live.commitOffer = { ...live.commitOffer, message: event.currentTarget.value };
      }}
      onkeydown={(event) => {
        if (event.key === 'Enter') void acceptCommitOffer();
        if (event.key === 'Escape') dismissCommitOffer();
      }}
    />
    {#if offer.error}<p class="error">{offer.error}</p>{/if}
    <div class="buttons">
      <button class="primary" disabled={offer.committing || offer.message.trim() === ''} onclick={() => void acceptCommitOffer()}
        >Commit</button
      >
      <button class="ghost" onclick={dismissCommitOffer}>Not now</button>
    </div>
  </div>
{/if}

<style>
  .offer {
    position: fixed;
    left: calc(var(--activity-bar-width) + 12px);
    bottom: calc(var(--statusbar-height) + 12px);
    width: 340px;
    padding: 12px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.25);
    z-index: 20;
  }
  .title {
    margin: 0 0 8px;
  }
  input {
    width: 100%;
    box-sizing: border-box;
  }
  .error {
    color: var(--error);
    margin: 6px 0 0;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
</style>
