<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import { openFolder, start, triggerCompile } from './lib/controller.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Editor from './components/Editor.svelte';
  import PdfPane from './components/PdfPane.svelte';
  import StatusBar from './components/StatusBar.svelte';

  onMount(() => {
    void start();
  });

  // Global shortcuts that must work even when the editor is not focused.
  function onKeydown(event: KeyboardEvent) {
    const mod = event.ctrlKey || event.metaKey;
    if (!mod) return;
    if (event.key === 'o') {
      event.preventDefault();
      void openFolder();
    } else if (event.key === 'b') {
      event.preventDefault();
      // triggerCompile saves every open tab first (S2.3), so nothing extra needs saving here.
      void triggerCompile();
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
    <button class="ghost" disabled={!app.project} onclick={() => (app.drawerOpen = !app.drawerOpen)}>
      Diagnostics
    </button>
  </header>

  <Sidebar />
  <Editor />
  <PdfPane />

  <StatusBar />
</div>
