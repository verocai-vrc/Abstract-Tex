<script lang="ts">
  import type { EditorView } from '@codemirror/view';
  import { app } from '../lib/state.svelte';
  import { resolveConflict, saveNow, triggerCompile } from '../lib/controller.svelte';
  import { createEditor, goToLine } from '../lib/editor/setup';
  import Drawer from './Drawer.svelte';

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | null = null;

  // Rebuild the editor whenever the open document changes. CodeMirror owns everything inside
  // `host`; Svelte only provides the element.
  $effect(() => {
    const doc = app.activeDoc;
    if (!host || !doc) return;
    view = createEditor(host, doc, {
      onSave: () => void saveNow(),
      onCompile: () => void (async () => {
        await saveNow();
        await triggerCompile();
      })(),
    });
    view.focus();
    return () => {
      view?.destroy();
      view = null;
    };
  });

  $effect(() => {
    const request = app.jumpRequest;
    if (request && view) goToLine(view, request.line);
  });
</script>

<section class="editor-column">
  {#if app.conflict}
    <div class="bar conflict" role="alert">
      <strong>{app.conflict.path}</strong> changed on disk while you had unsaved edits. Nothing has been merged.
      <span class="spacer"></span>
      <button onclick={() => void resolveConflict('keep-mine')}>Keep mine</button>
      <button class="primary" onclick={() => void resolveConflict('load-disk')}>Load from disk</button>
    </div>
  {/if}
  {#if app.notice}
    <div class="bar notice" role="status">
      <span>{app.notice}</span>
      <span class="spacer"></span>
      <button class="ghost" onclick={() => (app.notice = null)}>×</button>
    </div>
  {/if}

  {#if app.activeDoc}
    <div class="editor-host" bind:this={host}></div>
  {:else}
    <div class="empty">
      {#if app.project}
        <p>Pick a file on the left.</p>
      {:else}
        <p>
          <strong>Preamble</strong><br />
          Open a folder with a <code>.tex</code> file in it.<br />
          <kbd>Ctrl</kbd> <kbd>O</kbd>
        </p>
      {/if}
    </div>
  {/if}

  {#if app.drawerOpen}
    <Drawer />
  {/if}
</section>
