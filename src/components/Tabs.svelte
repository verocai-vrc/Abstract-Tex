<script lang="ts">
  // The tab strip for open documents (S2.3). One tab per open file, in the order it was
  // opened; each keeps its own Y.Doc alive whether or not it is the active one.
  import { app } from '../lib/state.svelte';
  import { closeTab, openFile } from '../lib/controller.svelte';
  import { baseName } from '../lib/paths';

  function onKeydown(event: KeyboardEvent, path: string) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    void openFile(path);
  }

  function onClose(event: MouseEvent, path: string) {
    // Without this, clicking × also bubbles up to the tab and re-selects the very tab being
    // closed a moment before it disappears.
    event.stopPropagation();
    void closeTab(path);
  }
</script>

{#if app.openTabs.length > 0}
  <div class="tabs" role="tablist" aria-label="Open files">
    {#each app.openTabs as path (path)}
      <div
        class="tab"
        class:active={path === app.activePath}
        role="tab"
        tabindex="0"
        aria-selected={path === app.activePath}
        title={path}
        onclick={() => void openFile(path)}
        onkeydown={(e) => onKeydown(e, path)}
      >
        <span class="name">{baseName(path)}</span>
        {#if app.dirtyPaths.has(path)}<span class="dot" aria-hidden="true"></span>{/if}
        <button class="close" title="Close" aria-label={`Close ${baseName(path)}`} onclick={(e) => onClose(e, path)}>
          ×
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    flex-shrink: 0;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 6px 6px 12px;
    border-right: 1px solid var(--border);
    font-size: 12px;
    color: var(--fg-muted);
    white-space: nowrap;
    cursor: pointer;
    user-select: none;
  }
  .tab.active {
    color: var(--fg);
    background: var(--bg-editor);
  }
  .tab:not(.active):hover {
    background: var(--bg-hover);
  }
  .tab .name {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab .dot {
    width: 6px;
    height: 6px;
    flex-shrink: 0;
    border-radius: 50%;
    background: currentColor;
  }
  .tab .close {
    border: none;
    background: transparent;
    color: inherit;
    font-size: 13px;
    line-height: 1;
    padding: 1px 4px;
    border-radius: 3px;
    cursor: pointer;
  }
  .tab .close:hover {
    background: var(--bg-selected);
  }
</style>
