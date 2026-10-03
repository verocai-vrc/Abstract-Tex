// The payload inspector's wording (S13.3): what a request carries, said in sentences a person can
// check against what they meant to send. Pure, so `assistant-payload.test.ts` needs no Svelte runtime.
//
// What it must never do: decide what is sent. `AssistantPayload` arrives from Rust, built from the
// request that will be sent; this only labels it.

import type { AssistantPayload, AssistantPayloadPart } from './ipc';

/** What a part is, for the heading above its text. */
export function partLabel(
  part: AssistantPayloadPart,
  position: number,
  parts: readonly AssistantPayloadPart[],
  kind: 'rewrite' | 'explain' = 'rewrite',
): string {
  if (part.label) return part.label;
  if (part.role === 'user') return kind === 'explain' ? 'The lines of your build log' : 'Your request';
  if (part.role === 'assistant') return 'An earlier answer';
  const systemBefore = parts.slice(0, position).filter((other) => other.role === 'system').length;
  if (part.cached) return 'Context the provider is asked to remember';
  return systemBefore === 0 ? 'Instructions to the model' : 'More instructions';
}

/** `1,204 characters (about 402 tokens)`, with "about" because the count is a guess. */
export function sizeLine(payload: AssistantPayload): string {
  const characters = payload.characters.toLocaleString('en-US');
  const tokens = payload.approximateTokens.toLocaleString('en-US');
  return `${characters} ${payload.characters === 1 ? 'character' : 'characters'} (about ${tokens} ${payload.approximateTokens === 1 ? 'token' : 'tokens'})`;
}

/** Where it goes, in one sentence, with the reassurance only when it is true. */
export function destinationLine(payload: AssistantPayload): string {
  return payload.staysOnThisComputer
    ? `To ${payload.destination}, which is this computer: nothing crosses a network.`
    : `To ${payload.destination}, over the internet, with model ${payload.model}.`;
}

/** The request body laid out for reading. Only the whitespace differs from what is sent; falls
 * back to the body as it is if it somehow is not JSON. */
export function prettyBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}

/** Whether the request carries the manuscript itself, not only what was selected. */
export function carriesDocument(payload: AssistantPayload): boolean {
  return payload.parts.some((part) => part.label !== null);
}

/** Whether the request carries more than the selection's own text and the standing instructions:
 * any system part marked cached is context the person did not select. Used to say so in words. */
export function carriesContext(payload: AssistantPayload): boolean {
  return payload.parts.some((part) => part.cached);
}
