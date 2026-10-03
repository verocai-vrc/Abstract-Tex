// The assistant's settings as the window holds them (S12.1b). Only the controller writes to
// `assistant`, as with `github` and `clone`. There is nowhere here to keep a key after it is sent:
// `keyInput` is what is being typed, and the controller empties it as soon as it has been saved.

import { assistantReady, blankForm, formFromProvider, type ProviderForm } from './assistant';
import { defaultChoices } from './assistant-review';
import type { AssistantHunk, AssistantProposal, AssistantStatus } from './ipc';

/** One suggestion under review. Replaced whole on every change, like the other lists. */
export interface ReviewState {
  id: number;
  /** The file the selection was in; applying is refused if another is active by then. */
  path: string;
  /** Where the selection started in that document's text, in UTF-16 units. */
  from: number;
  original: string;
  hunks: AssistantHunk[];
  choices: boolean[];
  actionLabel: string;
  unknownKeys: string[];
  /** Why *Apply* did not go through, as a sentence. */
  error: string | null;
  applying: boolean;
}

export function reviewFrom(proposal: AssistantProposal, path: string, from: number, actionLabel: string): ReviewState {
  return {
    id: proposal.id,
    path,
    from,
    original: proposal.original,
    hunks: proposal.hunks,
    choices: defaultChoices(proposal.hunks),
    actionLabel,
    unknownKeys: proposal.unknownKeys,
    error: null,
    applying: false,
  };
}

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

  /** The author's selection in the active document, or `null` when nothing is selected. Offsets
   * only; the text is read from the document when an action asks for it. */
  selection = $state.raw<{ from: number; to: number } | null>(null);
  /** An action is waiting on the model. */
  asking = $state(false);
  /** The suggestion being reviewed, if any. */
  review = $state.raw<ReviewState | null>(null);

  /** Entry points elsewhere in the app ask this and nothing else. */
  ready = $derived(assistantReady(this.status));
}

export const assistant = new AssistantState();

/** Put the saved provider back into the form, as when the view opens. */
export function resetForm(): void {
  assistant.form = formFromProvider(assistant.status?.provider ?? null);
  assistant.keyInput = '';
}
