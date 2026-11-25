<script lang="ts">
  // New (S11.11; a page since S17.6): the kinds of document as cards that can be searched and filtered,
  // then two questions, then *Create*, which makes it where documents are kept and opens it. Nothing is
  // written before that. Esc steps back (`startBack`).
  import { app } from '../lib/state.svelte';
  import { newProject, categoriesOf, categoryLabel, filterTemplates } from '../lib/templates.svelte';
  import { handledByInputMethod, pressesEnter } from '../lib/keys';
  import {
    changePlace,
    chooseTemplate,
    createProject,
    moveTemplateSelection,
    startBack,
    templateFilterChanged,
  } from '../lib/start-controller.svelte';
  import { placeText, startScreen } from '../lib/start.svelte';
  import { words } from '../lib/words';

  const catalog = $derived(newProject.catalog ?? []);
  const shown = $derived(filterTemplates(catalog, newProject.category, newProject.query));
  const categories = $derived(categoriesOf(catalog));
  const chosen = $derived(catalog.find((template) => template.id === newProject.selectedId) ?? null);
  const isBusy = $derived(newProject.creating || app.opening !== null);

  function onSearchKey(event: KeyboardEvent) {
    if (handledByInputMethod(event)) return; // an input method is composing: not ours to answer (R7.4)
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      moveTemplateSelection(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      moveTemplateSelection(-1);
    } else if (pressesEnter(event) && chosen && shown.includes(chosen)) {
      event.preventDefault();
      chooseTemplate(chosen.id);
    }
  }

  function onAnswerKey(event: KeyboardEvent) {
    if (!pressesEnter(event)) return;
    event.preventDefault();
    void createProject();
  }
</script>

{#if newProject.step === 'choose'}
  <h1 tabindex="-1">{words.newTitle}</h1>
  <div class="finding">
    <input
      data-first
      type="search"
      placeholder={words.searchTemplates}
      aria-label={words.searchTemplatesLabel}
      bind:value={newProject.query}
      oninput={templateFilterChanged}
      onkeydown={onSearchKey}
      spellcheck="false"
      autocomplete="off"
    />
    <div class="kinds" role="group" aria-label={words.kindOfDocument}>
      {#each ['all', ...categories] as category (category)}
        <button
          class="kind"
          aria-pressed={newProject.category === category}
          onclick={() => {
            newProject.category = category;
            templateFilterChanged();
          }}
        >
          {categoryLabel(category)}
        </button>
      {/each}
    </div>
  </div>

  {#if newProject.loadError}
    <p>{words.templatesCannotBeShown} {newProject.loadError}</p>
  {:else if newProject.catalog !== null && shown.length === 0}
    <p>{words.noTemplateMatches(newProject.query)}</p>
  {/if}
  <ul class="cards" role="list">
    {#each shown as template (template.id)}
      <li>
        <button class="card" class:chosen={template.id === newProject.selectedId} onclick={() => chooseTemplate(template.id)}>
          <img src={template.previewUrl} alt="" />
          <span class="text">
            <span class="name">{template.name}</span>
            <span class="muted">{template.description}</span>
          </span>
        </button>
      </li>
    {/each}
  </ul>
{:else if chosen}
  <h1 tabindex="-1">{words.newTitleOf(chosen.name)}</h1>
  <p class="muted">{chosen.description}</p>
  <div class="questions">
    <img src={chosen.previewUrl} alt={words.previewOf(chosen.name)} />
    <div class="answers">
      {#each chosen.fields as field, i (field.id)}
        <label for="new-{field.id}">{field.label}</label>
        <input
          id="new-{field.id}"
          type="text"
          data-first={i === 0 ? '' : undefined}
          placeholder={field.example}
          bind:value={newProject.values[field.id]}
          onkeydown={onAnswerKey}
          readonly={newProject.creating}
          spellcheck="false"
          autocomplete="off"
        />
      {/each}
      {#if startScreen.preferences}
        <span id="new-place-label" class="label">{words.keptIn}</span>
        <div class="place">
          <span id="new-place">{placeText(startScreen.preferences.place.parts)}</span>
          <button aria-disabled={isBusy} aria-describedby="new-place-label new-place" onclick={() => !isBusy && void changePlace()}>
            {words.change}
          </button>
        </div>
      {/if}
      <div class="go">
        <button aria-disabled={isBusy} onclick={startBack}>{words.back}</button>
        <button class="primary" aria-disabled={isBusy} onclick={() => void createProject()}>
          {newProject.creating ? words.creating : words.create}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .finding {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin: 20px 0;
  }
  .finding input {
    flex: 1 1 220px;
    max-width: 320px;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .kind[aria-pressed='true'] {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    font-weight: 600;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(300px, 100%), 1fr));
    gap: 12px;
    max-width: 1000px;
  }
  .card {
    display: flex;
    gap: 16px;
    align-items: center;
    width: 100%;
    height: 100%;
    padding: 12px;
    text-align: left;
  }
  .card.chosen {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .card img {
    flex: none;
    width: 72px;
    background: white;
    border: 1px solid var(--border);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .questions {
    display: flex;
    flex-wrap: wrap;
    gap: 32px;
    margin-top: 20px;
  }
  .questions > img {
    flex: none;
    width: 220px;
    align-self: flex-start;
    background: white;
    border: 1px solid var(--border);
  }
  .answers {
    flex: 1 1 320px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 560px;
  }
  label:not(:first-child),
  .label {
    margin-top: 12px;
  }
  label,
  .label {
    font-weight: 600;
  }
  .place {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    overflow-wrap: anywhere;
  }
  .go {
    display: flex;
    gap: 12px;
    margin-top: 20px;
  }
</style>
