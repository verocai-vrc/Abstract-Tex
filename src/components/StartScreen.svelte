<script lang="ts">
  // The start screen (S17.4, DESIGN.md §6): a full-window layer over the editor shell, which `App.svelte`
  // makes `inert` meanwhile. A list down the left, the chosen page on the right, as in Word. While the
  // app does not know yet whether the command line named a folder, it is only the plain background.
  import { tick, untrack } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { backToDocument, goToPage, loadRecents } from '../lib/start-controller.svelte';
  import { startScreen } from '../lib/start.svelte';
  import { newProject } from '../lib/templates.svelte';
  import { words } from '../lib/words';
  import { savePdf } from '../lib/controller.svelte';
  import SavedPdfNotice from './SavedPdfNotice.svelte';
  import StartLook from './StartLook.svelte';
  import StartHome from './StartHome.svelte';
  import StartNew from './StartNew.svelte';
  import StartOpen from './StartOpen.svelte';

  let page: HTMLElement | undefined = $state();

  // Each time Home is shown the list is read again: what was used last has changed. `untrack` because
  // loading reads the store it writes.
  $effect(() => {
    if (startScreen.visible && startScreen.page === 'home') untrack(() => void loadRecents());
  });

  // Put the focus where this page wants it: its heading, so that a screen reader says where one is, or
  // the first thing to press. Home's first thing is only known once *Carry on* has been decided.
  const isReady = $derived(
    startScreen.page !== 'home' ||
      startScreen.focusTarget === 'heading' ||
      (startScreen.decided && (newProject.catalog !== null || newProject.loadError !== null)),
  );
  $effect(() => {
    void startScreen.focusNonce;
    if (!startScreen.visible || !page || !isReady) return;
    const target = startScreen.focusTarget;
    void tick().then(() => {
      if (document.querySelector('.backdrop')) return; // a dialog has it
      const first = target === 'first' ? page?.querySelector<HTMLElement>('[data-first]') : null;
      (first ?? page?.querySelector<HTMLElement>('h1'))?.focus();
    });
  });
</script>

{#if app.launching}
  <div class="start"></div>
{:else if startScreen.visible}
  <div class="start">
    <nav aria-label={words.startNavigation}>
      <p class="name">{words.appName}</p>
      {#if app.project}
        <button class="back" aria-disabled={app.opening !== null} onclick={backToDocument}>
          <span aria-hidden="true">‹</span> {words.backTo(app.project.title)}
        </button>
        <button class="back" aria-disabled={!app.hasPdf || app.savingPdf || app.opening !== null} onclick={() => void savePdf()}>
          {words.savePdf}
        </button>
      {/if}
      <ul>
        <li>
          <button aria-current={startScreen.page === 'home' ? 'page' : undefined} onclick={() => goToPage('home', 'heading')}>
            {words.pages.home}
          </button>
        </li>
        <li>
          <button aria-current={startScreen.page === 'new' ? 'page' : undefined} onclick={() => goToPage('new', 'heading')}>
            {words.pages.new}
          </button>
        </li>
        <li>
          <button aria-current={startScreen.page === 'open' ? 'page' : undefined} onclick={() => goToPage('open', 'heading')}>
            {words.pages.open}
          </button>
        </li>
        <li class="apart">
          <button aria-current={startScreen.page === 'look' ? 'page' : undefined} onclick={() => goToPage('look', 'heading')}>
            {words.pages.look}
          </button>
        </li>
      </ul>
    </nav>

    <main bind:this={page}>
      <!-- Both regions are always here: one that appears together with its text is often not read out. -->
      <div role="alert">
        {#if startScreen.message?.kind === 'refusal'}<p class="message refusal">{startScreen.message.text}</p>{/if}
      </div>
      <div role="status">
        {#if startScreen.message?.kind === 'status'}<p class="message">{startScreen.message.text}</p>{/if}
      </div>
      <SavedPdfNotice />
      {#if startScreen.page === 'open'}
        <StartOpen />
      {:else if startScreen.page === 'new'}
        <StartNew />
      {:else if startScreen.page === 'look'}
        <StartLook />
      {:else}
        <StartHome />
      {/if}
    </main>
  </div>
{/if}

<style>
  .start {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    background: var(--bg);
    color: var(--fg);
    font-size: 16px;
    line-height: 1.45;
  }

  /* Every page's controls, once. Text of 16 px and a target of 44 px are the floor here: the people this
     screen is for are often older (`start.test.ts` guards both). */
  :global(.start :is(button, input)) {
    min-height: 44px;
    font: inherit;
    font-size: 16px;
  }
  :global(.start button) {
    padding: 8px 18px;
    background: var(--bg-editor);
  }
  :global(.start button:hover:not([aria-disabled='true'])) {
    background: var(--bg-selected);
  }
  :global(.start button.primary) {
    background: var(--accent);
    color: var(--accent-fg);
  }
  :global(.start button.primary:hover) {
    background: var(--accent);
    text-decoration: underline;
  }
  /* `aria-disabled` and not `disabled` while a document is opening: a disabled button drops the focus. */
  :global(.start [aria-disabled='true']) {
    opacity: 0.6;
    cursor: default;
  }
  :global(.start input) {
    padding: 8px 12px;
    color: var(--fg);
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  :global(.start :is(h1, h2, p, ul)) {
    margin: 0;
    padding: 0;
  }
  :global(.start ul) {
    list-style: none;
  }
  :global(.start h1) {
    font-size: 28px;
    font-weight: 600;
    line-height: 1.2;
  }
  :global(.start h2) {
    font-size: 20px;
    font-weight: 600;
  }
  :global(.start h1:focus) {
    outline: none;
  }
  :global(.start .muted) {
    color: var(--fg-muted);
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 24px 12px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--bg-panel);
  }
  .name {
    padding: 0 12px;
    font-size: 20px;
    font-weight: 600;
  }
  nav ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .apart {
    margin-top: 16px;
  }
  nav button {
    width: 100%;
    text-align: left;
    background: none;
    border-color: transparent;
    border-left: 4px solid transparent;
  }
  .back {
    margin-left: -8px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  nav button[aria-current] {
    border-left-color: var(--accent);
    font-weight: 600;
    background: var(--bg-selected);
  }

  main {
    padding: 32px 40px 48px;
    overflow-y: auto;
  }
  .message {
    margin-bottom: 20px;
    padding: 12px 16px;
    overflow-wrap: anywhere;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: var(--radius);
  }
  .message.refusal {
    border-left-color: var(--error);
  }

  /* Enlarged text in an ordinary window: the list goes above the page, so that neither is squeezed. */
  @media (max-width: 760px) {
    .start {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }
    nav {
      flex-flow: row wrap;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
    nav ul {
      flex-flow: row wrap;
    }
    nav button {
      width: auto;
    }
    .apart {
      margin-top: 0;
    }
    main {
      padding: 20px 20px 32px;
    }
  }
</style>
