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
    <button aria-disabled={isBusy} aria-describedby="start-open-folder" onclick={() => void chooseFolder()}>
      {words.chooseFolder}
    </button>
    <span id="start-open-folder" class="muted">{words.chooseFolderHint}</span>
  </li>
  <li>
    <button aria-disabled={isBusy} onclick={getFromGit}>{words.getFromGit}</button>
  </li>
</ul>

<!-- Both regions are always here: one that appears together with its text is often not read out. -->
<div role="status">
  {#if startScreen.looking !== null}<p class="said">{words.looking(startScreen.looking)}</p>{/if}
</div>
<div role="alert">
  {#if sentence}<p class="said">{sentence}</p>{/if}
</div>
{#if choice?.kind === 'crowdedFolder'}
  <button onclick={() => void chooseFolder()}>{words.chooseAgain}</button>
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
