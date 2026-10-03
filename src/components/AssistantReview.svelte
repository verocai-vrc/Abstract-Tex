<script lang="ts">
  // The review of one suggestion (S12.3b, DESIGN.md §5.5): the selection with the assistant's changes
  // marked in place, each one accepted or left with a click, then *Apply*. The buffer is not touched
  // until Apply, and then in one undoable step.
  //
  // A change the citation guard refused is drawn as refused and cannot be turned on; its sentence is
  // listed below with what to do about it. What this dialog shows is a preview: the text that reaches
  // the buffer is made and checked in Rust (`assistant_apply`).
  import { applyReview, closeReview, toggleReviewHunk } from '../lib/controller.svelte';
  import { assistant } from '../lib/assistant.svelte';
  import { counts, segments, summaryLine } from '../lib/assistant-review';
  import { modalFocus } from '../lib/dialog';

  const review = $derived(assistant.review);
  const parts = $derived(review ? segments(review.original, review.hunks) : []);
  const tally = $derived(review ? counts(review.hunks, review.choices) : { accepted: 0, allowed: 0 });
  const refusals = $derived(
    review
      ? [...new Set(review.hunks.flatMap((hunk) => hunk.refusal ?? []))]
      : [],
  );

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeReview();
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) void applyReview();
  }
</script>

{#if review}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && closeReview()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Review the assistant’s suggestion" tabindex="-1" use:modalFocus onkeydown={onKeydown}>
      <header>
        <span>{review.actionLabel}</span>
        <button class="ghost" onclick={closeReview} aria-label="Close">✕</button>
      </header>

      <p class="hint">
        Click a change to accept or leave it. Nothing is written until you press Apply, and Apply can be undone with Ctrl Z.
      </p>

      <div class="text" aria-label="The selection with the suggested changes">
        {#each parts as part, position (position)}
          {#if part.kind === 'same'}{part.text}{:else}<button
              class="change"
              class:accepted={review.choices[part.index] && part.refusal === null}
              class:left={!review.choices[part.index] && part.refusal === null}
              class:refused={part.refusal !== null}
              aria-pressed={review.choices[part.index]}
              disabled={part.refusal !== null}
              title={part.refusal ? part.refusal.join(' ') : review.choices[part.index] ? 'Accepted: click to leave it' : 'Left as it was: click to accept'}
              onclick={() => toggleReviewHunk(part.index)}
              >{#if part.removed}<del>{part.removed}</del>{/if}{#if part.added}<ins>{part.added}</ins>{/if}</button
            >{/if}
        {/each}
      </div>

      {#if refusals.length > 0}
        <div class="held" role="status">
          {#each refusals as sentence (sentence)}<p>{sentence}</p>{/each}
          {#if review.unknownKeys.length > 0}
            <p>
              Add {review.unknownKeys.length === 1 ? 'that reference' : 'those references'} to your bibliography first
              (paste a DOI to cite it); the assistant cannot cite what your .bib files do not hold.
            </p>
          {/if}
        </div>
      {/if}

      {#if review.error}<p class="error" role="alert">{review.error}</p>{/if}

      <footer>
        <span class="summary">{summaryLine(review.hunks, review.choices)}</span>
        <span class="spacer"></span>
        <button class="ghost" onclick={closeReview}>Cancel</button>
        <button class="primary" disabled={tally.accepted === 0 || review.applying} onclick={() => void applyReview()}>
          Apply
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    width: min(760px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .hint {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .text {
    margin: 8px 14px;
    padding: 10px 12px;
    overflow-y: auto;
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.7;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .change {
    all: unset;
    cursor: pointer;
    border-radius: 3px;
    padding: 0 1px;
    outline-offset: 1px;
  }
  .change:focus-visible {
    outline: 2px solid var(--accent);
  }
  .change del {
    text-decoration: line-through;
    color: var(--error);
    background: rgba(220, 70, 70, 0.12);
  }
  .change ins {
    text-decoration: none;
    color: var(--ok, #4caf7a);
    background: rgba(76, 175, 122, 0.14);
  }
  /* Left as it was: the change is shown, but quiet, and the old text reads as the text. */
  .change.left del {
    text-decoration: none;
    color: inherit;
    background: none;
  }
  .change.left ins {
    opacity: 0.4;
    text-decoration: line-through;
  }
  .change.refused {
    cursor: not-allowed;
    outline: 1px dashed var(--error);
  }
  .change.refused ins {
    opacity: 0.55;
    text-decoration: line-through;
  }
  .held {
    margin: 0 14px 8px;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--error);
    border-left: 3px solid var(--error);
    background: rgba(220, 70, 70, 0.08);
  }
  .held p {
    margin: 2px 0;
  }
  .error {
    margin: 0 14px 8px;
    color: var(--error);
    font-size: 12px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
  }
  .summary {
    font-size: 12px;
    color: var(--fg-muted);
  }
  .spacer {
    flex: 1;
  }
</style>
