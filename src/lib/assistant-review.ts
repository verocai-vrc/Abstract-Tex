// The review of one suggestion (S12.3b): the selection with its changes marked, each one to be
// accepted or left. Pure, so `assistant-review.test.ts` needs no Svelte runtime.
//
// What is shown here is a *preview*. The text that reaches the buffer is made in Rust, by
// `Review::apply`, which checks it against the citation guard; nothing here decides what is allowed.

import type { AssistantHunk } from './ipc';

/** A piece of the selection, in order: text that does not change, or one change. */
export type Segment =
  | { kind: 'same'; text: string }
  | { kind: 'change'; index: number; removed: string; added: string; refusal: string[] | null };

/** The selection cut at the hunks. `original` is sliced by the hunks' UTF-16 ranges, which is what
 * they are expressed in. Hunks arrive in order and never overlap; an out-of-order one is skipped
 * rather than trusted to slice backwards. */
export function segments(original: string, hunks: readonly AssistantHunk[]): Segment[] {
  const parts: Segment[] = [];
  let at = 0;
  hunks.forEach((hunk, index) => {
    if (hunk.start < at || hunk.end < hunk.start || hunk.end > original.length) return;
    if (hunk.start > at) parts.push({ kind: 'same', text: original.slice(at, hunk.start) });
    parts.push({
      kind: 'change',
      index,
      removed: original.slice(hunk.start, hunk.end),
      added: hunk.replacement,
      refusal: hunk.refusal,
    });
    at = hunk.end;
  });
  if (at < original.length) parts.push({ kind: 'same', text: original.slice(at) });
  return parts;
}

/** What each hunk starts as: accepted unless it is refused. */
export function defaultChoices(hunks: readonly AssistantHunk[]): boolean[] {
  return hunks.map((hunk) => hunk.refusal === null);
}

/** Flip one choice, never turning on a refused hunk. A new array: the caller replaces, not edits. */
export function toggled(hunks: readonly AssistantHunk[], choices: readonly boolean[], index: number): boolean[] {
  const hunk = hunks[index];
  if (!hunk || hunk.refusal !== null) return [...choices];
  return choices.map((chosen, i) => (i === index ? !chosen : chosen));
}

/** How many changes are accepted, out of how many may be. */
export function counts(hunks: readonly AssistantHunk[], choices: readonly boolean[]): { accepted: number; allowed: number } {
  return {
    accepted: choices.filter((chosen, i) => chosen && hunks[i]?.refusal === null).length,
    allowed: hunks.filter((hunk) => hunk.refusal === null).length,
  };
}

/** The selection with the chosen changes, for showing — not for applying. */
export function previewText(original: string, hunks: readonly AssistantHunk[], choices: readonly boolean[]): string {
  return segments(original, hunks)
    .map((part) => (part.kind === 'same' ? part.text : choices[part.index] ? part.added : part.removed))
    .join('');
}

/** The one-line summary above the buttons. Says nothing about "0" of a thing that was never offered. */
export function summaryLine(hunks: readonly AssistantHunk[], choices: readonly boolean[]): string {
  if (hunks.length === 0) return 'The assistant suggests no changes.';
  const { accepted, allowed } = counts(hunks, choices);
  const held = hunks.length - allowed;
  const base = `${accepted} of ${allowed} ${allowed === 1 ? 'change' : 'changes'} accepted`;
  return held > 0 ? `${base}; ${held} held back.` : `${base}.`;
}
