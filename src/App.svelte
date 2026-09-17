<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import {
    openFolder,
    saveNow,
    start,
    toggleCommandPalette,
    toggleDrawer,
    toggleQuickOpen,
    triggerCompile,
  } from './lib/controller.svelte';
  import { shortcutFor } from './lib/shortcuts';
  import Sidebar from './components/Sidebar.svelte';
  import Editor from './components/Editor.svelte';
  import PdfPane from './components/PdfPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import QuickOpen from './components/QuickOpen.svelte';
  import CommandPalette from './components/CommandPalette.svelte';

  onMount(() => {
    void start();
  });

  // The one place a global key becomes an action (S2.4). The table in shortcuts.ts says which
  // key means what; this only says what each action does. Listening on the window means the
  // chords work with the editor, the tree, the PDF or nothing at all focused.
  function onKeydown(event: KeyboardEvent) {
    const action = shortcutFor(event);
    if (!action) return;
    event.preventDefault(); // else the browser offers to save the page, or prints it
    switch (action) {
      case 'save':
        void saveNow();
        break;
      case 'compile':
        // triggerCompile saves every open tab first (S2.3), so nothing extra needs saving here.
        void triggerCompile();
        break;
      case 'open-folder':
        void openFolder();
        break;
      case 'quick-open':
        toggleQuickOpen();
        break;
      case 'command-palette':
        toggleCommandPalette();
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header class="toolbar">
    <span class="title">Preamble</span>
    <button onclick={() => void openFolder()}>Open folder…</button>
    <span class="project-name">{app.project ? app.project.rootDir : 'No project open'}</span>
    <button
      class="primary"
      disabled={!app.project || app.compile.phase === 'running'}
      onclick={() => void triggerCompile()}
    >
      Build
    </button>
    <button class="ghost" disabled={!app.project} onclick={() => toggleDrawer()}>
      Diagnostics
    </button>
  </header>

  <Sidebar />
  <Editor />
  <PdfPane />

  <StatusBar />
</div>

<QuickOpen />
<CommandPalette />
