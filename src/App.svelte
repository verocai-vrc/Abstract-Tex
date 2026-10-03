<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import {
    openFolder,
    saveNow,
    showCloneWindow,
    goToNextPlaceholder,
    showNewProjectWindow,
    showActivityView,
    start,
    toggleCommandPalette,
    toggleDrawer,
    toggleQuickOpen,
    triggerCompile,
  } from './lib/controller.svelte';
  import { shortcutFor } from './lib/shortcuts';
  import ActivityBar from './components/ActivityBar.svelte';
  import SourceControl from './components/SourceControl.svelte';
  import AssistantView from './components/AssistantView.svelte';
  import AssistantReview from './components/AssistantReview.svelte';
  import AssistantPayload from './components/AssistantPayload.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Editor from './components/Editor.svelte';
  import PdfPane from './components/PdfPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import QuickOpen from './components/QuickOpen.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import CloneWindow from './components/CloneWindow.svelte';
  import NewProjectWindow from './components/NewProjectWindow.svelte';
  import DiffView from './components/DiffView.svelte';
  import SnapshotViewer from './components/SnapshotViewer.svelte';
  import ZoteroLink from './components/ZoteroLink.svelte';
  import BibliographyHealth from './components/BibliographyHealth.svelte';

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
      case 'next-placeholder':
        void goToNextPlaceholder();
        break;
      case 'new-project':
        showNewProjectWindow();
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
      case 'view-files':
        showActivityView('files');
        break;
      case 'view-source-control':
        showActivityView('source-control');
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header class="toolbar">
    <span class="title">Abstract-Tex</span>
    <button onclick={() => showNewProjectWindow()}>New…</button>
    <button onclick={() => void openFolder()}>Open folder…</button>
    <button onclick={() => showCloneWindow()}>Clone…</button>
    <span class="project-name">{app.project ? app.project.rootDir : ''}</span>
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

  <ActivityBar />
  <!-- One left pane, two tenants (S10.3a). Each renders its own `.sidebar`, so the grid column
       holds whichever is current and nothing about the layout depends on which. -->
  {#if app.activityView === 'files'}
    <Sidebar />
  {:else if app.activityView === 'source-control'}
    <SourceControl />
  {:else}
    <AssistantView />
  {/if}
  <Editor />
  <PdfPane />

  <StatusBar />
</div>

<QuickOpen />
<CommandPalette />
<CloneWindow />
<NewProjectWindow />
<DiffView />
<SnapshotViewer />
<AssistantReview />
<AssistantPayload />
<ZoteroLink />
<BibliographyHealth />
