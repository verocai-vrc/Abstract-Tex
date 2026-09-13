<script lang="ts">
  import type { EditorView } from '@codemirror/view';
  import { app } from '../lib/state.svelte';
  import {
    lspCompletion,
    lspDiagnosticsFor,
    lspGoToDefinition,
    lspHover,
    resolveConflict,
    syncTexForward,
  } from '../lib/controller.svelte';
  import { createEditor, goToLine } from '../lib/editor/setup';
  import { applyDiagnostics, applyLspDiagnostics } from '../lib/editor/diagnostics';
  import { lspCompletionSource } from '../lib/editor/completion';
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
    const path = app.activePath;
    if (!host || !doc || !path) return;
    // The teardown below assigns to `view`, and `view` is reactive state this component also
    // reads. An $effect that writes state it depends on re-runs itself forever
    // (effect_update_depth_exceeded), so the instance the teardown needs is held in a local
    // that is never read reactively; `view` is only ever *written* here, for the benefit of
    // the two effects below.
    //
    // The completion source, and the hover/definition requesters below, are all built fresh per
    // tab, closing over `path`, rather than reading `app.activePath` at call time: each runs
    // asynchronously and a tab switch mid-request must not silently redirect an in-flight query
    // to a different file.
    const completionSource = lspCompletionSource((line, character) => lspCompletion(path, line, character));
    const hoverRequest = (line: number, character: number) => lspHover(path, line, character);
    const definitionRequest = (line: number, character: number) => lspGoToDefinition(path, line, character);
    const forwardSearchRequest = (line: number) => void syncTexForward(path, line);
    const created = createEditor(host, doc, completionSource, hoverRequest, definitionRequest, forwardSearchRequest);
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

  // Gutter dots from the language server (S3.3b), in a second effect so a publish and a build
  // never clear each other's dots — `applyLspDiagnostics` writes its own CodeMirror field.
  //
  // Unlike the effect above this is not root-file-only: a `publishDiagnostics` names its own
  // URI, so the controller can answer for whichever tab is open.
  //
  // The rows themselves live in a plain, non-reactive store, so the only thing in `app` that
  // moves when the server publishes is `lspDiagnosticsVersion`, a counter. It is passed as an
  // argument rather than read and discarded, so the `$derived` depends on it visibly and no
  // later reader mistakes the subscription for dead code. Nothing here writes what it reads
  // (MEMORY: `effect_update_depth_exceeded`).
  const lspRows = $derived(lspDiagnosticsFor(app.activePath, app.lspDiagnosticsVersion));

  $effect(() => {
    if (view) applyLspDiagnostics(view, lspRows);
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
