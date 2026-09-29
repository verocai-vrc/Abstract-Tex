<script lang="ts">
  // The diagnostics drawer: the application's conscience (DESIGN.md §6). One card per problem,
  // each a title and a couple of sentences from texlog's rule catalog (S2.6) — TeX's own words
  // are behind a "Raw log" button, never the default (DESIGN.md §2, rule 3). Clicking a card goes
  // to its line in its own file; a card whose rule carries a `Fix` (S5.5) also gets an "Apply fix"
  // button (S6.2).
  //
  // S6.3 (drawer v1): cards sit under a heading per file, errors before warnings within it; the
  // header carries a severity filter and a "this file" scope, each with live counts; every card
  // has its own "Raw log" that opens the transcript scrolled to that diagnostic's words. All of
  // the deciding — which file, what order, what the filter hides, where in the log — is in
  // `drawer.ts`, tested; this file only lays it out.
  import { app } from '../lib/state.svelte';
  import {
    allowShellEscape,
    applyDiagnosticFix,
    jumpToDiagnostic,
    setDrawerFilter,
    showRawLogFor,
    toggleRawLog,
  } from '../lib/controller.svelte';
  import { diagnosticsForFile, groupDiagnostics, locateInLog, type SeverityFilter } from '../lib/drawer';
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

  const rootFile = $derived(app.project?.rootFile ?? null);
  const documentFiles = $derived(app.project?.documentFiles ?? []);
  const groups = $derived(groupDiagnostics(app.compile.diagnostics, app.drawerFilter, app.activePath, rootFile, documentFiles));
  const shownCount = $derived(groups.reduce((n, g) => n + g.diagnostics.length, 0));
  const total = $derived(app.compile.diagnostics.length);
  /** How many of the build's diagnostics are about the active tab at all, before the severity
   * filter — so "nothing in this file" is only said when it is true, not when the file has
   * warnings and the author asked for errors. */
  const inActiveFile = $derived(diagnosticsForFile(app.compile.diagnostics, app.activePath, rootFile, documentFiles).length);

  // The raw view, split around the words the last "Raw log" click asked for (S6.3), so a `<mark>`
  // can wrap them. Three plain strings rather than markup built by hand: the log is untrusted
  // text and goes into text nodes only.
  const rawView = $derived.by(() => {
    const text = app.rawLog || app.compile.stderr || '(no output)';
    const focus = app.rawLogFocus;
    const range = focus && app.rawLog ? locateInLog(app.rawLog, focus.rawMessage) : null;
    if (!range) return { before: text, match: '', after: '' };
    return { before: text.slice(0, range.from), match: text.slice(range.from, range.to), after: text.slice(range.to) };
  });

  // Scroll the highlighted words into view once they are rendered. `mark` is set by `bind:this`
  // when the `{#if}` around it renders and cleared when it goes away; the effect reads only that
  // and the focus nonce, never writes either, so it cannot loop (MEMORY: effect_update_depth_exceeded).
  let mark: HTMLElement | undefined = $state();
  $effect(() => {
    const nonce = app.rawLogFocus?.nonce;
    if (mark && nonce !== undefined) mark.scrollIntoView({ block: 'center' });
  });

  const severityKinds: readonly SeverityFilter[] = ['all', 'errors', 'warnings'];

  function severityLabel(kind: SeverityFilter): string {
    if (kind === 'errors') return app.errorCount === 1 ? '1 error' : `${app.errorCount} errors`;
    if (kind === 'warnings') return app.warningCount === 1 ? '1 warning' : `${app.warningCount} warnings`;
    return `All ${total}`;
  }

  /** What to call a group: the file as TeX spelled it, or an honest "we do not know". */
  function groupTitle(file: string | null): string {
    return file ?? 'Not inside any file';
  }

  function onKeydown(event: KeyboardEvent, diagnostic: Diagnostic) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    void jumpToDiagnostic(diagnostic);
  }

  // The card itself is `role="button"` for "go to this line"; the buttons inside it must stop
  // the click from also bubbling up to that outer handler, or applying a fix would jump to the
  // line twice — once from each handler reaching the same `jumpToLine` call — and "Raw log"
  // would jump away from the card the author just asked to read the log for.
  function onApplyFix(event: MouseEvent, diagnostic: Diagnostic) {
    event.stopPropagation();
    void applyDiagnosticFix(diagnostic);
  }
  function onAllowShellEscape(event: MouseEvent) {
    event.stopPropagation();
    void allowShellEscape();
  }
  function onRawLog(event: MouseEvent, diagnostic: Diagnostic) {
    event.stopPropagation();
    void showRawLogFor(diagnostic);
  }
</script>

<div class="drawer" role="region" aria-label="Diagnostics">
  <header>
    <strong>{summary}</strong>
    {#if total > 0 && !app.showRawLog}
      <span class="filters" role="group" aria-label="Show">
        {#each severityKinds as kind (kind)}
          <button
            class="chip"
            class:active={app.drawerFilter.severity === kind}
            aria-pressed={app.drawerFilter.severity === kind}
            onclick={() => setDrawerFilter({ severity: kind })}
          >
            {severityLabel(kind)}
          </button>
        {/each}
        <button
          class="chip"
          class:active={app.drawerFilter.activeFileOnly}
          aria-pressed={app.drawerFilter.activeFileOnly}
          disabled={!app.activePath}
          title={app.activePath ? `Only problems in ${app.activePath}` : 'No file is open'}
          onclick={() => setDrawerFilter({ activeFileOnly: !app.drawerFilter.activeFileOnly })}
        >
          This file
        </button>
      </span>
    {/if}
    <span class="spacer"></span>
    <button class="ghost" onclick={() => void toggleRawLog()}>
      {app.showRawLog ? 'Back to explanations' : 'Raw log'}
    </button>
    <button class="ghost" title="Close" onclick={() => (app.drawerOpen = false)}>×</button>
  </header>
  <div class="body">
    {#if app.showRawLog}
      <pre>{rawView.before}{#if rawView.match}<mark bind:this={mark}>{rawView.match}</mark>{/if}{rawView.after}</pre>
    {:else if app.compile.phase === 'failed'}
      <p>{app.compile.message}</p>
    {:else if total === 0 && app.compile.phase === 'error'}
      <p>TeX stopped without reporting a specific error. The raw output may say why.</p>
      {#if app.compile.stderr}<pre>{app.compile.stderr.split('\n').slice(-12).join('\n')}</pre>{/if}
    {:else if shownCount === 0 && total > 0}
      <!-- The filter hid everything. Say so, and offer the way back — never a blank pane that
           looks like "nothing is wrong" when something is (DESIGN.md §6). -->
      <p class="filtered-out">
        {#if app.drawerFilter.activeFileOnly && app.activePath && inActiveFile === 0}
          Nothing reported in {app.activePath}. {total === 1 ? '1 problem is' : `${total} problems are`} in other files.
        {:else}
          Nothing matches this filter. {total === 1 ? '1 problem is' : `${total} problems are`} hidden.
        {/if}
        <button class="ghost" onclick={() => setDrawerFilter({ severity: 'all', activeFileOnly: false })}>Show all</button>
      </p>
    {:else}
      {#each groups as group (group.file)}
        <section class="group">
          <h3>
            <span class="file" class:unknown={group.file === null}>{groupTitle(group.file)}</span>
            <span class="counts">
              {#if group.errorCount > 0}<span class="count error">{group.errorCount === 1 ? '1 error' : `${group.errorCount} errors`}</span>{/if}
              {#if group.warningCount > 0}<span class="count warning">{group.warningCount === 1 ? '1 warning' : `${group.warningCount} warnings`}</span>{/if}
            </span>
          </h3>
          {#each group.diagnostics as diagnostic, i (i)}
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
              <span class="title">
                <span class="severity">{diagnostic.severity}</span>
                {diagnostic.title}
              </span>
              <span class="explanation">{diagnostic.explanation}</span>
              <span class="actions">
                {#if diagnostic.fix}
                  <button class="ghost fix" onclick={(e) => onApplyFix(e, diagnostic)}>
                    {diagnostic.fix.description}
                  </button>
                {/if}
                {#if diagnostic.rule === 'shell-escape-required' && !app.shellEscapeAllowed}
                  <!-- S9.8: not a text fix, a permission; the dialog behind it asks first. -->
                  <button class="ghost fix" onclick={onAllowShellEscape}>Allow for this folder…</button>
                {/if}
                <button class="ghost raw" title="Show what TeX printed for this" onclick={(e) => onRawLog(e, diagnostic)}>
                  Raw log
                </button>
              </span>
            </div>
          {/each}
        </section>
      {/each}
    {/if}
  </div>
</div>
