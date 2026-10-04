<script lang="ts">
  // S14.2c: *Share* — a relay address and a display name turn into a fresh invite (room + key),
  // connected to immediately so there is a live session the instant a coauthor pastes it in.
  //
  // The address is never validated beyond "not empty": `DESIGN.md` §10 is "host nothing", so this
  // app has no way to know a real relay from a typo before trying to connect to it, and trying is
  // exactly what *Create invite* already does.
  import { app } from '../lib/state.svelte';
  import { live } from '../lib/live.svelte';
  import { modalFocus } from '../lib/dialog';
  import { closeShareWindow, startShare } from '../lib/controller.svelte';

  let copied = $state(false);
  $effect(() => {
    if (!live.shareVisible) copied = false;
  });

  // S14.2c: the invite stays shown once created (so it can still be copied), but the connection
  // behind it can still fail or drop afterward — this is what tells the author that happened
  // without making them go find the status bar's own, smaller version of the same thing.
  const connectionStatus = $derived(app.activePath ? (live.status.get(app.activePath) ?? null) : null);
  const connectionError = $derived(app.activePath ? (live.error.get(app.activePath) ?? null) : null);

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeShareWindow();
  }

  async function copyInvite() {
    if (!live.shareInvite) return;
    await navigator.clipboard.writeText(live.shareInvite);
    copied = true;
  }
</script>

{#if live.shareVisible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeShareWindow()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Share this document, live" tabindex="-1" use:modalFocus onkeydown={onKeydown}>
      <header>
        <span>Share this document, live</span>
        <button class="ghost" onclick={closeShareWindow} aria-label="Close">✕</button>
      </header>

      {#if live.shareInvite}
        <p class="muted">
          Anyone with this invite can edit along with you until one of you leaves. Send it however you'd send a
          link — it carries the key, and the relay never sees it.
        </p>
        <div class="field">
          <label for="share-invite">Invite</label>
          <textarea id="share-invite" readonly rows="2" value={live.shareInvite}></textarea>
        </div>
        {#if connectionStatus === 'error' || connectionStatus === 'closed'}
          <p class="error connection-notice">
            {connectionStatus === 'error' ? 'Could not reach that relay.' : 'The connection ended.'}
            {connectionError ? ` ${connectionError}` : ''}
          </p>
        {/if}
        <footer>
          <span class="spacer"></span>
          <button class="ghost" onclick={closeShareWindow}>Done</button>
          <button class="primary" onclick={() => void copyInvite()}>{copied ? 'Copied' : 'Copy invite'}</button>
        </footer>
      {:else}
        <div class="field">
          <label for="share-relay">Relay address</label>
          <input
            id="share-relay"
            type="text"
            placeholder="relay.example.com:8080"
            bind:value={live.relayAddress}
            disabled={live.sharing}
            spellcheck="false"
            autocomplete="off"
          />
          <label for="share-name">Your name</label>
          <input
            id="share-name"
            type="text"
            placeholder="Anonymous"
            bind:value={live.displayName}
            disabled={live.sharing}
            spellcheck="false"
            autocomplete="off"
          />
        </div>
        <footer>
          {#if live.shareError}<p class="error">{live.shareError}</p>{/if}
          <span class="spacer"></span>
          <button class="ghost" onclick={closeShareWindow}>Cancel</button>
          <button class="primary" disabled={live.sharing || live.relayAddress.trim() === ''} onclick={() => void startShare()}>
            {live.sharing ? 'Connecting…' : 'Create invite'}
          </button>
        </footer>
      {/if}
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
  .muted {
    margin: 0;
    padding: 10px 12px 0;
    color: var(--fg-muted);
    font-size: 13px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  input,
  textarea {
    font: inherit;
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
    resize: none;
  }
  textarea {
    font-family: var(--font-mono);
    word-break: break-all;
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
  .connection-notice {
    margin: 0;
    padding: 8px 12px 0;
    font-size: 12px;
  }
</style>
