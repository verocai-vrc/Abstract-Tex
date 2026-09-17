<script lang="ts">
  // The diagnostics drawer: the application's conscience (DESIGN.md §6). One card per problem,
  // each a title and a couple of sentences from texlog's rule catalog (S2.6) — TeX's own words
  // are behind the "Raw log" button, never the default (DESIGN.md §2, rule 3). Clicking a card
  // goes to its line; a card whose rule carries a `Fix` (S5.5) also gets an "Apply fix" button
  // (S6.2); grouping/filtering is S6.3.
  import { app } from '../lib/state.svelte';
  import { applyDiagnosticFix, jumpToDiagnostic, toggleRawLog } from '../lib/controller.svelte';
  import type { Diagnostic } from '../lib/ipc';

  const summary = $derived.by(() => {
    const c = app.compile;
    if (c.phase === 'failed') return 'The build could not run.';
    if (c.phase === 'error') {
      const n = app.errorCount;
      return n === 0 ? 'The build failed.' : n === 1 ? '1 error stopped the build.' : `${n} errors stopped the build.`;
    }
    if (c.phase === 'ok' && app.warningCount > 0) {
      const n = app.warningCount;
      return n === 1 ? 'Built, with 1 thing to look at.' : `Built, with ${n} things to look at.`;
    }
    return 'Build succeeded.';
  });

  // Errors first, in the order TeX reported them; then warnings. An unexplained error (no rule
  // matched) sits among the errors, not at the bottom: it still stopped the build.
  const ordered = $derived(
    [...app.compile.diagnostics].sort((a, b) => (a.severity === b.severity ? 0 : a.severity === 'error' ? -1 : 1)),
  );

  function onKeydown(event: KeyboardEvent, diagnostic: Diagnostic) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    void jumpToDiagnostic(diagnostic);
  }

  // The card itself is `role="button"` for "go to this line"; the fix button sits inside it and
  // must stop the click from also bubbling up to that outer handler, or applying a fix would
  // jump to the line twice — once from each handler reaching the same `jumpToLine` call.
  function onApplyFix(event: MouseEvent, diagnostic: Diagnostic) {
    event.stopPropagation();
    void applyDiagnosticFix(diagnostic);
  }
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
    {:else if ordered.length === 0 && app.compile.phase === 'error'}
      <p>TeX stopped without reporting a specific error. The raw output may say why.</p>
      {#if app.compile.stderr}<pre>{app.compile.stderr.split('\n').slice(-12).join('\n')}</pre>{/if}
    {:else}
      {#each ordered as diagnostic, i (i)}
        <div
          class="diag"
          class:warning={diagnostic.severity === 'warning'}
          class:unexplained={diagnostic.rule === null}
          role="button"
          tabindex="0"
          title={diagnostic.line ? `Go to line ${diagnostic.line}` : 'TeX did not say which line'}
          onclick={() => void jumpToDiagnostic(diagnostic)}
          onkeydown={(e) => onKeydown(e, diagnostic)}
        >
          <span class="loc">{diagnostic.line ? `line ${diagnostic.line}` : '—'}</span>
          <span class="title">{diagnostic.title}</span>
          <span class="explanation">{diagnostic.explanation}</span>
          {#if diagnostic.fix}
            <button class="ghost fix" onclick={(e) => onApplyFix(e, diagnostic)}>
              {diagnostic.fix.description}
            </button>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>
