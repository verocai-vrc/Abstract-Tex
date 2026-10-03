<script lang="ts">
  import type { EditorView } from '@codemirror/view';
  import { app } from '../lib/state.svelte';
  import { bibliography } from '../lib/bibliography.svelte';
  import {
    lspCompletion,
    lspDiagnosticsFor,
    lspGoToDefinition,
    lspHover,
    openFolder,
    pasteCite,
    resolveConflict,
    showCloneWindow,
    showNewProjectWindow,
    syncTexForward,
  } from '../lib/controller.svelte';
  import { createEditor, goToLine, setFocusMode, setTypewriterMode } from '../lib/editor/setup';
  import { applyDiagnostics, applyLspDiagnostics } from '../lib/editor/diagnostics';
  import { diagnosticsForFile } from '../lib/drawer';
  import { citeThenLsp } from '../lib/editor/cite';
  import { lspCompletionSource } from '../lib/editor/completion';
  import { git } from '../lib/git.svelte';
  import { placeholders } from '../lib/placeholders.svelte';
  import { assistant } from '../lib/assistant.svelte';
  import ConflictResolver from './ConflictResolver.svelte';
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
    //
    // `citeThenLsp` (S7.3) wraps the LSP source rather than sitting beside it in a second
    // `override` entry: inside a `\cite{`-family argument it answers from the bibliography index
    // on its own and never calls the LSP source at all, so there is one popup at that position,
    // not TexLab's word completion competing with the reference list. `() => [...bibliography.
    // entries.values()]` reads the store fresh on every keystroke rather than closing over a
    // snapshot, the same freshness `citeSource`'s own doc comment requires.
    const completionSource = citeThenLsp(
      () => [...bibliography.entries.values()],
      lspCompletionSource((line, character) => lspCompletion(path, line, character)),
    );
    const hoverRequest = (line: number, character: number) => lspHover(path, line, character);
    const definitionRequest = (line: number, character: number) => lspGoToDefinition(path, line, character);
    const forwardSearchRequest = (line: number) => void syncTexForward(path, line);
    const created = createEditor(
      host,
      doc,
      completionSource,
      hoverRequest,
      definitionRequest,
      forwardSearchRequest,
      app.focusModeEnabled,
      app.typewriterModeEnabled,
      pasteCite,
      (line) => (placeholders.cursorLine = line),
      (from, to) => (assistant.selection = from === to ? null : { from, to }),
    );
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

  // S4.5's two writing modes: the card that defined this loop did not list this file, but the
  // toggle lives in `app` (state.svelte.ts) and something has to push a changed value into the
  // live `EditorView`'s `Compartment` — the same "state changes, an effect applies it to the
  // view" shape `applyDiagnostics`/`applyLspDiagnostics` below already use. Reconfiguring is
  // idempotent, so these also run harmlessly once at view creation, in addition to the initial
  // values already passed into `createEditor`.
  $effect(() => {
    if (view) setFocusMode(view, app.focusModeEnabled);
  });
  $effect(() => {
    if (view) setTypewriterMode(view, app.typewriterModeEnabled);
  });

  // Gutter dots for the last build (S2.7), on the tab each diagnostic belongs to (S6.3): a
  // chapter's own dots on the chapter's tab, and the ones that named no file on the root's —
  // `diagnosticsForFile` applies the same rule the drawer's click and one-click fix use. Until
  // S6.3 every dot was drawn on the root file's tab, the only honest place before `Diagnostic.file`
  // existed (S5.6).
  $effect(() => {
    if (view) applyDiagnostics(view, diagnosticsForFile(app.compile.diagnostics, app.activePath, app.project?.rootFile ?? null, app.project?.documentFiles ?? []));
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

  /** S11.2b: whether the active tab is a conflicted file. `openFile` still opens it normally — a
   * tab and a `Y.Doc` both exist — but CodeMirror never mounts over it while this is true;
   * `ConflictResolver` reads the file itself and is what the author sees instead. */
  const activeIsConflicted = $derived(app.activePath !== null && git.conflictedRows.some((row) => row.path === app.activePath));
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

  {#if activeIsConflicted}
    <ConflictResolver />
  {:else if app.activeDoc}
    <div class="editor-host" bind:this={host}></div>
  {:else}
    <div class="empty">
      {#if app.project}
        <p>Pick a file on the left, or press <kbd>Ctrl</kbd> <kbd>P</kbd>.</p>
      {:else}
        <div>
          <strong>Abstract-Tex</strong>
          <p>Start a document, or open one you already have.</p>
          <div class="doors">
            <button class="primary" onclick={() => showNewProjectWindow()}>
              New from template… <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>N</kbd>
            </button>
            <button onclick={() => void openFolder()}>Open folder… <kbd>Ctrl</kbd> <kbd>O</kbd></button>
            <button onclick={() => showCloneWindow()}>Clone a repository…</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  {#if app.drawerOpen}
    <Drawer />
  {/if}
</section>
