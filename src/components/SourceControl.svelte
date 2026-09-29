<script lang="ts">
  // The Source Control view (S10.3a, DESIGN.md §6), the second tenant of the left pane. Copied
  // from VS Code top to bottom, and this loop builds the bottom half of it: the file lists and
  // their three verbs. The commit box, the Commit button and the Graph section are S10.3b.
  //
  // Everything here reads `git` and calls the controller; the panel itself decides nothing. Two
  // states are deliberately sentences rather than empty space — no project, and a folder that is
  // not a Git repository — because an empty panel would look like a clean tree, which is a
  // different and much more reassuring thing than "nobody asked Git anything".
  import { discardChange, openFile, refreshGitStatus, stageChange, unstageChange } from '../lib/controller.svelte';
  import { git, type ChangeRow } from '../lib/git.svelte';
  import { app } from '../lib/state.svelte';

  /** A deleted file has nothing to open, so its row is not a button. §6's "a click opens a diff"
   * arrives with the merge view (S11.6); until then a click opens the file itself. */
  function openRow(row: ChangeRow) {
    if (row.kind !== 'deleted') void openFile(row.path);
  }
</script>

{#snippet row(item: ChangeRow, staged: boolean)}
  <li class="change-row">
    <button class="change-open" title={item.renamedFrom ? `Renamed from ${item.renamedFrom}` : item.path} onclick={() => openRow(item)}>
      <span class="change-name" class:gone={item.kind === 'deleted'}>{item.name}</span>
      {#if item.dir}<span class="change-dir">{item.dir}</span>{/if}
    </button>
    <span class="change-actions">
      {#if staged}
        <button class="ghost" title="Unstage this file" onclick={() => void unstageChange(item.path)}>−</button>
      {:else}
        <button class="ghost" title="Discard changes to this file" onclick={() => void discardChange(item.path, item.kind === 'untracked')}>
          ↺
        </button>
        <button class="ghost" title="Stage this file" onclick={() => void stageChange(item.path)}>+</button>
      {/if}
    </span>
    <span class={`change-letter kind-${item.kind}`} title={item.kind}>{item.letter}</span>
  </li>
{/snippet}

{#snippet section(title: string, rows: ChangeRow[], staged: boolean)}
  {#if rows.length > 0}
    <div class="sc-section">
      <div class="sidebar-head">
        <span class="label">{title}</span>
        <span class="count">{rows.length}</span>
      </div>
      <ul class="change-list">
        {#each rows as item (item.path)}
          {@render row(item, staged)}
        {/each}
      </ul>
    </div>
  {/if}
{/snippet}

<aside class="sidebar source-control">
  {#if !app.project}
    <p class="hint">Open a folder to see its changes.</p>
  {:else if !git.isRepository}
    <p class="hint">
      This folder is not a Git repository, so there is no history to show. Running
      <code>git init</code> in it is enough; doing that from here, with a remote, arrives in a
      later version.
    </p>
    {#if git.error}<p class="hint error">{git.error}</p>{/if}
  {:else}
    <div class="sidebar-head">
      <span class="label">Source Control</span>
      <button class="ghost" title="Refresh" onclick={() => void refreshGitStatus()}>⟳</button>
    </div>

    {#if git.error}<p class="hint error">{git.error}</p>{/if}
    {#if git.lastDiscard}<p class="hint">{git.lastDiscard}</p>{/if}

    {#if git.conflictedRows.length > 0}
      <!-- A conflicted path is in no other list, so without this section it would vanish from
           the one panel whose job is to say what changed. S11.2 replaces it with two paragraphs. -->
      {@render section('Merge Changes', git.conflictedRows, false)}
      <p class="hint">
        Edit the file to resolve it, then stage it. A conflict shown as two paragraphs rather than
        as markers arrives in a later version.
      </p>
    {/if}
    {@render section('Staged Changes', git.stagedRows, true)}
    {@render section('Changes', git.unstagedRows, false)}

    {#if git.changedCount === 0}
      <p class="hint">No changes. Everything here matches the last commit.</p>
    {/if}
  {/if}
</aside>

<style>
  .source-control {
    overflow-y: auto;
  }
  .sidebar-head {
    display: flex;
    align-items: center;
    padding: 2px 6px 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .sidebar-head .label {
    flex: 1;
  }
  .sidebar-head .count {
    padding: 0 4px;
  }
  .hint {
    color: var(--fg-muted);
    padding: 4px 12px 8px;
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
  }
  .hint.error {
    color: var(--error);
  }
  .sc-section {
    margin-bottom: 6px;
  }
  .change-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .change-row {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px 0 10px;
  }
  .change-row:hover {
    background: var(--bg-hover);
  }
  .change-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 3px 0;
    cursor: pointer;
  }
  .change-name {
    white-space: nowrap;
  }
  /* A deleted file is still worth naming, but it is not there to be opened. */
  .change-name.gone {
    text-decoration: line-through;
  }
  .change-dir {
    color: var(--fg-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Hover actions, as VS Code has them: present in the layout only when the row is hovered or
     one of its own buttons has focus, so a keyboard user can still reach them. */
  .change-actions {
    display: flex;
    gap: 2px;
    visibility: hidden;
  }
  .change-row:hover .change-actions,
  .change-actions:focus-within {
    visibility: visible;
  }
  .change-actions button {
    padding: 0 4px;
    line-height: 1.2;
  }
  .change-letter {
    width: 1.2em;
    text-align: center;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
  }
  .kind-modified,
  .kind-renamed {
    color: var(--warn);
  }
  .kind-added,
  .kind-untracked {
    color: var(--ok);
  }
  .kind-deleted,
  .kind-conflicted {
    color: var(--error);
  }
</style>
