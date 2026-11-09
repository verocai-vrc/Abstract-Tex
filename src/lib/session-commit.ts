// The sentence offered as the commit message when a live session ends (S15.2). Plain functions, so
// the test needs no Svelte runtime and no clock.

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'] as const;

/** `Bea`, `Bea and Cy`, `Ada, Bea and Cy`: the people the session was with, in the order given. */
export function namesInProse(names: readonly string[]): string {
  if (names.length <= 1) return names[0] ?? '';
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

/** `Live session with Bea, 9 Nov`; `Live session, 9 Nov` when nobody else was there at the end.
 * `now` is an argument so a test does not depend on the day it runs. */
export function sessionCommitMessage(peerNames: readonly string[], now: Date): string {
  const unique = [...new Set(peerNames.filter((name) => name.trim() !== ''))];
  const date = `${now.getDate()} ${MONTHS[now.getMonth()]}`;
  return unique.length === 0 ? `Live session, ${date}` : `Live session with ${namesInProse(unique)}, ${date}`;
}
