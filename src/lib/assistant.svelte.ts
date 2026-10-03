// The assistant's settings as the window holds them (S12.1b). Only the controller writes to
// `assistant`, as with `github` and `clone`. There is nowhere here to keep a key after it is sent:
// `keyInput` is what is being typed, and the controller empties it as soon as it has been saved.

import { assistantReady, blankForm, formFromProvider, type ProviderForm } from './assistant';
import type { AssistantStatus } from './ipc';

class AssistantState {
  /** `null` until asked, and again when no project is open to ask about. */
  status = $state.raw<AssistantStatus | null>(null);

  form = $state<ProviderForm>(blankForm('anthropic'));
  /** The key as it is typed. Cleared on save; never filled from anywhere. */
  keyInput = $state('');

  /** A request is out (saving, or testing the connection). */
  busy = $state(false);
  /** The last thing worth saying, as a sentence: a refusal, or the test's answer. */
  message = $state<string | null>(null);
  messageIsError = $state(false);

  /** Entry points elsewhere in the app ask this and nothing else. */
  ready = $derived(assistantReady(this.status));
}

export const assistant = new AssistantState();

/** Put the saved provider back into the form, as when the view opens. */
export function resetForm(): void {
  assistant.form = formFromProvider(assistant.status?.provider ?? null);
  assistant.keyInput = '';
}
