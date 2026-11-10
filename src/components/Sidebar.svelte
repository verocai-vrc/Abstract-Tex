<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { createFile, openFile, setRootFile } from '../lib/controller.svelte';
  import TreeItem from './TreeItem.svelte';
  import DocumentMap from './DocumentMap.svelte';

  // The name is typed into a field under the header rather than into the browser's own
  // `window.prompt` dialog: that one is titled "JavaScript - http://localhost:1420", is not
  // reliably supported by every webview this app ships on, and cannot be styled or tested.
  let naming = $state(false);
  let draftName = $state('');

  function startNaming() {
    draftName = '';
    naming = true;
  }

  async function finishNaming() {
    const name = draftName.trim();
    naming = false;
    if (name) await createFile(name);
  }

  /** Svelte action: focus the field when it appears, so the keyboard is in the box without a
   * click. The suggestion is a placeholder, not pre-filled text: selecting pre-filled text right
   * after focusing it did not hold in the webview, and typing then appended to the suggestion. */
  function focusOnCreate(node: HTMLInputElement) {
    node.focus();
  }

  function onNameKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void finishNaming();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      naming = false;
    }
  }
</script>

<aside class="sidebar">
  {#if app.project}
    <div class="files-section">
      <div class="sidebar-head">
        <span class="label">Files</span>
        <button class="ghost" title="New file" aria-label="New file" onclick={startNaming}>+</button>
      </div>
      {#if naming}
        <input
          class="new-file-name"
          use:focusOnCreate
          bind:value={draftName}
          placeholder="sections/new.tex"
          aria-label="New file name, relative to the project"
          spellcheck="false"
          onkeydown={onNameKeydown}
          onblur={() => (naming = false)}
        />
      {/if}
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
  .new-file-name {
    display: block;
    box-sizing: border-box;
    width: calc(100% - 20px);
    margin: 0 10px 6px;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-editor);
    border: 1px solid var(--accent);
    border-radius: 4px;
  }
  .hint {
    color: var(--fg-muted);
    padding: 8px 12px;
    margin: 0;
  }
</style>
