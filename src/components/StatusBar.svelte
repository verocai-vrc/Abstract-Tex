<script lang="ts">
  import { app } from '../lib/state.svelte';

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
</script>

<footer class="statusbar">
  {#if app.compile.phase === 'running'}
    <span class="warn">
      <span class="dot pulse"></span>Compiling… {elapsed}s{app.compile.progress ? ` · ${app.compile.progress}` : ''}
    </span>
  {:else if app.compile.phase === 'ok'}
    <span class="ok"><span class="dot"></span>Built in {((app.compile.durationMs ?? 0) / 1000).toFixed(1)}s</span>
    {#if app.warningCount > 0}
      <button class="ghost warn" onclick={() => (app.drawerOpen = !app.drawerOpen)}>
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
  <span class={app.engine === null ? 'error' : ''}>{engineLabel}</span>
  <span><kbd>Ctrl</kbd><kbd>S</kbd> save · <kbd>Ctrl</kbd><kbd>B</kbd> build</span>
</footer>
