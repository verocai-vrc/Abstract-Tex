<script lang="ts">
  // "New project from template" (S11.11, DESIGN.md §6 "Start a document"): the front door for a
  // person with no .tex file yet. Two steps — the templates as a list you can search and filter,
  // then the two questions — and one press, which asks where to put it, writes it, opens it and
  // builds it. Esc steps back; nothing is written until *Create*.
  //
  // Everything is reachable from the keyboard: the search box has focus when the window opens, ↑ ↓
  // move the highlight, Enter chooses, Enter in a question creates, Esc goes back.
  import { newProject, categoriesOf, categoryLabel, filterTemplates } from '../lib/templates.svelte';
  import { modalFocus } from '../lib/dialog';
  import {
    chooseTemplate,
    closeNewProjectWindow,
    createProject,
    moveTemplateSelection,
    newProjectBack,
    templateFilterChanged,
  } from '../lib/controller.svelte';

  const catalog = $derived(newProject.catalog ?? []);
  const shown = $derived(filterTemplates(catalog, newProject.category, newProject.query));
  const categories = $derived(categoriesOf(catalog));
  const chosen = $derived(catalog.find((template) => template.id === newProject.selectedId) ?? null);

  // Put the cursor where typing is wanted whenever the step (or the window) changes.
  let searchBox: HTMLInputElement | undefined = $state();
  let questions: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (!newProject.visible) return;
    if (newProject.step === 'choose') searchBox?.focus();
    else questions?.querySelector('input')?.focus();
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      newProjectBack();
      return;
    }
    if (newProject.step === 'choose') {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        moveTemplateSelection(1);
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        moveTemplateSelection(-1);
      } else if (event.key === 'Enter' && event.target instanceof HTMLInputElement && chosen && shown.includes(chosen)) {
        event.preventDefault();
        chooseTemplate(chosen.id);
      }
    } else if (event.key === 'Enter' && event.target instanceof HTMLInputElement) {
      event.preventDefault();
      void createProject();
    }
  }
</script>

{#if newProject.visible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && newProjectBack()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="New project from template" tabindex="-1" use:modalFocus onkeydown={onKeydown}>
      <header>
        <span>{newProject.step === 'choose' ? 'New project' : chosen?.name ?? 'New project'}</span>
        <button class="ghost" onclick={closeNewProjectWindow} aria-label="Close">✕</button>
      </header>

      {#if newProject.step === 'choose'}
        <div class="search">
          <input
            bind:this={searchBox}
            type="search"
            placeholder="Search templates…"
            aria-label="Search templates"
            bind:value={newProject.query}
            oninput={templateFilterChanged}
            spellcheck="false"
            autocomplete="off"
          />
        </div>
        <div class="chips" role="group" aria-label="Kind of document">
          {#each ['all', ...categories] as category (category)}
            <button
              class="chip"
              class:on={newProject.category === category}
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

        {#if newProject.loadError}
          <p class="muted error">{newProject.loadError}</p>
        {:else if newProject.catalog === null}
          <p class="muted">Loading…</p>
        {:else if shown.length === 0}
          <p class="muted">No template matches “{newProject.query}”. Try fewer words.</p>
        {:else}
          <ul aria-label="Templates">
            {#each shown as template (template.id)}
              <li>
                <button
                  class="card"
                  class:chosen={template.id === newProject.selectedId}
                  onclick={() => chooseTemplate(template.id)}
                >
                  <img src={template.previewUrl} alt="" width="48" />
                  <span class="text">
                    <span class="name">{template.name}</span>
                    <span class="muted">{template.description}</span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        <footer>
          <span class="muted hint">↑ ↓ to move · Enter to choose · Esc to close</span>
          <span class="spacer"></span>
          <button class="ghost" onclick={closeNewProjectWindow}>Cancel</button>
          <button class="primary" disabled={!chosen || !shown.includes(chosen)} onclick={() => chosen && chooseTemplate(chosen.id)}>
            Next
          </button>
        </footer>
      {:else if chosen}
        <div class="questions" bind:this={questions}>
          <img class="preview" src={chosen.previewUrl} alt="First page of the {chosen.name} template" />
          <div class="fields">
            <p class="muted">{chosen.description}</p>
            {#each chosen.fields as field (field.id)}
              <label for="new-project-{field.id}">{field.label}</label>
              <input
                id="new-project-{field.id}"
                type="text"
                placeholder={field.example}
                bind:value={newProject.values[field.id]}
                disabled={newProject.creating}
                spellcheck="false"
                autocomplete="off"
              />
            {/each}
          </div>
        </div>

        <footer>
          {#if newProject.error}<p class="error">{newProject.error}</p>{/if}
          <span class="spacer"></span>
          <button class="ghost" disabled={newProject.creating} onclick={newProjectBack}>Back</button>
          <button class="primary" disabled={newProject.creating} onclick={() => void createProject()}>
            {newProject.creating ? 'Creating…' : 'Create…'}
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
    width: min(540px, 92vw);
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
  input {
    font: inherit;
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
  }
  .search {
    display: flex;
    padding: 10px 12px 4px;
  }
  .search input {
    flex: 1;
  }
  /* An example is a hint, not an answer: the default placeholder is as bright as typed text in the
     dark theme, and "Your name" looked like something already filled in. */
  input::placeholder {
    color: var(--fg-muted);
    opacity: 0.55;
    font-style: italic;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 12px 8px;
  }
  .chip {
    padding: 2px 10px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .chip.on {
    background: var(--bg-selected);
    color: var(--fg);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
  }
  .card {
    display: flex;
    gap: 12px;
    align-items: center;
    width: 100%;
    box-sizing: border-box;
    padding: 6px 12px;
    text-align: left;
    background: none;
    border: none;
    font: inherit;
    font-size: 13px;
    color: var(--fg);
    cursor: pointer;
  }
  .card:hover,
  .card.chosen {
    background: var(--bg-selected);
  }
  .card img {
    flex: none;
    border: 1px solid var(--border);
    background: white;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .questions {
    display: flex;
    gap: 16px;
    padding: 12px;
    overflow-y: auto;
  }
  .preview {
    flex: none;
    width: 150px;
    align-self: flex-start;
    border: 1px solid var(--border);
    background: white;
  }
  .fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .fields .muted {
    padding: 0 0 8px;
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
  .hint {
    padding: 0;
    font-size: 12px;
  }
  .muted {
    margin: 0;
    padding: 4px 12px;
    color: var(--fg-muted);
    font-size: 13px;
  }
  .error {
    color: var(--error);
  }
</style>
