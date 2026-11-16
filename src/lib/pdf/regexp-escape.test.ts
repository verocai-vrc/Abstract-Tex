// `RegExp.escape` for webviews that do not have it yet.
//
// pdf.js 5.7 builds its search pattern with `RegExp.escape` (ES2025: Chrome 136, Firefox 134, Safari 18.2).
// A webview older than that, such as the WebKitGTK of Ubuntu 24.04 or a Mac that has not taken Safari
// 18.2's WebKit, throws inside pdf.js on any query with punctuation in it, and nothing ever answers: the
// find bar would say "Searching…" for "non-trivial" for ever. Node 22, which runs these tests, lacks it too.

import { describe, expect, it } from 'vitest';
import { escapeForRegExp, installRegExpEscape } from './regexp-escape';

/** The pattern, anchored, with the `u` flag pdf.js's patterns use: strict about what may be escaped. */
function matcher(text: string): RegExp {
  return new RegExp(`^${escapeForRegExp(text)}$`, 'u');
}

describe('escapeForRegExp', () => {
  it('makes a pattern that matches the text itself, for every ASCII punctuation mark', () => {
    for (let code = 0x20; code < 0x7f; code++) {
      const text = `a${String.fromCharCode(code)}b`;
      expect(matcher(text).test(text), JSON.stringify(text)).toBe(true);
    }
  });

  it('matches nothing but the text itself', () => {
    expect(matcher('a.b').test('axb')).toBe(false);
    expect(matcher('C++').test('CC')).toBe(false);
    expect(matcher('(n)').test('n')).toBe(false);
    expect(matcher('a|b').test('a')).toBe(false);
    expect(matcher('non-trivial').test('nontrivial')).toBe(false);
  });

  it('is safe at the start of the text, where a digit or a letter could extend an earlier escape', () => {
    expect(matcher('1.5').test('1.5')).toBe(true);
    expect(matcher('e-mail').test('e-mail')).toBe(true);
  });

  it('handles spaces, tabs, line breaks and non-ASCII text', () => {
    for (const text of ['a b', 'a\tb', 'a\nb', 'a b', 'Erdős', '日本語', 'a b', '😀']) {
      expect(matcher(text).test(text), JSON.stringify(text)).toBe(true);
    }
  });

  it('escapes a hyphen the way a `u`-flag pattern allows (`\\-` outside a class is an error there)', () => {
    expect(() => new RegExp(escapeForRegExp('-'), 'u')).not.toThrow();
  });

  it('writes what the specification writes (checked by hand against its examples)', () => {
    // A leading letter or digit is a hex escape, other punctuation after it is `\x..`, syntax is `\.`.
    expect(escapeForRegExp('foo')).toBe('\\x66oo');
    expect(escapeForRegExp('foo.bar')).toBe('\\x66oo\\.bar');
    expect(escapeForRegExp('non-trivial')).toBe('\\x6eon\\x2dtrivial');
    expect(escapeForRegExp('1.5')).toBe('\\x31\\.5');
    expect(escapeForRegExp('a b')).toBe('\\x61\\x20b');
    expect(escapeForRegExp('a\nb')).toBe('\\x61\\nb');
    expect(escapeForRegExp('')).toBe('');
  });

  it('agrees with the engine\'s own RegExp.escape where there is one', () => {
    const native = (RegExp as unknown as { escape?: (text: string) => string }).escape;
    if (native === undefined) return; // Node 22: nothing to compare with
    for (const text of ['non-trivial', 'a.b', 'C++', '1.5', 'x y', 'Erdős', '(n)', '[12]', '$5', '']) {
      expect(escapeForRegExp(text), JSON.stringify(text)).toBe(native(text));
    }
  });
});

describe('installRegExpEscape', () => {
  const holder = RegExp as unknown as { escape?: (text: string) => string };
  // Read through a function: after `delete holder.escape` TypeScript believes the property is gone for
  // good and types any later read of it as `never`, although `installRegExpEscape` has put it back.
  const current = () => holder.escape;

  it('adds RegExp.escape when the engine has none', () => {
    const original = holder.escape;
    try {
      delete holder.escape;
      installRegExpEscape();
      expect(typeof current()).toBe('function');
      expect(current()?.('a-b')).toBe(escapeForRegExp('a-b'));
    } finally {
      if (original) holder.escape = original;
      else delete holder.escape;
    }
  });

  it('leaves the engine\'s own RegExp.escape alone', () => {
    const original = holder.escape;
    const own = (text: string) => `own:${text}`;
    try {
      holder.escape = own;
      installRegExpEscape();
      expect(holder.escape).toBe(own);
    } finally {
      if (original) holder.escape = original;
      else delete holder.escape;
    }
  });
});
