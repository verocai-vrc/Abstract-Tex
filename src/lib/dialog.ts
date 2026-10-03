// What every modal dialog needs and a bare `<div role="dialog">` does not give: keyboard focus
// inside it while it is open, and back where it was when it closes.
//
// Found under Xvfb (S11.7b's rung 4): the merge view opened with focus still on the Source Control
// row behind it, so `Escape` — handled on the dialog element — did nothing, and typed text landed in
// the editor behind the dialog and was saved to the manuscript. Dialogs that focus an input of their
// own (New project, the palettes) were fine; the viewers and Clone were not.
//
// An action (`use:modalFocus`), because it needs the element and must clean up when it goes. The
// Tab-wrapping arithmetic is a plain function so it is tested without a DOM.

/** Where Tab or Shift Tab goes next among `count` focusable things when `current` has focus, with
 * wrapping at both ends. `current` is `-1` when focus is on the dialog itself (or outside it). */
export function nextFocusIndex(count: number, current: number, backwards: boolean): number {
  if (count <= 0) return -1;
  if (backwards) return current <= 0 ? count - 1 : current - 1;
  return current < 0 || current >= count - 1 ? 0 : current + 1;
}

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href]';

/** Focus `node` now, keep Tab inside it, and give focus back to what had it when it is removed. */
export function modalFocus(node: HTMLElement) {
  const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  node.focus();

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== 'Tab') return;
    const focusable = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((element) => !element.hidden);
    const target = nextFocusIndex(focusable.length, focusable.indexOf(document.activeElement as HTMLElement), event.shiftKey);
    event.preventDefault();
    if (target >= 0) focusable[target]?.focus();
  }
  node.addEventListener('keydown', onKeydown);

  return {
    destroy() {
      node.removeEventListener('keydown', onKeydown);
      if (before?.isConnected) before.focus();
    },
  };
}
