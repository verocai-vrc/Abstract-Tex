<script lang="ts">
  // `Ctrl P`: type part of a file name, arrow to it, Enter (S2.4). Keyboard first, DESIGN.md §2.
  // This is the seed of the command palette (S4.3): the list will grow actions and sections,
  // the input, ranking and key handling stay as they are here.
  import { tick } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { quickOpenPick } from '../lib/controller.svelte';
  import { highlightMatch, rank } from '../lib/fuzzy';
  import { listFiles } from '../lib/paths';

  let query = $state('');
  let selected = $state(0);
  let input: HTMLInputElement | undefined = $state();

  // With nothing typed, the files already open in tabs come first — the author is most likely
  // switching between them — then the rest of the tree in the order the sidebar shows it.
  const candidates = $derived.by(() => {
    const all = app.project ? listFiles(app.project.tree) : [];
    const open = app.openTabs.filter((path) => all.includes(path));
    return [...open, ...all.filter((path) => !open.includes(path))];
  });
  const results = $derived(rank(query, candidates, (path) => path).slice(0, 50));

  // Reset and focus every time the list is shown; a stale query from last time would hide the
  // file the author is about to type the first letter of.
  $effect(() => {
    if (!app.quickOpenVisible) return;
    query = '';
    selected = 0;
    void tick().then(() => input?.focus());
  });

  // A new query can shrink the list under the selection.
  $effect(() => {
    if (selected >= results.length) selected = Math.max(0, results.length - 1);
  });

  function onKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        selected = Math.min(selected + 1, results.length - 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        selected = Math.max(selected - 1, 0);
        break;
      case 'Enter': {
        event.preventDefault();
        const chosen = results[selected];
        if (chosen) void quickOpenPick(chosen.item);
        break;
      }
      case 'Escape':
        event.preventDefault();
        app.quickOpenVisible = false;
        break;
    }
  }

</script>

{#if app.quickOpenVisible}
  <!-- A click on the backdrop itself closes the list; clicks inside the palette bubble up here
       too, so check the target. Escape is handled on the input, which has focus. -->
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.quickOpenVisible = false)}>
    <div class="palette" role="dialog" aria-label="Go to file">
      <input
        bind:this={input}
        bind:value={query}
        onkeydown={onKeydown}
        type="text"
        placeholder="Go to file…"
        spellcheck="false"
        autocomplete="off"
        aria-controls="quick-open-list"
        aria-activedescendant={results.length ? `quick-open-${selected}` : undefined}
      />
      <ul id="quick-open-list" role="listbox">
        {#each results as result, i (result.item)}
          <!-- The keyboard path is the input above (arrows + Enter via aria-activedescendant),
               so the rows themselves need no key handler. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id={`quick-open-${i}`}
            role="option"
            aria-selected={i === selected}
            class:selected={i === selected}
            onmousemove={() => (selected = i)}
            onclick={() => void quickOpenPick(result.item)}
          >
            {#each highlightMatch(result.item, result.match.positions) as segment, j (j)}
              {#if segment.hit}<mark>{segment.text}</mark>{:else}{segment.text}{/if}
            {/each}
            {#if app.dirtyPaths.has(result.item)}<span class="dot" aria-label="unsaved"></span>{/if}
          </li>
        {:else}
          <li class="empty">No file matches “{query}”.</li>
        {/each}
      </ul>
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
    padding-top: 10vh;
  }
  .palette {
    width: min(560px, 90vw);
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 10px 12px;
    font: inherit;
    font-size: 14px;
    color: var(--fg);
    background: var(--bg-editor);
    border: none;
    border-bottom: 1px solid var(--border);
    outline: none;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    max-height: 50vh;
    overflow-y: auto;
    font-family: var(--font-mono);
    font-size: 12.5px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  li.selected {
    background: var(--bg-selected);
  }
  li.empty {
    color: var(--fg-muted);
    cursor: default;
    font-family: var(--font-ui);
  }
  mark {
    background: none;
    color: var(--accent);
    font-weight: 600;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
    flex-shrink: 0;
  }
</style>
