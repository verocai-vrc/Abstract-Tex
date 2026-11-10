<script lang="ts">
  // "Abstract-Tex closed unexpectedly last time" (S16.3, design interview F8). Shows the whole
  // report, so the author knows exactly what would be shared, and offers three things: open a
  // prefilled GitHub issue in the browser, copy the text, or not now. Nothing leaves this machine
  // until the author submits the page that opens — and this window has no way to send anything.
  import { copyCrashReport, dismissCrash, reportCrash } from '../lib/controller.svelte';
  import { crash } from '../lib/crash.svelte';
  import { modalFocus } from '../lib/dialog';
</script>

{#if crash.report}
  <div class="backdrop" role="presentation">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="dialog"
      role="dialog"
      aria-label="Abstract-Tex closed unexpectedly"
      tabindex="-1"
      use:modalFocus
      onkeydown={(event) => event.key === 'Escape' && void dismissCrash()}
    >
      <header>Abstract-Tex closed unexpectedly last time</header>
      <p class="intro">
        Sorry about that. This is what it wrote down, and it is all that it wrote down: the version, your
        system, and where the program failed. It holds none of your documents. Nothing has been sent anywhere.
      </p>
      <textarea readonly rows="10" aria-label="The crash report" value={crash.report.text}></textarea>
      {#if crash.error}<p class="error">{crash.error}</p>{/if}
      <footer>
        <button class="ghost" onclick={() => void dismissCrash()}>Not now</button>
        <span class="spacer"></span>
        <button class="ghost" onclick={() => void copyCrashReport()}>{crash.copied ? 'Copied' : 'Copy report'}</button>
        <button class="primary" onclick={() => void reportCrash()}>Report on GitHub…</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(0, 0, 0, 0.25);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 8vh;
  }
  .dialog {
    width: min(620px, 92vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  header {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .intro {
    margin: 0;
    padding: 10px 12px 6px;
    color: var(--fg-muted);
  }
  textarea {
    margin: 0 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    resize: vertical;
    background: var(--bg-editor);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .error {
    color: var(--error);
    margin: 6px 12px 0;
  }
  footer {
    display: flex;
    gap: 6px;
    padding: 10px 12px;
  }
  .spacer {
    flex: 1;
  }
</style>
