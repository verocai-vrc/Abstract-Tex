<script lang="ts">
  // Home (S17.4): the document to carry on with, five templates, and the documents used last. Someone
  // with no documents yet sees a welcome, the templates, and the way to open one they already have.
  import { tick } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { showNewProjectWindow } from '../lib/controller.svelte';
  import type { RecentDocument } from '../lib/ipc';
  import { relativeTime } from '../lib/git.svelte';
  import { newProject } from '../lib/templates.svelte';
  import {
    clearRecents,
    findRecent,
    forgetRecent,
    goToPage,
    openRecent,
    showRecentInFolder,
    startFromTemplate,
  } from '../lib/start-controller.svelte';
  import { homeSections, homeTemplates, placeText, SEARCH_FROM, searchRecents, startScreen } from '../lib/start.svelte';
  import { words } from '../lib/words';

  const recents = $derived(startScreen.recents ?? []);
  const sections = $derived(homeSections(recents.length, app.project !== null));
  const carryOn = $derived(recents.find((recent) => recent.folder === startScreen.carryOn) ?? null);
  const shown = $derived(searchRecents(recents, startScreen.query));
  const templates = $derived(homeTemplates(newProject.catalog ?? []));
  const isBusy = $derived(app.opening !== null);

  const when = (recent: RecentDocument) => relativeTime(recent.usedAt, Date.now());
  const isOpenNow = (recent: RecentDocument) => recent.folder === app.project?.rootDir;
  const isMissing = (recent: RecentDocument) => startScreen.presence.get(recent.folder) === 'missing';

  let recentsHeading: HTMLHeadingElement | undefined = $state();
  let isAskingToClear = $state(false);

  /** What was focused is gone from the list, so the focus goes to the list's heading rather than nowhere. */
  async function change(action: Promise<void>): Promise<void> {
    isAskingToClear = false;
    await action;
    await tick();
    recentsHeading?.focus();
  }
</script>

{#if startScreen.recents !== null}
  {#if sections.includes('welcome')}
    <h1 tabindex="-1">{words.welcome}</h1>
    <p class="lead">{words.welcomeLine}</p>
  {:else}
    <h1 class="sr-only" tabindex="-1">{words.pages.home}</h1>
  {/if}

  {#each sections as section (section)}
    {#if section === 'carry-on'}
      <!-- Its space is kept while the folders are being asked: the page must not jump under a hand. -->
      <section class="carry-on" class:gone={startScreen.decided && carryOn === null}>
        {#if startScreen.decided && carryOn}
          <h2>{words.carryOn}</h2>
          <div class="card">
            <div class="about">
              <p class="title">{carryOn.title}</p>
              <p class="muted">{placeText(carryOn.place)}</p>
              <p class="muted">{isMissing(carryOn) ? words.notFound : when(carryOn)}</p>
            </div>
            <button
              class="primary"
              data-first
              aria-disabled={isBusy}
              aria-label={words.onDocument(
                isMissing(carryOn) ? words.findIt : words.open,
                words.row(carryOn.title, placeText(carryOn.place), when(carryOn)),
              )}
              onclick={() => void (isMissing(carryOn) ? findRecent(carryOn.folder) : openRecent(carryOn.folder))}
            >
              {isMissing(carryOn) ? words.findIt : words.open}
            </button>
          </div>
        {/if}
      </section>
    {:else if section === 'templates'}
      <section>
        <header>
          <h2>{words.startSomethingNew}</h2>
          {#if templates.length > 0}
            <button class="quiet" aria-disabled={isBusy} onclick={() => showNewProjectWindow()}>
              {words.seeAll(newProject.catalog?.length ?? 0)}
            </button>
          {/if}
        </header>
        {#if newProject.loadError}
          <p>{words.templatesCannotBeShown} {newProject.loadError}</p>
        {:else}
          <ul class="templates">
            {#each templates as template, i (template.id)}
              <li>
                <button
                  class="template"
                  data-first={i === 0 && !(sections.includes('carry-on') && carryOn) ? '' : undefined}
                  aria-disabled={isBusy}
                  onclick={() => startFromTemplate(template.id)}
                >
                  <img src={template.previewUrl} alt="" />
                  <span>{template.name}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {:else if section === 'open'}
      <section>
        <h2>{words.openOneYouHave}</h2>
        <button class="alone" onclick={() => goToPage('open', 'heading')}>{words.openButton}</button>
      </section>
    {:else if section === 'recents'}
      <section>
        <header>
          <h2 bind:this={recentsHeading} tabindex="-1">{words.recentDocuments}</h2>
          {#if recents.length >= SEARCH_FROM}
            <input
              type="search"
              placeholder={words.search}
              aria-label={words.searchRecents}
              bind:value={startScreen.query}
              spellcheck="false"
              autocomplete="off"
            />
          {/if}
        </header>
        {#if shown.length === 0}
          <p>{words.noRecentMatches(startScreen.query)}</p>
        {/if}
        <ul class="recents">
          {#each shown as recent (recent.folder)}
            <li>
              {#if isMissing(recent)}
                <div class="row">
                  <span class="title">{recent.title}</span>
                  <span class="muted">{placeText(recent.place)}</span>
                  <span class="not-found">{words.notFound}</span>
                </div>
              {:else}
                <button
                  class="row"
                  aria-disabled={isBusy}
                  aria-label={words.row(recent.title, placeText(recent.place), isOpenNow(recent) ? words.openNow : when(recent))}
                  onclick={() => void openRecent(recent.folder)}
                >
                  <span class="title">{recent.title}</span>
                  <span class="muted">{placeText(recent.place)}</span>
                  <span class="muted">{isOpenNow(recent) ? words.openNow : when(recent)}</span>
                </button>
              {/if}
              <div class="actions">
                {#if isMissing(recent)}
                  <button
                    class="quiet"
                    aria-disabled={isBusy}
                    aria-label={words.onDocument(words.findIt, recent.title)}
                    onclick={() => void findRecent(recent.folder)}
                  >
                    {words.findIt}
                  </button>
                {:else}
                  <button
                    class="quiet"
                    aria-label={words.onDocument(words.showInFolder, recent.title)}
                    onclick={() => void showRecentInFolder(recent.folder)}
                  >
                    {words.showInFolder}
                  </button>
                {/if}
                <button
                  class="quiet"
                  aria-label={words.onDocument(words.takeOffList, recent.title)}
                  onclick={() => void change(forgetRecent(recent.folder))}
                >
                  {words.takeOffList}
                </button>
                {#if isMissing(recent)}<span class="muted">{words.nothingIsDeleted}</span>{/if}
              </div>
            </li>
          {/each}
        </ul>
        <div class="clear">
          {#if isAskingToClear}
            <p>{words.clearListQuestion(recents.length)}</p>
            <button onclick={() => void change(clearRecents())}>{words.clearListYes}</button>
            <button onclick={() => (isAskingToClear = false)}>{words.clearListNo}</button>
          {:else}
            <button class="quiet" onclick={() => (isAskingToClear = true)}>{words.clearList}</button>
          {/if}
        </div>
      </section>
    {/if}
  {/each}
{/if}

<style>
  section {
    max-width: 960px;
    margin-bottom: 32px;
  }
  .lead {
    max-width: 640px;
    margin: 8px 0 24px;
    font-size: 18px;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 8px;
  }
  .title {
    font-size: 18px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .quiet {
    background: none;
    border-color: transparent;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .carry-on {
    min-height: 152px;
  }
  .carry-on.gone {
    display: none;
  }
  .card {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 8px;
    padding: 16px 20px;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .card .title {
    font-size: 20px;
  }
  .about {
    min-width: 0;
    flex: 1 1 240px;
  }
  .card button {
    min-width: 120px;
  }

  .templates {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 16px;
  }
  .template {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    padding: 12px;
  }
  .template img {
    width: 100%;
    max-width: 130px;
    border: 1px solid var(--border);
    background: white;
  }

  .recents > li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 12px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .row {
    flex: 1 1 420px;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 16px;
    padding: 8px 12px;
    text-align: left;
    background: none;
    border-color: transparent;
  }
  .row .title {
    flex: 1 1 220px;
    min-width: 0;
  }
  .row .muted {
    overflow-wrap: anywhere;
  }
  .not-found {
    flex-basis: 100%;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
  }
  .actions .quiet {
    padding: 8px 10px;
  }
  .clear {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 12px;
  }
  .alone {
    margin-top: 8px;
  }
</style>
