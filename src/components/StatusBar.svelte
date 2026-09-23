<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { bibliography } from '../lib/bibliography.svelte';
  import { detectZotero, toggleDrawer } from '../lib/controller.svelte';

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
</script>

<footer class="statusbar">
  {#if app.compile.phase === 'running'}
    <span class="warn">
      <span class="dot pulse"></span>Compiling… {elapsed}s{app.compile.progress ? ` · ${app.compile.progress}` : ''}
    </span>
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
  {/if}

  <span class="spacer"></span>
  {#if app.lspMessage}
    <!-- Quiet, not a notice: completion being unavailable does not stop anyone writing. -->
    <span class="muted" title={app.lspMessage}>no language server</span>
  {/if}
  <span class={app.engine === null ? 'error' : ''}>{engineLabel}</span>
  <button class={`ghost muted${bibliography.zoteroStatus === 'ready' ? ' ok' : ''}`} onclick={onZoteroClick}>
    {zoteroLabel}
  </button>
  <span><kbd>Ctrl</kbd><kbd>S</kbd> save · <kbd>Ctrl</kbd><kbd>B</kbd> build</span>
</footer>
