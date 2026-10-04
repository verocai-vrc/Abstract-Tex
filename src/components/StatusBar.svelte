<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { assistant } from '../lib/assistant.svelte';
  import { bibliography } from '../lib/bibliography.svelte';
  import { branchLabel, git, syncArrows } from '../lib/git.svelte';
  import { github } from '../lib/github.svelte';
  import { placeholders } from '../lib/placeholders.svelte';
  import { leftToFillIn } from '../lib/editor/placeholders';
  import { detectZotero, disallowShellEscape, goToNextPlaceholder, leaveLiveSession, showActivityView, showJoinWindow, showShareWindow, toggleDrawer } from '../lib/controller.svelte';
  import { live } from '../lib/live.svelte';

  // Clicking the Zotero button probes on the first click (status still `null`); once it reads
  // `ready`, the same button opens the "link a collection" picker (S8.2) instead of re-probing —
  // there is nothing more to detect once linking is possible, and re-probing on every click would
  // be a wasted round trip for no visible benefit.
  function onZoteroClick() {
    if (bibliography.zoteroStatus === 'ready') {
      app.zoteroLinkVisible = true;
    } else {
      void detectZotero();
    }
  }

  // A ticking clock while a build runs, so a long first compile (package downloads) never
  // looks like a hang (DESIGN.md §6, first run).
  let now = $state(Date.now());
  $effect(() => {
    if (app.compile.phase !== 'running') return;
    const timer = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(timer);
  });

  const elapsed = $derived(((now - app.compile.startedAt) / 1000).toFixed(1));
  const engineLabel = $derived(
    app.engine === undefined ? 'probing engine…' : app.engine === null ? 'no TeX engine found' : `${app.engine.name} ${app.engine.version.replace(/^tectonic\s*/i, '')}`,
  );

  // Detection is a manual probe (S8.1), not a background poll: nobody asked until they click
  // this, so `zoteroStatus` stays `null` and the button reads as an offer, not a state. Once
  // `ready`, the same button's job changes from "detect" to "link a collection" (S8.2).
  const zoteroLabel = $derived(
    bibliography.zoteroStatus === 'ready'
      ? 'Link Zotero collection'
      : bibliography.zoteroStatus === 'no_better_bibtex'
        ? 'Zotero running, no Better BibTeX'
        : bibliography.zoteroStatus === 'not_running'
          ? 'Zotero not running'
          : 'Detect Zotero',
  );

  // S10.3b: the branch at the left, as VS Code and DESIGN.md §6 both put it. Empty — not "no
  // branch" — in a project with no repository: a status bar that mentioned Git there would be
  // saying something about a feature the author has not asked for.
  const branch = $derived(git.isRepository ? branchLabel(git.branch) : '');
  const arrows = $derived(syncArrows(git.branch));

  // S8.3: a quiet count, shown only once there is something to say — an empty bibliography or a
  // clean one both mean "nothing to show here", matching the drawer's own "only interrupt for a
  // real problem" rule (DESIGN.md §6).
  // S11.12: said only while there is something to say, and never as "0 left".
  const leftText = $derived(leftToFillIn(placeholders.total));

  const bibIssueCount = $derived(bibliography.findings.length);

  // S14.2c: the live session on whichever tab is active, if any — `null` the rest of the time,
  // which is every project before this sprint and most sessions even now.
  const liveStatus = $derived(app.activePath ? live.status.get(app.activePath) ?? null : null);
  const livePeers = $derived(app.activePath ? live.peers.get(app.activePath) ?? [] : []);
</script>

<footer class="statusbar">
  {#if branch}
    <!-- Clicking it opens the panel the branch belongs to, which is what VS Code's does. The
         arrows stay information only even now that §5.7's one-verb path works (S11.1): the design
         puts that button in the Source Control view, not here, and drawing two Sync controls
         would leave them to disagree about whether a sync is already running. -->
    <button class="ghost" title="Source Control (Ctrl Shift G)" onclick={() => showActivityView('source-control')}>
      {branch}{arrows ? ` ${arrows}` : ''}
    </button>
  {/if}
  {#if app.compile.phase === 'running'}
    <span class="warn">
      <span class="dot pulse"></span>Compiling… {elapsed}s{app.compile.progress ? ` · ${app.compile.progress}` : ''}
    </span>
    {#if app.pdfDraftOf}
      <!-- S9.9: the PDF on screen is one chapter, typeset ahead of the whole document. -->
      <span class="muted" title="Only this chapter was typeset; the full PDF replaces it when the build finishes.">
        showing a draft of {app.pdfDraftOf}
      </span>
    {/if}
  {:else if app.compile.phase === 'ok'}
    <span class="ok"><span class="dot"></span>Built in {((app.compile.durationMs ?? 0) / 1000).toFixed(1)}s</span>
    {#if app.warningCount > 0}
      <button class="ghost warn" onclick={() => toggleDrawer()}>
        {app.warningCount === 1 ? '1 warning' : `${app.warningCount} warnings`}
      </button>
    {/if}
  {:else if app.compile.phase === 'error'}
    <span class="error"><span class="dot"></span>
      {app.errorCount === 0 ? 'Build failed' : app.errorCount === 1 ? '1 error' : `${app.errorCount} errors`}
    </span>
  {:else if app.compile.phase === 'failed'}
    <span class="error"><span class="dot"></span>Could not build</span>
  {:else}
    <span>Ready</span>
  {/if}

  {#if app.activePath}
    <span>{app.activePath}{app.dirty ? ' •' : ''}</span>
    {#if liveStatus === 'connected'}
      <!-- S14.2c: one verb to end it, same as Zotero's and shell-escape's buttons elsewhere on
           this bar — the status itself is the button, there is no separate indicator to click
           past to find the control. -->
      <button
        class="ghost ok"
        title={livePeers.length > 0 ? `Live with ${livePeers.map((peer) => peer.name).join(', ')}. Click to leave.` : 'Live, nobody else here yet. Click to leave.'}
        onclick={() => leaveLiveSession(app.activePath!)}
      >
        <span class="dot"></span>Live{livePeers.length > 0 ? ` · ${livePeers.length}` : ''}
      </button>
    {:else if liveStatus === 'connecting'}
      <span class="muted"><span class="dot pulse"></span>Connecting…</span>
    {:else if liveStatus === 'error'}
      <button class="ghost warn" title={live.error.get(app.activePath) ?? ''} onclick={() => leaveLiveSession(app.activePath!)}>
        Live session failed
      </button>
    {:else if liveStatus === 'closed'}
      <button class="ghost warn" title={live.error.get(app.activePath) ?? 'The other side left, or the connection dropped.'} onclick={() => leaveLiveSession(app.activePath!)}>
        Live session ended
      </button>
    {:else}
      <button class="ghost muted" title="Share this document live, or join a session" onclick={() => showShareWindow()}>
        Share…
      </button>
      <button class="ghost muted" onclick={() => showJoinWindow()}>Join…</button>
    {/if}
  {/if}

  {#if leftText}
    <!-- S11.12: a fresh template says how much of it is still the template's. One click goes to
         the next, the same as F8. -->
    <button class="ghost" title="Go to next placeholder (F8)" onclick={() => void goToNextPlaceholder()}>
      {leftText}
    </button>
  {/if}

  {#if assistant.asking}
    <!-- The one moment text leaves the machine for the assistant, so it says so. -->
    <span class="muted"><span class="dot pulse"></span>Asking the assistant…</span>
  {/if}

  <span class="spacer"></span>
  {#if app.shellEscapeAllowed}
    <!-- S9.8: on means visible. One click turns it off; turning it on again asks. -->
    <button
      class="ghost warn"
      title="Documents in this folder may run programs while they build. Click to stop allowing it."
      onclick={() => void disallowShellEscape()}
    >
      shell escape on
    </button>
  {/if}
  {#if app.lspMessage}
    <!-- Quiet, not a notice: completion being unavailable does not stop anyone writing. -->
    <span class="muted" title={app.lspMessage}>no language server</span>
  {/if}
  {#if github.account}
    <!-- S10.4b. Only when signed in: an account nobody has asked for is not worth a word in the
         status bar, and the offer to sign in lives in the panel that needs it. -->
    <span class="muted" title="Signed in to GitHub as {github.account.login}">{github.account.login}</span>
  {/if}
  <span class={app.engine === null ? 'error' : ''}>{engineLabel}</span>
  <button class={`ghost muted${bibliography.zoteroStatus === 'ready' ? ' ok' : ''}`} onclick={onZoteroClick}>
    {zoteroLabel}
  </button>
  {#if bibIssueCount > 0}
    <button class="ghost warn" onclick={() => (app.bibliographyPanelVisible = !app.bibliographyPanelVisible)}>
      {bibIssueCount === 1 ? '1 bibliography issue' : `${bibIssueCount} bibliography issues`}
    </button>
  {/if}
  <span><kbd>Ctrl</kbd><kbd>S</kbd> save · <kbd>Ctrl</kbd><kbd>B</kbd> build</span>
</footer>
