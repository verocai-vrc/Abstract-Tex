import { describe, expect, it } from 'vitest';
import { handledByInputMethod, pressesEnter } from './keys';

describe('handledByInputMethod (R7.4)', () => {
  it('is true for the Enter an input method uses to accept a candidate, in both engines', () => {
    // Chromium: the keydown carries isComposing.
    expect(handledByInputMethod({ isComposing: true, keyCode: 13 })).toBe(true);
    // Safari's engine: compositionend has already fired, so isComposing is false, and the keyCode is 229.
    expect(handledByInputMethod({ isComposing: false, keyCode: 229 })).toBe(true);
  });

  it('is false for an ordinary Enter', () => {
    expect(handledByInputMethod({ isComposing: false, keyCode: 13 })).toBe(false);
    expect(handledByInputMethod({})).toBe(false); // a test double with neither field
  });
});

describe('pressesEnter', () => {
  it('is a press of it, not its repeat while it is held, nor an input method’s', () => {
    expect(pressesEnter({ key: 'Enter' })).toBe(true);
    expect(pressesEnter({ key: 'Enter', repeat: true })).toBe(false);
    expect(pressesEnter({ key: 'Enter', isComposing: true })).toBe(false);
    expect(pressesEnter({ key: 'a' })).toBe(false);
  });
});
