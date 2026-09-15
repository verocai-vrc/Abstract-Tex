<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { createFile, openFile, setRootFile } from '../lib/controller.svelte';
  import TreeItem from './TreeItem.svelte';
  import DocumentMap from './DocumentMap.svelte';

  async function newFile() {
    const name = window.prompt('New file name (relative to the project):', 'sections/new.tex');
    if (name) await createFile(name.trim());
  }
</script>

<aside class="sidebar">
  {#if app.project}
    <div class="files-section">
      <div class="sidebar-head">
        <span class="label">Files</span>
        <button class="ghost" title="New file" onclick={newFile}>+</button>
      </div>
      <ul class="tree" role="tree">
        {#each app.project.tree as node (node.path)}
          <TreeItem
            {node}
            depth={0}
            activePath={app.activePath}
            rootFile={app.project.rootFile}
            dirtyPaths={app.dirtyPaths}
            onOpen={(p) => void openFile(p)}
            onSetRoot={(p) => void setRootFile(p)}
          />
        {/each}
      </ul>
    </div>
    <DocumentMap />
  {:else}
    <p class="hint">Open a folder to see its files.</p>
  {/if}
</aside>

<style>
  .sidebar-head {
    display: flex;
    align-items: center;
    padding: 2px 8px 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .sidebar-head .label {
    flex: 1;
  }
  .hint {
    color: var(--fg-muted);
    padding: 8px 12px;
    margin: 0;
  }
</style>
