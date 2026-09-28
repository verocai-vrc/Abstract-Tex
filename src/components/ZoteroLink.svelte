<script lang="ts">
  // "Link a Zotero collection" (S8.2), opened from the status bar's Zotero button once detection
  // says `ready`. Flat list of collections (library name as a header, not a selectable row —
  // Better BibTeX's own personal-library "collection" is really the whole library, which this
  // picker does not offer linking as a unit, only real collections) with nesting shown by indent.
  import { app } from '../lib/state.svelte';
  import { bibliography } from '../lib/bibliography.svelte';
  import { linkZoteroCollection, listZoteroLibraries, unlinkBibFile } from '../lib/controller.svelte';
  import type { ZoteroCollection, ZoteroLibrary } from '../lib/ipc';

  let linking = $state<string | null>(null);

  $effect(() => {
    if (!app.zoteroLinkVisible) return;
    void listZoteroLibraries();
  });

  function close() {
    app.zoteroLinkVisible = false;
    bibliography.zoteroLibraries = null;
  }

  async function pick(collection: ZoteroCollection) {
    linking = collection.path;
    await linkZoteroCollection(collection);
    linking = null;
    app.zoteroLinkVisible = false;
  }

  // Depth-first flattening with a depth number, so the template stays one `{#each}` rather than
  // a recursive component for what is, at most, a few dozen rows.
  function flatten(collections: ZoteroCollection[], depth = 0): Array<{ collection: ZoteroCollection; depth: number }> {
    return collections.flatMap((collection) => [{ collection, depth }, ...flatten(collection.children, depth + 1)]);
  }

  function rows(library: ZoteroLibrary) {
    return flatten(library.collections);
  }
</script>

{#if app.zoteroLinkVisible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-label="Link a Zotero collection">
      <header>
        <span>Link a Zotero collection</span>
        <button class="ghost" onclick={close} aria-label="Close">✕</button>
      </header>

      {#if bibliography.linkedFiles.length > 0}
        <!-- S8.7: what is already linked, each with a way back out. -->
        <ul class="linked">
          <li class="library-name">Linked to this project</li>
          {#each bibliography.linkedFiles as path (path)}
            <li class="linked-row">
              <span>{path}</span>
              <button class="ghost" onclick={() => void unlinkBibFile(path)}>Unlink</button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if bibliography.zoteroLibraries === null}
        <p class="muted">Asking Zotero…</p>
      {:else if bibliography.zoteroLibraries.length === 0}
        <p class="muted">No libraries found.</p>
      {:else}
        <ul>
          {#each bibliography.zoteroLibraries as library (library.id)}
            <li class="library-name">{library.name}</li>
            {#each rows(library) as { collection, depth } (collection.path)}
              <li>
                <button
                  class="collection"
                  style={`padding-left: ${12 + depth * 16}px`}
                  disabled={linking !== null}
                  onclick={() => void pick(collection)}
                >
                  {collection.name}
                  {#if linking === collection.path}<span class="muted"> · linking…</span>{/if}
                </button>
              </li>
            {:else}
              <li class="muted" style="padding-left: 12px">No collections in this library.</li>
            {/each}
          {/each}
        </ul>
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
    padding-top: 10vh;
  }
  .dialog {
    width: min(420px, 90vw);
    max-height: 60vh;
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
  ul {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow-y: auto;
  }
  .linked {
    flex: none;
    border-bottom: 1px solid var(--border);
  }
  .linked-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px 12px;
    font-size: 13px;
  }
  .library-name {
    padding: 6px 12px 2px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
  }
  .collection {
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
  .collection:hover:not(:disabled) {
    background: var(--bg-selected);
  }
  .collection:disabled {
    cursor: default;
    opacity: 0.6;
  }
  p.muted {
    padding: 12px;
    margin: 0;
    color: var(--fg-muted);
    font-size: 13px;
  }
</style>
