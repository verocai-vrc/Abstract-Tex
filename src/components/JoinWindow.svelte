<script lang="ts">
  // S14.2c: *Join* — the inverse of Share. Paste an invite, connect, and the active tab starts
  // syncing with whoever shared it, replacing any session it already had.
  import { live } from '../lib/live.svelte';
  import { modalFocus } from '../lib/dialog';
  import { closeJoinWindow, joinSession } from '../lib/controller.svelte';

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeJoinWindow();
    if (event.key === 'Enter' && event.target instanceof HTMLInputElement) void joinSession();
  }
</script>

{#if live.joinVisible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeJoinWindow()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Join a live session" tabindex="-1" use:modalFocus onkeydown={onKeydown}>
      <header>
        <span>Join a live session</span>
        <button class="ghost" onclick={closeJoinWindow} aria-label="Close">✕</button>
      </header>

      <div class="field">
        <label for="join-invite">Invite</label>
        <input
          id="join-invite"
          type="text"
          placeholder="ws://relay.example.com:8080/room#…"
          bind:value={live.joinInviteText}
          disabled={live.joining}
          spellcheck="false"
          autocomplete="off"
        />
        <label for="join-name">Your name</label>
        <input
          id="join-name"
          type="text"
          placeholder="Anonymous"
          bind:value={live.displayName}
          disabled={live.joining}
          spellcheck="false"
          autocomplete="off"
        />
      </div>

      <footer>
        {#if live.joinError}<p class="error">{live.joinError}</p>{/if}
        <span class="spacer"></span>
        <button class="ghost" onclick={closeJoinWindow}>Cancel</button>
        <button class="primary" disabled={live.joining || live.joinInviteText.trim() === ''} onclick={() => void joinSession()}>
          {live.joining ? 'Joining…' : 'Join'}
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
    padding-top: 8vh;
  }
  .dialog {
    width: min(440px, 92vw);
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  input {
    font: inherit;
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
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
  footer p {
    margin: 0;
    font-size: 12px;
  }
  .error {
    color: var(--error);
  }
</style>
