// The assistant's settings form (S12.1b): turning what someone typed into a provider, and the status
// into sentences. Pure, so `assistant.test.ts` needs no Svelte runtime.
//
// What it must never do: hold a key. The key is a field of the *form* for as long as the person is
// typing it and is cleared the moment it is sent; nothing here reads one back, because the backend
// never sends one (`src-tauri/src/assistant.rs`).

import type { AssistantProvider, AssistantStatus } from './ipc';

export type ProviderKind = AssistantProvider['kind'];

export const ANTHROPIC_ADDRESS = 'https://api.anthropic.com';
const LOCAL_MODEL_ADDRESS = 'http://localhost:11434/v1';

/** What the three fields hold. `address` is only used by the OpenAI-compatible kind. */
export interface ProviderForm {
  kind: ProviderKind;
  model: string;
  address: string;
}

/** A model name to suggest, which the person is expected to change: models come and go. */
export const SUGGESTED_MODEL: Record<ProviderKind, string> = {
  anthropic: 'claude-sonnet-5-5',
  openAiCompatible: '',
};

export function blankForm(kind: ProviderKind): ProviderForm {
  return {
    kind,
    model: SUGGESTED_MODEL[kind],
    address: kind === 'anthropic' ? ANTHROPIC_ADDRESS : LOCAL_MODEL_ADDRESS,
  };
}

export function formFromProvider(provider: AssistantProvider | null): ProviderForm {
  return provider ? { kind: provider.kind, model: provider.model, address: provider.address } : blankForm('anthropic');
}

/** The provider for the form, or the sentence saying what is missing. The backend checks the
 * address properly; this only spares a round trip for what is obviously empty. */
export function providerFromForm(form: ProviderForm): AssistantProvider | string {
  const model = form.model.trim();
  if (!model) return 'Choose a model: the provider needs to be told which one to use.';
  if (form.kind === 'anthropic') return { kind: 'anthropic', model, address: ANTHROPIC_ADDRESS };
  const address = form.address.trim();
  if (!address) return 'Give the address of the model server, for example http://localhost:11434/v1.';
  return { kind: 'openAiCompatible', model, address };
}

/** Whether the provider can work without a key: a model on this computer has none. */
export function keyIsOptional(kind: ProviderKind): boolean {
  return kind === 'openAiCompatible';
}

/** One sentence about where things stand, for the top of the view. Never says "0" of anything. */
export function describeStatus(status: AssistantStatus | null, hasProject: boolean): string {
  if (!status || !status.provider) return 'Not set up. Choose a provider below; nothing is sent until you ask.';
  const needsKey = !status.hasKey && !keyIsOptional(status.provider.kind);
  if (needsKey) return 'Add an API key to finish setting up.';
  if (!hasProject) return 'Ready. Open a project to switch the assistant on for it.';
  return status.enabled
    ? 'On for this project, on this machine. Nothing is sent until you ask for something.'
    : 'Set up, and off for this project.';
}

/** Whether assistant features may appear anywhere in the app: a provider is chosen, a key is there
 * if one is needed, and this project is switched on. Anything else and the entry points are absent. */
export function assistantReady(status: AssistantStatus | null): boolean {
  if (!status || !status.provider || !status.enabled) return false;
  return status.hasKey || keyIsOptional(status.provider.kind);
}
