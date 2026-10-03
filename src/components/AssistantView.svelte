<script lang="ts">
  // The Assistant view (S12.1b, DESIGN.md §5.5), the third tenant of the left pane: which provider
  // this machine uses, its key, and whether the assistant is on for the open project.
  //
  // Nothing here sends anything except the *Test the connection* button. The key box is write-only:
  // it is emptied the moment the key is saved, and the window is never told what the key was —
  // only whether one is saved.
  import {
    forgetAssistantKey,
    saveAssistantSettings,
    setAssistantEnabled,
    setAssistantInspectFirst,
    testAssistant,
  } from '../lib/controller.svelte';
  import { assistant } from '../lib/assistant.svelte';
  import { blankForm, describeStatus, keyIsOptional, type ProviderKind } from '../lib/assistant';
  import { app } from '../lib/state.svelte';

  const status = $derived(assistant.status);
  const summary = $derived(describeStatus(status, app.project !== null));
  const optionalKey = $derived(keyIsOptional(assistant.form.kind));
  const configured = $derived(status?.provider != null && (status.hasKey || keyIsOptional(status.provider.kind)));

  function chooseKind(kind: ProviderKind) {
    if (assistant.form.kind === kind) return;
    assistant.form = blankForm(kind);
    assistant.keyInput = '';
  }
</script>

<aside class="sidebar assistant" aria-label="Assistant">
  <div class="sidebar-head"><span class="label">Assistant</span></div>

  <p class="hint">{summary}</p>

  <section class="group" aria-label="Provider">
    <div class="kinds" role="group" aria-label="Kind of provider">
      <button
        class="kind"
        class:on={assistant.form.kind === 'anthropic'}
        aria-pressed={assistant.form.kind === 'anthropic'}
        onclick={() => chooseKind('anthropic')}>Anthropic</button
      >
      <button
        class="kind"
        class:on={assistant.form.kind === 'openAiCompatible'}
        aria-pressed={assistant.form.kind === 'openAiCompatible'}
        onclick={() => chooseKind('openAiCompatible')}>Other (OpenAI-compatible)</button
      >
    </div>

    {#if assistant.form.kind === 'openAiCompatible'}
      <label for="assistant-address">Address</label>
      <input
        id="assistant-address"
        type="text"
        bind:value={assistant.form.address}
        placeholder="http://localhost:11434/v1"
        spellcheck="false"
        autocomplete="off"
      />
      <p class="note">A model on this computer (Ollama, LM Studio, llama.cpp) or any service that speaks the OpenAI chat format.</p>
    {/if}

    <label for="assistant-model">Model</label>
    <input
      id="assistant-model"
      type="text"
      bind:value={assistant.form.model}
      placeholder={assistant.form.kind === 'anthropic' ? 'claude-sonnet-5-5' : 'the model’s name'}
      spellcheck="false"
      autocomplete="off"
    />

    <label for="assistant-key">API key{optionalKey ? ' (optional)' : ''}</label>
    <input
      id="assistant-key"
      type="password"
      bind:value={assistant.keyInput}
      placeholder={status?.hasKey ? 'A key is saved. Paste a new one to replace it.' : optionalKey ? 'Not needed for a model on this computer' : 'Paste your key'}
      spellcheck="false"
      autocomplete="off"
    />
    <p class="note">Kept in this computer’s keychain, never in a project, and never shown again.</p>

    <div class="buttons">
      <button class="primary" disabled={assistant.busy} onclick={() => void saveAssistantSettings()}>Save</button>
      {#if status?.hasKey}
        <button class="ghost" disabled={assistant.busy} onclick={() => void forgetAssistantKey()}>Remove key</button>
      {/if}
    </div>
  </section>

  <section class="group" aria-label="This project">
    <label class="switch">
      <input
        type="checkbox"
        checked={status?.enabled ?? false}
        disabled={!app.project || !configured}
        onchange={(event) => void setAssistantEnabled(event.currentTarget.checked)}
      />
      <span>Use the assistant in this project</span>
    </label>
    <p class="note">
      {#if !app.project}
        Open a project to switch this on for it.
      {:else if !configured}
        Finish the provider above first.
      {:else}
        Only on this computer; a copy of the project elsewhere starts off.
      {/if}
    </p>
  </section>

  <section class="group" aria-label="Requests">
    <label class="switch">
      <input
        type="checkbox"
        checked={status?.inspectFirst ?? true}
        onchange={(event) => void setAssistantInspectFirst(event.currentTarget.checked)}
      />
      <span>Show me each request before it is sent</span>
    </label>
    <p class="note">
      A window shows exactly what would leave this computer, text and model name, and sends it only when you press Send. Off, an
      assistant action sends as soon as you choose it.
    </p>
  </section>

  <section class="group" aria-label="Connection">
    <button class="ghost" disabled={assistant.busy || !configured} onclick={() => void testAssistant()}>
      Test the connection
    </button>
    <p class="note">The only thing here that sends anything: a few words to the provider, to see that the address, model and key work.</p>
  </section>

  {#if assistant.message}
    <p class="hint" class:error={assistant.messageIsError} role="status">{assistant.message}</p>
  {/if}
</aside>

<style>
  .assistant {
    overflow-y: auto;
  }
  .sidebar-head {
    display: flex;
    align-items: center;
    padding: 2px 6px 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .hint {
    color: var(--fg-muted);
    padding: 4px 12px 8px;
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
  }
  .hint.error {
    color: var(--error);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 12px 10px;
    border-top: 1px solid var(--border);
  }
  label {
    font-size: 12px;
    color: var(--fg-muted);
    margin-top: 4px;
  }
  .note {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
    color: var(--fg-muted);
    opacity: 0.85;
  }
  input[type='text'],
  input[type='password'] {
    width: 100%;
    font: inherit;
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-editor);
    color: var(--fg);
  }
  input::placeholder {
    color: var(--fg-muted);
    opacity: 0.55;
    font-style: italic;
  }
  .kinds {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .kind {
    padding: 2px 10px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .kind.on {
    background: var(--bg-selected);
    color: var(--fg);
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--fg);
    font-size: 13px;
    margin-top: 0;
  }
</style>
