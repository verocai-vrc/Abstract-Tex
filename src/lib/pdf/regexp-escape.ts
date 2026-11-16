// `RegExp.escape` for webviews that do not have it yet (ES2025: Chrome 136, Firefox 134, Safari 18.2).
//
// Why this exists: pdf.js 5.7 builds its search pattern with `RegExp.escape` whenever the query has
// punctuation in it ("non-trivial", "e-mail", "U.S.A.", "C++", "O(n)"). On a webview older than that (the
// WebKitGTK of Ubuntu 24.04, a Mac that has not taken Safari 18.2's WebKit) the call throws inside pdf.js,
// inside a promise nobody sees, and the find bar waits on "Searching…" for ever. The test runner, Node 22,
// lacks it too, which is how this was found. A webview that has it is left alone.
//
// The escaping follows the specification, which is stricter than "put a backslash before anything odd":
// pdf.js's patterns carry the `u` flag, where `\-` is a syntax error, so a hyphen must be `\x2d`.

/** Characters that are syntax in a pattern: a backslash in front makes them plain. (The spec adds `/`.) */
const SYNTAX_CHARACTERS = new Set('^$\\.*+?()[]{}|/');

/** Other punctuation the spec writes as a hex escape, so it can never join its neighbours into syntax
 * (a `-` between two characters of a class, a `,` inside `{1,2}`). */
const OTHER_PUNCTUATORS = new Set(',-=<>#&!%:;@~\'`"');

const CONTROL_ESCAPES: Record<string, string> = { '\t': '\\t', '\n': '\\n', '\v': '\\v', '\f': '\\f', '\r': '\\r' };

/** `\x2d` for a code point up to 0xFF, ` ` above it. */
function hexEscape(codeUnit: number): string {
  return codeUnit <= 0xff
    ? `\\x${codeUnit.toString(16).padStart(2, '0')}`
    : `\\u${codeUnit.toString(16).padStart(4, '0')}`;
}

/** `text`, written so that a regular expression made from it matches that text and nothing else. */
export function escapeForRegExp(text: string): string {
  let escaped = '';
  let atStart = true;
  // `for…of` walks code points, so a character outside the Basic Multilingual Plane (an emoji) stays whole.
  for (const character of text) {
    const codePoint = character.codePointAt(0) ?? 0;
    const isLoneSurrogate = character.length === 1 && codePoint >= 0xd800 && codePoint <= 0xdfff;
    if (atStart && /[0-9A-Za-z]/.test(character)) {
      // A leading digit or letter could otherwise fuse with something written before the pattern
      // (`\1` followed by `2`, `\c` followed by `A`).
      escaped += hexEscape(codePoint);
    } else if (SYNTAX_CHARACTERS.has(character)) {
      escaped += `\\${character}`;
    } else if (character in CONTROL_ESCAPES) {
      escaped += CONTROL_ESCAPES[character];
    } else if (OTHER_PUNCTUATORS.has(character) || /\s/u.test(character) || isLoneSurrogate) {
      escaped += hexEscape(codePoint);
    } else {
      escaped += character;
    }
    atStart = false;
  }
  return escaped;
}

/** Give the webview a `RegExp.escape` if it has none. Safe to call any number of times. */
export function installRegExpEscape(): void {
  const regExp = RegExp as unknown as { escape?: (text: string) => string };
  if (typeof regExp.escape !== 'function') regExp.escape = escapeForRegExp;
}
