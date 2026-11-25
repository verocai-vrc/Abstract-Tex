// Whether there is room for three panes (S17.13). `app.css` folds them with the same words, which
// `layout.test.ts` sees to; the window's actions ask here, where it matters which pane is in sight.

/** The media query under which the window shows one pane at a time. */
export const FOLDED = '(max-width: 923px)';

export const folded = (): boolean => globalThis.matchMedia?.(FOLDED).matches ?? false;
