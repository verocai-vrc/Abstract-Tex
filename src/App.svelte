<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import {
    saveNow,
    savePdf,
    goToNextPlaceholder,
    openPdfFind,
    showActivityView,
    showPane,
    start,
    toggleCommandPalette,
    toggleDrawer,
    toggleQuickOpen,
    triggerCompile,
  } from './lib/controller.svelte';
  import { backToDocument, showStart, startBack } from './lib/start-controller.svelte';
  import { chordOnStartScreen, startScreen } from './lib/start.svelte';
  import { handledByInputMethod } from './lib/keys';
  import { shortcutFor } from './lib/shortcuts';
  import { words } from './lib/words';
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
  import StartScreen from './components/StartScreen.svelte';
  import OpeningNotice from './components/OpeningNotice.svelte';
  import CloneWindow from './components/CloneWindow.svelte';
  import ShareWindow from './components/ShareWindow.svelte';
  import JoinWindow from './components/JoinWindow.svelte';
  import DiffView from './components/DiffView.svelte';
  import CrashReportWindow from './components/CrashReportWindow.svelte';
  import SessionCommitOffer from './components/SessionCommitOffer.svelte';
  import CommentsView from './components/CommentsView.svelte';
  import SnapshotViewer from './components/SnapshotViewer.svelte';
  import ZoteroLink from './components/ZoteroLink.svelte';
  import BibliographyHealth from './components/BibliographyHealth.svelte';

  onMount(() => {
    void start();
  });

  /** Every dialog sits on a `.backdrop` (`dialogs.test.ts`). */
  const dialogIsUp = () => document.querySelector('.backdrop') !== null;

  /**
   * Whether an Escape now on its way is for the start screen. Asked before anything else hears the key:
   * a dialog that closes on it is gone by the time it gets to the window, and so is an opening that
   * stops, and that same key must not go on to leave the page as well.
   */
  let escapeIsOurs = false;
  function onKeydownFirst(event: KeyboardEvent) {
    escapeIsOurs =
      event.key === 'Escape' && startScreen.visible && app.opening === null && !handledByInputMethod(event) && !dialogIsUp();
  }

  // The one place a global key becomes an action (S2.4). The table in shortcuts.ts says which
  // key means what; this only says what each action does. Listening on the window means the
  // chords work with the editor, the tree, the PDF or nothing at all focused.
  function onKeydown(event: KeyboardEvent) {
    if (escapeIsOurs && event.key === 'Escape' && !event.defaultPrevented) {
      event.preventDefault();
      startBack();
      return;
    }
    const action = shortcutFor(event);
    if (!action) return;
    event.preventDefault(); // else the browser offers to save the page, or prints it
    if (app.opening !== null) return; // the document these act on may be about to go (S17.2)
    if (startScreen.visible) {
      const answer = chordOnStartScreen(action, app.project !== null);
      if (answer === 'nothing') return;
      if (answer === 'back-first') {
        if (dialogIsUp()) return; // it would be left standing over the document
        backToDocument();
      }
    }
    switch (action) {
      case 'home':
        showStart('home', dialogIsUp());
        break;
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
        showStart('new', dialogIsUp());
        break;
      case 'open-folder':
        showStart('open', dialogIsUp());
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
      case 'find-in-pdf':
        openPdfFind();
        break;
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydownFirst} onkeydown={onKeydown} />

<!-- `inert`: while the start screen covers this, or another folder is being opened, nobody may type
     into it, click in it, or Tab into it. `OpeningNotice` is outside it so that *Stop* still works. -->
<div class="app" data-pane={app.pane} inert={app.launching || startScreen.visible || app.opening !== null}>
  <header class="toolbar">
    <span class="title">Abstract-Tex</span>
    <button onclick={() => showStart('home', dialogIsUp())}>{words.pages.home}</button>
    <span class="project-name" title={app.project?.rootDir}>{app.project?.title ?? ''}</span>
    <!-- Drawn only where there is room for one pane (app.css). The left pane has other things in it too,
         for which the activity bar has the buttons. -->
    <div class="pane-choice" role="group" aria-label={words.panesLabel}>
      <button aria-pressed={app.pane === 'files' && app.activityView === 'files'} onclick={() => showActivityView('files')}>
        {words.panes.files}
      </button>
      <button aria-pressed={app.pane === 'write'} onclick={() => showPane('write')}>{words.panes.write}</button>
      <button aria-pressed={app.pane === 'pdf'} onclick={() => showPane('pdf')}>{words.panes.pdf}</button>
    </div>
    <button disabled={!app.hasPdf} onclick={() => void savePdf()}>{words.savePdf}</button>
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
  <!-- One left pane, two tenants (S10.3a). Each renders its own `.left-pane`, so the grid column
       holds whichever is current and nothing about the layout depends on which. -->
  {#if app.activityView === 'files'}
    <Sidebar />
  {:else if app.activityView === 'source-control'}
    <SourceControl />
  {:else if app.activityView === 'comments'}
    <CommentsView />
  {:else}
    <AssistantView />
  {/if}
  <Editor />
  <PdfPane />

  <StatusBar />
</div>

<StartScreen />
<OpeningNotice />
<SessionCommitOffer />
<CrashReportWindow />
<QuickOpen />
<CommandPalette />
<CloneWindow />
<ShareWindow />
<JoinWindow />
<DiffView />
<SnapshotViewer />
<AssistantReview />
<AssistantPayload />
<ZoteroLink />
<BibliographyHealth />
