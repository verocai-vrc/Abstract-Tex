import { describe, expect, it } from 'vitest';
import { locateFix } from './fix';

describe('locateFix', () => {
  it('finds the edit on a middle line of a multi-line document', () => {
    const text = 'one\nSalt & pepper.\nthree\n';
    const edit = locateFix(text, 2, { description: '', find: '&', replace: '\\&' });
    expect(edit).not.toBeNull();
    expect(text.slice(edit!.from, edit!.to)).toBe('&');
    const result = text.slice(0, edit!.from) + edit!.insert + text.slice(edit!.to);
    expect(result).toBe('one\nSalt \\& pepper.\nthree\n');
  });

  it('finds the edit on the first line', () => {
    const text = 'Salt & pepper.\n';
    const edit = locateFix(text, 1, { description: '', find: '&', replace: '\\&' });
    expect(edit).toEqual({ from: 5, to: 6, insert: '\\&' });
  });

  it('returns null when the line does not contain find', () => {
    const text = 'Hello.\n';
    expect(locateFix(text, 1, { description: '', find: '&', replace: '\\&' })).toBeNull();
  });

  // Review finding R5.14. `texlog` promises that a fix's `find` "cannot match anything but the
  // mistake"; when it nevertheless appears twice on the line there is no way to know which one, and
  // editing the first is how `\ref{sec_intro}` was rewritten instead of `a_n`.
  it('declines when find appears more than once on the line, rather than guessing the first', () => {
    const text = 'See \\ref{sec_intro} and a_n.\n';
    expect(locateFix(text, 1, { description: '', find: '_', replace: '\\_' })).toBeNull();
  });

  it('applies an anchored find to the right underscore when an earlier one exists', () => {
    const text = 'As shown in Section~\\ref{sec_intro}, the value a_n grows.\n';
    const edit = locateFix(text, 1, { description: '', find: 'e value a_', replace: 'e value a\\_' });
    expect(edit).not.toBeNull();
    const result = text.slice(0, edit!.from) + edit!.insert + text.slice(edit!.to);
    expect(result).toBe('As shown in Section~\\ref{sec_intro}, the value a\\_n grows.\n');
  });

  it('returns null when the line number does not exist', () => {
    const text = 'one\ntwo\n';
    expect(locateFix(text, 5, { description: '', find: 'x', replace: 'y' })).toBeNull();
    expect(locateFix(text, 0, { description: '', find: 'x', replace: 'y' })).toBeNull();
  });

  it('replaces a whole-line anchor, the shape verb-unterminated and display-math-wrong-delimiter use', () => {
    const text = '\\documentclass{article}\n\\verb|unterminated\n\\end{document}\n';
    const edit = locateFix(text, 2, {
      description: 'Close with |',
      find: '\\verb|unterminated',
      replace: '\\verb|unterminated|',
    });
    expect(edit).not.toBeNull();
    const result = text.slice(0, edit!.from) + edit!.insert + text.slice(edit!.to);
    expect(result).toBe('\\documentclass{article}\n\\verb|unterminated|\n\\end{document}\n');
  });
});
