<script lang="ts">
  // The diagnostics drawer: the application's conscience (DESIGN.md §6).
  // Sprint 1 shows TeX's own message with the line it claims; v0.3 replaces the message with
  // a sentence about the document and adds one-click fixes. The raw log is never the default.
  import { app } from '../lib/state.svelte';
  import { jumpToLine, toggleRawLog } from '../lib/controller.svelte';

  const summary = $derived.by(() => {
    const c = app.compile;
    if (c.phase === 'failed') return 'The build could not run.';
    if (c.phase === 'error') {
      const n = c.errors.length;
      return n === 0 ? 'The build failed.' : n === 1 ? '1 error stopped the build.' : `${n} errors stopped the build.`;
    }
    return 'Build succeeded.';
  });
</script>

<div class="drawer" role="region" aria-label="Diagnostics">
  <header>
    <strong>{summary}</strong>
    <span class="spacer"></span>
    <button class="ghost" onclick={() => void toggleRawLog()}>
      {app.showRawLog ? 'Hide raw output' : 'Raw log'}
    </button>
    <button class="ghost" title="Close" onclick={() => (app.drawerOpen = false)}>×</button>
  </header>
  <div class="body">
    {#if app.showRawLog}
      <pre>{app.rawLog || app.compile.stderr || '(no output)'}</pre>
    {:else if app.compile.phase === 'failed'}
      <p>{app.compile.message}</p>
    {:else if app.compile.errors.length === 0 && app.compile.phase === 'error'}
      <p>TeX stopped without reporting a specific error. The raw output may say why.</p>
      {#if app.compile.stderr}<pre>{app.compile.stderr.split('\n').slice(-12).join('\n')}</pre>{/if}
    {:else}
      {#each app.compile.errors as error, i (i)}
        <div
          class="diag"
          role="button"
          tabindex="0"
          onclick={() => error.line && jumpToLine(error.line)}
          onkeydown={(e) => e.key === 'Enter' && error.line && jumpToLine(error.line)}
        >
          <span class="loc">{error.line ? `line ${error.line}` : '—'}</span>
          <span class="msg">{error.message}</span>
          {#if error.context}<span class="ctx">{error.context}</span>{/if}
        </div>
      {/each}
    {/if}
  </div>
</div>
