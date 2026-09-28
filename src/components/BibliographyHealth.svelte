<script lang="ts">
  // The bibliography health panel (S8.3, DESIGN.md §5.4): six background checks — a `.bib` that
  // is not on disk (S8.6), undefined citation, never cited, duplicate DOI, missing required field,
  // wrong dash in a page range — each a sentence, never a raw anything (DESIGN.md §2 rule 3). Opened from the status bar's
  // issue count; a separate panel from the compile Drawer on purpose, since a `Finding` here has
  // no `.log` line behind it the way a `Diagnostic` does, and forcing the two into one shape would
  // mean inventing a raw view for something that never had one.
  import { app } from '../lib/state.svelte';
  import { bibliography } from '../lib/bibliography.svelte';
  import { jumpToFinding, unlinkBibFile } from '../lib/controller.svelte';
  import type { Finding } from '../lib/ipc';

  function close() {
    app.bibliographyPanelVisible = false;
  }

  async function onClick(finding: Finding) {
    await jumpToFinding(finding);
    close();
  }

  function onKeydown(event: KeyboardEvent, finding: Finding) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    void onClick(finding);
  }
</script>

{#if app.bibliographyPanelVisible}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-label="Bibliography health">
      <header>
        <span>Bibliography</span>
        <button class="ghost" onclick={close} aria-label="Close">✕</button>
      </header>
      <div class="body">
        {#if bibliography.findings.length === 0}
          <p class="muted">No problems found.</p>
        {:else}
          {#each bibliography.findingsGroups as { file, findings } (file)}
            <section class="group">
              <h3>{file}</h3>
              {#each findings as finding, i (i)}
                {#if finding.jump.kind === 'missingFile'}
                  <!-- Nothing to open for a file that is not on disk (S8.6), so not a button —
                       but an export that will never appear can be unlinked from here (S8.7),
                       which is the one place that works with Zotero closed. -->
                  <div class="finding static" class:warning={finding.severity === 'warning'}>
                    <span class="severity">{finding.severity}</span>
                    <span class="message">{finding.message}</span>
                    <button class="ghost unlink" onclick={() => void unlinkBibFile(finding.jump.file)}>Unlink</button>
                  </div>
                {:else}
                  <div
                    class="finding"
                    class:warning={finding.severity === 'warning'}
                    role="button"
                    tabindex="0"
                    onclick={() => void onClick(finding)}
                    onkeydown={(e) => onKeydown(e, finding)}
                  >
                    <span class="severity">{finding.severity}</span>
                    <span class="message">{finding.message}</span>
                  </div>
                {/if}
              {/each}
            </section>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgba(0, 0, 0, 0.18);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 10vh;
  }
  .dialog {
    width: min(480px, 90vw);
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .body {
    overflow-y: auto;
    padding: 4px 0;
  }
  .group h3 {
    margin: 0;
    padding: 6px 12px 2px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
  }
  .finding {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 6px 12px;
    cursor: pointer;
    font-size: 13px;
  }
  .finding:hover {
    background: var(--bg-selected);
  }
  .finding.static {
    cursor: default;
  }
  .unlink {
    flex: none;
    margin-left: auto;
  }
  .finding.static:hover {
    background: none;
  }
  .severity {
    flex: none;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--error);
    padding-top: 2px;
  }
  .finding.warning .severity {
    color: var(--warn);
  }
  .message {
    color: var(--fg);
  }
  p.muted {
    padding: 12px;
    margin: 0;
    color: var(--fg-muted);
    font-size: 13px;
  }
</style>
