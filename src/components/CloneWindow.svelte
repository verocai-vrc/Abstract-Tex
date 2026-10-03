<script lang="ts">
  // "Clone a repository" (S11.5b, design interview B2): the second machine's way in, with no
  // terminal. Two ways to name what to clone — a row from the signed-in account's repositories, or
  // any address pasted in — and then one press, which asks where to put it and opens the result.
  //
  // The account list is a convenience and never a gate (rule 6): with nobody signed in the window
  // says so in a sentence, and the address field beside it works regardless.
  import { clone, filterRepositories } from '../lib/clone.svelte';
  import { github, timeLeft } from '../lib/github.svelte';
  import {
    chooseRepositoryToClone,
    cloneRepository,
    closeCloneWindow,
    loadCloneList,
    openVerificationPage,
    signInToGitHub,
  } from '../lib/controller.svelte';

  const shown = $derived(clone.repositories ? filterRepositories(clone.repositories, clone.filter) : []);

  let now = $state(Date.now());
  $effect(() => {
    if (!clone.visible || github.stage !== 'waiting') return;
    const timer = setInterval(() => (now = Date.now()), 1_000);
    return () => clearInterval(timer);
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeCloneWindow();
    if (event.key === 'Enter' && event.target instanceof HTMLInputElement && event.target.name === 'url') {
      void cloneRepository();
    }
  }
</script>

{#if clone.visible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeCloneWindow()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Clone a repository" tabindex="-1" onkeydown={onKeydown}>
      <header>
        <span>Clone a repository</span>
        <button class="ghost" onclick={closeCloneWindow} aria-label="Close">✕</button>
      </header>

      <div class="field">
        <label for="clone-url">Address</label>
        <input
          id="clone-url"
          name="url"
          type="text"
          placeholder="https://github.com/ada/thesis.git"
          bind:value={clone.url}
          disabled={clone.cloning}
          spellcheck="false"
          autocomplete="off"
        />
        <label for="clone-folder">Folder name <span class="muted">(optional)</span></label>
        <input
          id="clone-folder"
          type="text"
          placeholder="the repository's own name"
          bind:value={clone.folderName}
          disabled={clone.cloning}
          spellcheck="false"
          autocomplete="off"
        />
      </div>

      <div class="list-head">
        <span>Your repositories</span>
        {#if github.account}<span class="muted">@{github.account.login}</span>{/if}
      </div>

      {#if github.account}
        <input
          class="filter"
          type="search"
          placeholder="Filter…"
          aria-label="Filter your repositories"
          bind:value={clone.filter}
        />
        {#if clone.listing}
          <p class="muted">Asking GitHub…</p>
        {:else if clone.listError}
          <p class="muted error">{clone.listError}</p>
          <p class="muted"><button class="ghost" onclick={() => void loadCloneList()}>Try again</button></p>
        {:else if clone.repositories === null}
          <p class="muted">Not loaded yet.</p>
        {:else if clone.repositories.length === 0}
          <p class="muted">This account has no repositories yet. An address above still works.</p>
        {:else if shown.length === 0}
          <p class="muted">Nothing matches “{clone.filter}”.</p>
        {:else}
          <ul>
            {#each shown as repository (repository.cloneUrl)}
              <li>
                <button
                  class="repository"
                  class:chosen={clone.url === repository.cloneUrl}
                  disabled={clone.cloning}
                  onclick={() => chooseRepositoryToClone(repository)}
                >
                  {repository.fullName}
                  {#if repository.private}<span class="muted"> · private</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if github.stage === 'waiting' && github.code}
        <div class="sign-in">
          <span class="muted">Type this code at {github.code.verificationUri}</span>
          <span class="user-code">{github.code.userCode}</span>
          <span class="muted">{timeLeft(github.code.expiresAt, now)}</span>
          <button class="ghost" onclick={() => void openVerificationPage()}>Open GitHub</button>
        </div>
      {:else}
        <p class="muted">
          Sign in to GitHub to pick from your repositories, or paste an address above.
        </p>
        <p class="muted">
          <button disabled={github.stage !== 'idle'} onclick={() => void signInToGitHub()}>Sign in to GitHub…</button>
        </p>
        {#if github.error}<p class="muted error">{github.error}</p>{/if}
      {/if}

      <footer>
        {#if clone.error}<p class="error">{clone.error}</p>{/if}
        <span class="spacer"></span>
        <button class="ghost" onclick={closeCloneWindow}>Cancel</button>
        <button class="primary" disabled={clone.cloning || clone.url.trim() === ''} onclick={() => void cloneRepository()}>
          {clone.cloning ? 'Cloning…' : 'Clone…'}
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
    width: min(480px, 92vw);
    max-height: 80vh;
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
    padding: 10px 12px 4px;
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
  .filter {
    margin: 0 12px 4px;
  }
  .list-head {
    display: flex;
    justify-content: space-between;
    padding: 8px 12px 2px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
  }
  .repository {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: 5px 12px;
    text-align: left;
    background: none;
    border: none;
    font: inherit;
    font-size: 13px;
    color: var(--fg);
    cursor: pointer;
  }
  .repository:hover:not(:disabled),
  .repository.chosen {
    background: var(--bg-selected);
  }
  .sign-in {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 6px 12px;
  }
  .user-code {
    font-family: var(--font-mono);
    font-size: 18px;
    letter-spacing: 0.12em;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
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
  .muted {
    margin: 0;
    padding: 4px 12px;
    color: var(--fg-muted);
    font-size: 13px;
  }
  .field .muted {
    padding: 0;
    font-size: inherit;
  }
  .error {
    color: var(--error);
  }
</style>
