// Keyboard facts that more than one component needs and `shortcuts.ts` (a table of chords) is not
// the place for.

/** The fields of a `KeyboardEvent` that tell whether an input method is in the middle of a word. */
export interface CompositionFacts {
  isComposing?: boolean;
  keyCode?: number;
}

/**
 * Whether this keydown belongs to an input method (Japanese, Chinese, Korean and others) that is
 * composing text, so the field's own handler must leave it alone (R7.4). The Enter that accepts a
 * candidate is the same key as the Enter that means "do it", the arrows choose between candidates,
 * and Escape abandons the composition: a handler that does not ask runs the command on a
 * half-composed name, moves a list under the person's candidate window, or closes the dialog.
 *
 * Two tests, because the engines differ: Chromium marks that keydown `isComposing`; Safari's engine
 * has already ended the composition by then, so `isComposing` is false and the only sign left is the
 * legacy `keyCode` 229, which every engine uses for "an input method handled this key".
 */
export function handledByInputMethod(event: CompositionFacts): boolean {
  return event.isComposing === true || event.keyCode === 229;
}
