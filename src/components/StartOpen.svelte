<script lang="ts">
  // Open a document people already have (S17.8): a file, a folder, or a Git address. What cannot simply be
  // opened says what it is and what to do, here under the buttons.
  import { app } from '../lib/state.svelte';
  import { cancelChoice, chooseFile, chooseFolder, copyAndOpen, getFromGit, goToPage, openAnyway } from '../lib/start-controller.svelte';
  import { placeText, startScreen, strayQuestion } from '../lib/start.svelte';
  import { words } from '../lib/words';

  const choice = $derived(startScreen.choice);
  const isBusy = $derived(app.opening !== null);

  const sentence = $derived.by((): string => {
    if (choice === null) return '';
    switch (choice.kind) {
      case 'crowdedFolder':
        return words.crowdedFolder(placeText(choice.place));
      case 'noTex':
        return words.noTex(nameOf(choice.folder));
      case 'strayFile':
        return strayQuestion(choice);
      case 'zip':
        return ''; // unpacked at once
      default:
        return words.refused[choice.kind];
    }
  });

  let folderButton: HTMLButtonElement | undefined = $state();

  /** *Choose again…* is gone as soon as another is chosen; the button it stands for is not. */
  function chooseAgain() {
    folderButton?.focus();
    void chooseFolder();
  }

  // The answer is under the buttons, which fill the window when the text is enlarged. After the frame: where
  // the focus goes then is settled, and scrolling to that would undo this.
  let answer: HTMLElement | undefined = $state();
  $effect(() => {
    if (choice === null) return;
    requestAnimationFrame(() => answer?.scrollIntoView({ block: 'nearest' }));
  });

  function nameOf(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  }
</script>

<h1 tabindex="-1">{words.openTitle}</h1>
<ul class="ways" role="list">
  <li>
    <button data-first aria-disabled={isBusy} aria-describedby="start-open-file" onclick={() => void chooseFile()}>
      {words.chooseFile}
    </button>
    <span id="start-open-file" class="muted">{words.chooseFileHint}</span>
  </li>
  <li>
    <button bind:this={folderButton} aria-disabled={isBusy} aria-describedby="start-open-folder" onclick={() => void chooseFolder()}>
      {words.chooseFolder}
    </button>
    <span id="start-open-folder" class="muted">{words.chooseFolderHint}</span>
  </li>
  <li>
    <button aria-disabled={isBusy} onclick={getFromGit}>{words.getFromGit}</button>
  </li>
</ul>

<div class="answer" bind:this={answer}>
  <!-- Always here: a region that appears together with its text is often not read out. -->
  <div role="alert">
    {#if sentence}<p class="said">{sentence}</p>{/if}
  </div>
  {#if choice?.kind === 'crowdedFolder'}
    <button onclick={chooseAgain}>{words.chooseAgain}</button>
  {:else if choice?.kind === 'noTex'}
    <button aria-disabled={isBusy} onclick={() => void openAnyway()}>{words.openAnyway}</button>
  {:else if choice?.kind === 'strayFile'}
    <div class="answers">
      <button class="primary" aria-disabled={isBusy} onclick={() => void copyAndOpen()}>{words.copyAndOpen}</button>
      <button onclick={cancelChoice}>{words.cancel}</button>
    </div>
  {:else if choice?.kind === 'word'}
    <button onclick={() => goToPage('new', 'heading')}>{words.newDocument}</button>
  {/if}
</div>

{#if choice?.kind !== 'word'}
  <section>
    <p>{words.comingFromWord}</p>
    <button onclick={() => goToPage('new', 'heading')}>{words.newDocument}</button>
  </section>
{/if}

<style>
  .ways {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 24px;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }
  /* So that it is not scrolled to with its button against the edge of the window. */
  .answer {
    scroll-margin-bottom: 12px;
  }
  .answers {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .said {
    max-width: 720px;
    margin: 24px 0 12px;
    padding: 12px 16px;
    overflow-wrap: anywhere;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: var(--radius);
  }
  /* Enlarged text: the sentence and its buttons together in what is left for the page. */
  @media (max-width: 760px) {
    .said {
      margin: 12px 0 8px;
      padding: 8px 12px;
    }
  }
  section {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    max-width: 720px;
    margin-top: 40px;
    padding-top: 24px;
    border-top: 1px solid var(--border);
  }
</style>
