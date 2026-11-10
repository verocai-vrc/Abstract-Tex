<script lang="ts">
  // The Comments view (S14.3c), the fourth tenant of the left pane: who said what about which
  // words, open ones first. Comments live in the project's Git history (Rust); this view only
  // reads them and asks for the three things that change them.
  import {
    cancelComment,
    jumpToComment,
    refreshComments,
    saveComment,
    setCommentResolved,
    startComment,
  } from '../lib/controller.svelte';
  import { comments } from '../lib/comments.svelte';
  import { groupComments, problemSentence, quotePreview } from '../lib/comments';
  import { app } from '../lib/state.svelte';

  /** Svelte action: focus the element when it is created, so the keyboard is in the comment box. */
  function focusOnCreate(node: HTMLElement) {
    node.focus();
  }

  const groups = $derived(groupComments(comments.rows ?? [], app.activePath));
</script>

<aside class="sidebar comments" aria-label="Comments">
  <div class="sidebar-head">
    <span class="label">Comments</span>
    <button class="ghost" title="Comment on the selected text" aria-label="Comment on the selected text" onclick={startComment}>+</button>
  </div>

  {#if comments.draft}
    <section class="draft" aria-label="New comment">
      <p class="quote">“{quotePreview(comments.draft.quote)}”</p>
      <textarea
        use:focusOnCreate
        bind:value={comments.body}
        rows="3"
        placeholder="Write a comment…"
        aria-label="Comment"
        onkeydown={(event) => {
          if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) void saveComment();
          if (event.key === 'Escape') cancelComment();
        }}
      ></textarea>
      <div class="buttons">
        <button class="primary" disabled={!comments.body.trim() || comments.saving} onclick={() => void saveComment()}
          >Comment</button
        >
        <button class="ghost" onclick={cancelComment}>Cancel</button>
      </div>
    </section>
  {/if}

  {#if !app.project}
    <p class="hint">Open a folder to see its comments.</p>
  {:else if comments.error}
    <p class="hint">{comments.error}</p>
  {:else if comments.rows === null}
    <p class="hint">Reading comments…</p>
  {:else if comments.rows.length === 0}
    <p class="hint">No comments yet. Select some text and choose “Comment on selection…”.</p>
  {:else}
    <ul class="list">
      {#each groups.open as row (row.id)}
        {@render comment(row)}
      {/each}
    </ul>
    {#if groups.resolved.length > 0}
      <details class="resolved">
        <summary>Resolved ({groups.resolved.length})</summary>
        <ul class="list">
          {#each groups.resolved as row (row.id)}
            {@render comment(row)}
          {/each}
        </ul>
      </details>
    {/if}
    <button class="ghost refresh" onclick={() => void refreshComments()}>Refresh</button>
  {/if}
</aside>

{#snippet comment(row: import('../lib/ipc').CommentRow)}
  <li class="item" class:orphaned={row.state !== 'anchored'}>
    <button class="jump" disabled={row.state === 'fileMissing'} onclick={() => void jumpToComment(row)}>
      <span class="meta">{row.author} · {row.file}</span>
      <span class="quote">“{quotePreview(row.quote)}”</span>
      <span class="body">{row.body}</span>
      {#if problemSentence(row)}<span class="problem">{problemSentence(row)}</span>{/if}
    </button>
    <button class="ghost" onclick={() => void setCommentResolved(row, !row.resolved)}>
      {row.resolved ? 'Reopen' : 'Resolve'}
    </button>
  </li>
{/snippet}

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
  .draft {
    padding: 4px 10px 10px;
    border-bottom: 1px solid var(--border);
  }
  .draft textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
  }
  .item {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .jump {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    background: none;
    border: 0;
    padding: 0;
    color: inherit;
    cursor: pointer;
  }
  .meta,
  .quote {
    color: var(--fg-muted);
    font-size: 11px;
  }
  .problem {
    color: var(--fg-muted);
    font-size: 11px;
    font-style: italic;
  }
  .orphaned .quote {
    text-decoration: line-through;
  }
  .resolved summary {
    padding: 6px 12px;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .item > .ghost {
    align-self: flex-start;
    padding-left: 0;
  }
  .refresh {
    margin: 6px 10px;
    align-self: flex-start;
  }
</style>
