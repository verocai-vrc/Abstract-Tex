<script lang="ts">
  // "Two paragraphs and a choice" (S11.2b, DESIGN.md §5.7) — what `Editor.svelte` shows instead of
  // CodeMirror while the active tab is a conflicted file. The real `<<<<<<<` markers S11.2a's
  // `sync` wrote are real, on disk, for a terminal's sake; this is the one place in the app that
  // reads them, and it reads them once, to build this view, never to put them on screen.
  //
  // Reads the file itself rather than the tab's own `Y.Doc` — `openFile` still made one, but it
  // holds the raw marker text nobody is meant to see, and `resolveMergeConflict` discards it
  // rather than ever syncing it back to disk.
  import { hunkCount, parseConflictMarkers, reassembleConflictSections, type ConflictSection } from '../lib/conflict';
  import { resolveMergeConflict } from '../lib/controller.svelte';
  import { ipc } from '../lib/ipc';
  import { app } from '../lib/state.svelte';

  let sections = $state.raw<ConflictSection[] | null>(null);
  /** One entry per hunk, in order — what each hunk's editable box currently holds. Starts as
   * *Yours*, the same default a plain Git checkout already left in the working tree. */
  let resolutions = $state<string[]>([]);
  let loadError = $state<string | null>(null);
  let resolving = $state(false);
  /** The path the two above were built from, so a tab switch between two conflicted files reloads
   * rather than showing the previous one's hunks under the new one's name. */
  let loadedPath: string | null = null;

  $effect(() => {
    const path = app.activePath;
    if (!path || path === loadedPath) return;
    loadedPath = path;
    sections = null;
    loadError = null;
    resolving = false;
    void load(path);
  });

  async function load(path: string): Promise<void> {
    let text: string;
    try {
      text = await ipc.readFile(path);
    } catch (error) {
      loadError = `Could not read ${path}: ${String(error)}`;
      return;
    }
    const parsed = parseConflictMarkers(text);
    if (!parsed) {
      loadError = `${path} has a conflict this view cannot show as paragraphs — likely a file added or removed on both sides, not a line both sides edited. Resolve it in a terminal, then stage it here.`;
      return;
    }
    sections = parsed;
    resolutions = parsed.filter((section) => section.kind === 'conflict').map((hunk) => hunk.ours);
  }

  /** Every section, with each hunk's own index into `resolutions` attached — computed once per
   * parse rather than searched for on every keystroke in its box. */
  const renderItems = $derived.by(() => {
    let hunkIndex = -1;
    return (sections ?? []).map((section) =>
      section.kind === 'clean' ? { kind: 'clean' as const, text: section.text } : { kind: 'conflict' as const, index: ++hunkIndex, ours: section.ours, theirs: section.theirs },
    );
  });

  const remaining = $derived(sections ? hunkCount(sections) : 0);

  async function markResolved(): Promise<void> {
    if (!sections || !app.activePath || resolving) return;
    resolving = true;
    await resolveMergeConflict(app.activePath, reassembleConflictSections(sections, resolutions));
    resolving = false;
  }
</script>

<div class="conflict-resolver">
  {#if loadError}
    <p class="hint error">{loadError}</p>
  {:else if !sections}
    <p class="hint">Reading the conflict…</p>
  {:else}
    <div class="conflict-head">
      <span>{remaining === 1 ? '1 conflict in this file' : `${remaining} conflicts in this file`}</span>
      <button class="primary" disabled={resolving} onclick={() => void markResolved()}>
        {resolving ? 'Resolving…' : 'Mark Resolved'}
      </button>
    </div>
    <div class="conflict-body">
      {#each renderItems as item, itemIndex (itemIndex)}
        {#if item.kind === 'clean'}
          <pre class="clean-run">{item.text}</pre>
        {:else}
          <div class="hunk">
            <div class="hunk-side">
              <div class="hunk-label">
                Yours
                <button class="ghost" onclick={() => (resolutions[item.index] = item.ours)}>Keep mine</button>
              </div>
              <pre class="hunk-text">{item.ours}</pre>
            </div>
            <div class="hunk-side">
              <div class="hunk-label">
                Theirs
                <button class="ghost" onclick={() => (resolutions[item.index] = item.theirs)}>Keep theirs</button>
              </div>
              <pre class="hunk-text">{item.theirs}</pre>
            </div>
            <div class="hunk-side">
              <div class="hunk-label">What the file will have</div>
              <textarea class="hunk-edit" rows="3" bind:value={resolutions[item.index]}></textarea>
            </div>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .conflict-resolver {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .conflict-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    position: sticky;
    top: 0;
  }
  .conflict-body {
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .clean-run {
    margin: 0;
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--fg-muted);
  }
  .hunk {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--warn);
    border-radius: var(--radius);
  }
  .hunk-side {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .hunk-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .hunk-text {
    margin: 0;
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: 13px;
    padding: 4px 6px;
    background: var(--bg-editor);
    border-radius: var(--radius);
  }
  .hunk-edit {
    font-family: var(--font-mono);
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
    resize: vertical;
  }
</style>
