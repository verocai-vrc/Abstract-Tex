<script lang="ts">
  // `Ctrl K`: every action, every open file, every section of the current file, searched
  // together (S4.3, DESIGN.md §5.3). `QuickOpen.svelte` is `Ctrl P`'s narrower file-only list and
  // keeps working unchanged; this does not replace it, it adds the wider search DESIGN.md §5.4
  // asks for. The two share `fuzzy.ts`'s `rank`/`highlightMatch` rather than each owning its own
  // copy of the ranking and highlighting logic.
  import { tick } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { goToOutlineItem, openFile } from '../lib/controller.svelte';
  import { allCommands, searchCommands, type Command } from '../lib/commands';
  import { fuzzyMatch, highlightMatch } from '../lib/fuzzy';
  import { listFiles } from '../lib/paths';

  let query = $state('');
  let selected = $state(0);
  let input: HTMLInputElement | undefined = $state();

  // Open tabs before the rest of the tree, exactly as `QuickOpen.svelte` orders its file list —
  // the author is more likely switching to a file already open than to one it has never seen.
  const fileCommands = $derived.by((): Command[] => {
    const all = app.project ? listFiles(app.project.tree) : [];
    const open = app.openTabs.filter((path) => all.includes(path));
    const ordered = [...open, ...all.filter((path) => !open.includes(path))];
    return ordered.map((path) => ({
      id: `file:${path}`,
      title: path,
      category: 'file',
      run: () => void openFile(path),
    }));
  });

  // Only the active file's outline — a palette is for getting somewhere fast, not for browsing
  // every section of every file in the project at once.
  const sectionCommands = $derived.by((): Command[] =>
    app.outline
      .filter((item) => item.kind === 'section')
      .map((item) => ({
        id: `section:${item.line}`,
        title: `Go to: ${item.title || '(untitled section)'}`,
        category: 'section',
        run: () => goToOutlineItem(item),
      })),
  );

  // Actions first, so an empty query — the state right after `Ctrl K` — leads with "every
  // action reachable" (DESIGN.md §5.3) rather than a file list `Ctrl P` already gives.
  const candidates = $derived([...allCommands(), ...fileCommands, ...sectionCommands]);
  const results = $derived(searchCommands(query, candidates).slice(0, 50));

  $effect(() => {
    if (!app.commandPaletteVisible) return;
    query = '';
    selected = 0;
    void tick().then(() => input?.focus());
  });

  $effect(() => {
    if (selected >= results.length) selected = Math.max(0, results.length - 1);
  });

  function run(command: Command): void {
    app.commandPaletteVisible = false;
    command.run();
  }

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
        if (chosen) run(chosen);
        break;
      }
      case 'Escape':
        event.preventDefault();
        app.commandPaletteVisible = false;
        break;
    }
  }

  const CATEGORY_LABEL: Record<Command['category'], string> = {
    action: 'Action',
    file: 'File',
    section: 'Section',
  };

  // `searchCommands` returns plain `Command`s, not the `Match` it ranked them with (the card's
  // signature is `(query) => Command[]`), so the positions to highlight are recomputed here — one
  // more `fuzzyMatch` per visible row (at most 50) against a title already known to match, which
  // is cheap next to the ranking pass over every candidate that already happened.
  function positionsFor(title: string): number[] {
    return fuzzyMatch(query, title)?.positions ?? [];
  }
</script>

{#if app.commandPaletteVisible}
  <!-- Same backdrop-click-to-close pattern as `QuickOpen.svelte`; see its comment for why the
       target check is needed. -->
  <div
    class="backdrop"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && (app.commandPaletteVisible = false)}
  >
    <div class="palette" role="dialog" aria-label="Command palette">
      <input
        bind:this={input}
        bind:value={query}
        onkeydown={onKeydown}
        type="text"
        placeholder="Type a command, file or section…"
        spellcheck="false"
        autocomplete="off"
        aria-controls="command-palette-list"
        aria-activedescendant={results.length ? `command-${selected}` : undefined}
      />
      <ul id="command-palette-list" role="listbox">
        {#each results as result, i (result.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id={`command-${i}`}
            role="option"
            aria-selected={i === selected}
            class:selected={i === selected}
            onmousemove={() => (selected = i)}
            onclick={() => run(result)}
          >
            <span class="category">{CATEGORY_LABEL[result.category]}</span>
            <span class="title">
              {#each highlightMatch(result.title, positionsFor(result.title)) as segment, j (j)}
                {#if segment.hit}<mark>{segment.text}</mark>{:else}{segment.text}{/if}
              {/each}
            </span>
            {#if result.shortcut}<span class="shortcut">{result.shortcut}</span>{/if}
          </li>
        {:else}
          <li class="empty">No command, file or section matches “{query}”.</li>
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
    font-size: 12.5px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
  }
  li.selected {
    background: var(--bg-selected);
  }
  li.empty {
    color: var(--fg-muted);
    cursor: default;
  }
  .category {
    flex-shrink: 0;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    width: 4.5em;
  }
  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--font-mono);
  }
  .shortcut {
    flex-shrink: 0;
    color: var(--fg-muted);
    font-size: 11px;
  }
  mark {
    background: none;
    color: var(--accent);
    font-weight: 600;
  }
</style>
