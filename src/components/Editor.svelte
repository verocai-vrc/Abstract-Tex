<script lang="ts">
  import type { EditorView } from '@codemirror/view';
  import { app } from '../lib/state.svelte';
  import { resolveConflict } from '../lib/controller.svelte';
  import { createEditor, goToLine } from '../lib/editor/setup';
  import { applyDiagnostics } from '../lib/editor/diagnostics';
  import Drawer from './Drawer.svelte';
  import Tabs from './Tabs.svelte';

  let host: HTMLDivElement | undefined = $state();
  // `$state.raw` rather than a plain `let` so the gutter effect below re-runs when a tab switch
  // replaces the view; raw because CodeMirror's view must never be proxied.
  let view = $state.raw<EditorView | null>(null);

  // Rebuild the editor whenever the open document changes — including switching tabs: each
  // open file keeps its own Y.Doc (S2.3), but only the active one has a live CodeMirror view.
  // CodeMirror owns everything inside `host`; Svelte only provides the element.
  $effect(() => {
    const doc = app.activeDoc;
    if (!host || !doc) return;
    // The teardown below assigns to `view`, and `view` is reactive state this component also
    // reads. An $effect that writes state it depends on re-runs itself forever
    // (effect_update_depth_exceeded), so the instance the teardown needs is held in a local
    // that is never read reactively; `view` is only ever *written* here, for the benefit of
    // the two effects below.
    const created = createEditor(host, doc);
    view = created;
    created.focus();
    return () => {
      created.destroy();
      view = null;
    };
  });

  $effect(() => {
    const request = app.jumpRequest;
    if (request && view) goToLine(view, request.line);
  });

  // Gutter dots for the last build (S2.7). A diagnostic carries a line but no file until S5.2,
  // so the dots are drawn only on the root file's tab — any other tab would be a guess.
  $effect(() => {
    const diagnostics = app.compile.diagnostics;
    const isRoot = app.activePath !== null && app.activePath === app.project?.rootFile;
    if (view) applyDiagnostics(view, isRoot ? diagnostics : []);
  });
</script>

<section class="editor-column">
  <Tabs />
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
        <p>Pick a file on the left, or press <kbd>Ctrl</kbd> <kbd>P</kbd>.</p>
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
