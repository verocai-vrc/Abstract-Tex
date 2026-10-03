<script lang="ts">
  // The payload inspector (S13.3, DESIGN.md §5.5): exactly what a request would carry out of this
  // computer, shown before it goes. Nothing has been sent while this is open; *Send* is the click
  // that sends it, and Esc or *Cancel* sends nothing.
  //
  // The parts are the text the model will read, in order, each labelled; *The exact request* below
  // them is the body byte for byte and the header names. A key is listed as a header with its value
  // replaced by a phrase: it never reaches the window.
  import { cancelPrepared, sendPrepared, setAssistantInspectFirst } from '../lib/controller.svelte';
  import { assistant } from '../lib/assistant.svelte';
  import { carriesContext, destinationLine, partLabel, prettyBody, sizeLine } from '../lib/assistant-payload';
  import { modalFocus } from '../lib/dialog';

  const prepared = $derived(assistant.prepared);
  const payload = $derived(prepared?.payload ?? null);

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') cancelPrepared();
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) void sendPrepared();
  }
</script>

{#if prepared && payload}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && cancelPrepared()}>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="What will be sent" tabindex="-1" use:modalFocus onkeydown={onKeydown}>
      <header>
        <span>{prepared.actionLabel}: what will be sent</span>
        <button class="ghost" onclick={cancelPrepared} aria-label="Close" disabled={prepared.sending}>✕</button>
      </header>

      <div class="scroll">
        <p class="lead">
          <strong>Nothing has been sent yet.</strong>
          {destinationLine(payload)}
          {sizeLine(payload)}.
          {#if carriesContext(payload)}
            This includes text you did not select, marked below.
          {:else}
            Only the text you selected leaves, with the instructions below; not the rest of the file, and not your project.
          {/if}
        </p>

        <div class="parts">
          {#each payload.parts as part, position (position)}
            <section class="part" class:context={part.cached}>
              <h3>{partLabel(part, position, payload.parts)}</h3>
              <pre>{part.text}</pre>
            </section>
          {/each}
        </div>

        <details>
          <summary>The exact request</summary>
          <p class="note">
            <code>POST {payload.url}</code>
          </p>
          <ul class="headers">
            {#each payload.headers as header (header.name)}
              <li><code>{header.name}: {header.value}</code></li>
            {/each}
          </ul>
          <pre class="body" aria-label="The request body">{prettyBody(payload.body)}</pre>
          <p class="note">Laid out for reading; the bytes sent differ only in whitespace.</p>
        </details>

        <label class="remember">
          <input
            type="checkbox"
            checked={assistant.status?.inspectFirst ?? true}
            onchange={(event) => void setAssistantInspectFirst(event.currentTarget.checked)}
          />
          <span>Show me each request before it is sent (you can change this in the Assistant view)</span>
        </label>

      </div>

      <footer>
        <span class="summary">Ctrl Enter sends</span>
        <span class="spacer"></span>
        <button class="ghost" onclick={cancelPrepared} disabled={prepared.sending}>Cancel</button>
        <button class="primary" onclick={() => void sendPrepared()} disabled={prepared.sending}>
          {prepared.sending ? 'Sending…' : 'Send'}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    width: min(760px, 92vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  /* Only the middle scrolls: the Send and Cancel buttons stay where they can be reached. */
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .lead {
    margin: 0;
    padding: 10px 14px 4px;
    font-size: 13px;
    line-height: 1.5;
  }
  .parts {
    padding: 4px 14px;
  }
  .part {
    margin: 8px 0;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-muted);
  }
  .part.context h3 {
    color: var(--accent);
  }
  pre {
    margin: 0;
    padding: 8px 10px;
    max-height: 22vh;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    background: var(--bg-editor);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    user-select: text;
  }
  .part.context pre {
    border-color: var(--accent);
  }
  details {
    margin: 4px 14px;
    font-size: 12px;
  }
  summary {
    cursor: pointer;
    color: var(--fg-muted);
  }
  .headers {
    margin: 4px 0;
    padding-left: 18px;
    font-size: 12px;
  }
  .note {
    margin: 4px 0;
    font-size: 11px;
    color: var(--fg-muted);
  }
  .remember {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 14px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
  }
  .summary {
    font-size: 12px;
    color: var(--fg-muted);
  }
  .spacer {
    flex: 1;
  }
</style>
