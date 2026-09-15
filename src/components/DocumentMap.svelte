<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { goToOutlineItem } from '../lib/controller.svelte';
  import type { OutlineItem } from '../lib/outline';

  /** A short text prefix so the kind is legible without colour alone — sections lean on their
   * own indent instead, the way a table of contents already reads without one. */
  function prefix(item: OutlineItem): string {
    switch (item.kind) {
      case 'figure':
        return 'Fig';
      case 'table':
        return 'Tbl';
      case 'label':
        return '@';
      case 'todo':
        return 'TODO';
      case 'section':
        return '';
    }
  }

  /** Sections indent by nesting level (DESIGN.md §5.3's outline); every other kind sits at one
   * fixed indent, since none of them nest under one another the way headings do. */
  function indent(item: OutlineItem): number {
    return item.kind === 'section' ? 8 + item.level * 14 : 22;
  }
</script>

<div class="doc-map">
  <div class="head"><span class="label">Outline</span></div>
  {#if app.outline.length === 0}
    <p class="hint">No sections yet.</p>
  {:else}
    <ul class="outline" role="list">
      {#each app.outline as item, i (i)}
        <li>
          <button
            class="row"
            class:todo={item.kind === 'todo'}
            style:padding-left="{indent(item)}px"
            onclick={() => goToOutlineItem(item)}
            title={item.title}
          >
            {#if prefix(item)}<span class="kind">{prefix(item)}</span>{/if}
            <span class="title">{item.title || '(untitled)'}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .doc-map {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    border-top: 1px solid var(--border);
  }
  .head {
    padding: 6px 8px 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .outline {
    list-style: none;
    margin: 0;
    padding: 0 0 8px;
    overflow-y: auto;
    min-height: 0;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    width: 100%;
    padding: 3px 10px 3px 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--bg-hover);
  }
  .row:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .kind {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--fg-muted);
  }
  .row.todo .kind,
  .row.todo .title {
    color: var(--warn);
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    color: var(--fg-muted);
    padding: 4px 12px 8px;
    margin: 0;
  }
</style>
