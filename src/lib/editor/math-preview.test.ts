import { Text } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import { blankComments, mathAtOffset, mathPreviewSource, renderMath } from './math-preview';

describe('mathAtOffset', () => {
  it('finds inline $n$ and strips the delimiters from tex', () => {
    const doc = 'Let $n$ be an integer.';
    expect(mathAtOffset(Text.of(doc.split('\n')), 5)).toEqual({ from: 4, to: 7, tex: 'n', display: false });
  });

  it('finds \\(…\\) as inline maths', () => {
    const doc = 'and \\(x + y\\) here';
    expect(mathAtOffset(Text.of(doc.split('\n')), 7)).toEqual({ from: 4, to: 13, tex: 'x + y', display: false });
  });

  it('finds \\[…\\] as display maths', () => {
    const doc = 'so\n\\[\n  a^2 + b^2 = c^2\n\\]\nand';
    const span = mathAtOffset(Text.of(doc.split('\n')), 8);
    expect(span).toEqual({ from: 3, to: 26, tex: '\n  a^2 + b^2 = c^2\n', display: true });
  });

  it('finds $$…$$ as display maths, not two empty $…$ spans', () => {
    const doc = 'then $$E = mc^2$$ follows';
    expect(mathAtOffset(Text.of(doc.split('\n')), 9)).toEqual({ from: 5, to: 17, tex: 'E = mc^2', display: true });
  });

  it('keeps the \\begin/\\end wrapper for an align* environment', () => {
    const doc = 'We have\n\\begin{align*}\n  a &= b \\\\\n  c &= d\n\\end{align*}\nas claimed.';
    const span = mathAtOffset(Text.of(doc.split('\n')), 30);
    expect(span).not.toBeNull();
    expect(span!.display).toBe(true);
    expect(span!.tex).toBe('\\begin{align*}\n  a &= b \\\\\n  c &= d\n\\end{align*}');
    expect(doc.slice(span!.from, span!.to)).toBe(span!.tex);
  });

  it('does not treat a non-maths environment as maths', () => {
    const doc = '\\begin{itemize}\n\\item one\n\\end{itemize}';
    expect(mathAtOffset(Text.of(doc.split('\n')), 20)).toBeNull();
  });

  it('ignores an escaped \\$ in prose', () => {
    const doc = 'It costs \\$5 and \\$7 today.';
    expect(mathAtOffset(Text.of(doc.split('\n')), 11)).toBeNull();
    expect(mathAtOffset(Text.of(doc.split('\n')), 19)).toBeNull();
  });

  it('is null for prose between two maths spans', () => {
    const doc = 'both $a$ and $b$ hold';
    expect(mathAtOffset(Text.of(doc.split('\n')), 10)).toBeNull();
  });

  it('is null for an unclosed $', () => {
    const doc = 'a lone $x with no closer';
    expect(mathAtOffset(Text.of(doc.split('\n')), 9)).toBeNull();
  });

  it('still finds a closed span that precedes an unclosed one', () => {
    const doc = 'good $a$ then bad $b';
    expect(mathAtOffset(Text.of(doc.split('\n')), 6)).toEqual({ from: 5, to: 8, tex: 'a', display: false });
    expect(mathAtOffset(Text.of(doc.split('\n')), 19)).toBeNull();
  });

  it('counts an offset on either delimiter as inside', () => {
    const doc = 'x $ab$ y';
    expect(mathAtOffset(Text.of(doc.split('\n')), 2)!.tex).toBe('ab'); // on the opening $
    expect(mathAtOffset(Text.of(doc.split('\n')), 5)!.tex).toBe('ab'); // on the closing $
    expect(mathAtOffset(Text.of(doc.split('\n')), 1)).toBeNull(); // the space before it
    expect(mathAtOffset(Text.of(doc.split('\n')), 7)).toBeNull(); // the space after it
  });

  it('bounds the scan to the paragraph: a $ left open in one paragraph does not reach the next', () => {
    const doc = 'broken $x here\n\nnext $y$ fine';
    expect(mathAtOffset(Text.of(doc.split('\n')), 22)).toEqual({ from: 21, to: 24, tex: 'y', display: false });
  });

  it('is null for an expression longer than 2000 characters', () => {
    const doc = `$${'x'.repeat(2001)}$`;
    expect(mathAtOffset(Text.of(doc.split('\n')), 10)).toBeNull();
  });
});

describe('comments in the maths scan', () => {
  it('blanks a comment to the end of its line and keeps every offset', () => {
    const text = 'a % price is $5\nb';
    const blanked = blankComments(text);
    expect(blanked).toHaveLength(text.length);
    expect(blanked).toBe('a ' + ' '.repeat(13) + '\nb');
  });

  it('keeps an escaped percent sign, and starts a comment after a line break', () => {
    expect(blankComments('50\\% off')).toBe('50\\% off'); // \% is a percent sign
    expect(blankComments('a \\\\% $x$')).toBe('a \\\\' + ' '.repeat(5)); // \\ then a comment
  });

  it('a $ inside a comment does not shift the pairing of the real ones', () => {
    const doc = '% price is $5\nThe value $x$ is';
    const at = doc.indexOf('x');
    expect(mathAtOffset(Text.of(doc.split('\n')), at)).toEqual({ from: at - 1, to: at + 2, tex: 'x', display: false });
    // And the prose before the formula is prose, not a formula that starts in the comment.
    expect(mathAtOffset(Text.of(doc.split('\n')), doc.indexOf('The'))).toBeNull();
  });

  it('still finds a formula that has a comment inside it, and hands KaTeX the comment too', () => {
    const doc = '$a % why\n+ b$';
    expect(mathAtOffset(Text.of(doc.split('\n')), 1)?.tex).toBe('a % why\n+ b');
  });
});

describe('renderMath', () => {
  it('does not warn about a non-ASCII glyph (strict: ignore)', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const result = renderMath('é + \\alpha', false);
    expect('html' in result).toBe(true);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });

  it('renders \\frac{a}{b} to KaTeX HTML', () => {
    const result = renderMath('\\frac{a}{b}', false);
    expect('html' in result && result.html).toContain('katex');
  });

  it('reports an unclosed brace as a one-line message, without throwing', () => {
    const result = renderMath('\\frac{a}{b', false);
    expect('error' in result).toBe(true);
    const error = (result as { error: string }).error;
    expect(error).toMatch(/expected '}'/);
    expect(error).not.toMatch(/^KaTeX parse error/);
    expect(error).not.toContain(' at position ');
    expect(error).not.toContain('\n');
  });

  it('reports an undefined macro as a one-line message, without throwing', () => {
    const result = renderMath('\\undefinedmacro', false);
    expect(result).toEqual({ error: 'Undefined control sequence: \\undefinedmacro' });
  });

  it('renders an align* environment with its wrapper in display mode', () => {
    const result = renderMath('\\begin{align*}a &= b \\\\ c &= d\\end{align*}', true);
    expect('html' in result).toBe(true);
  });

  it('renders multline by substituting the environment KaTeX lacks', () => {
    const result = renderMath('\\begin{multline}a + b \\\\ + c\\end{multline}', true);
    expect('html' in result).toBe(true);
  });
});

/** Same Node-only stand-in as `hover.test.ts`: `mathPreviewSource` only reads `view.state.doc`,
 * and this Vitest environment has no DOM to build a real `EditorView` in. */
function viewWithDoc(doc: string): EditorView {
  return { state: { doc: Text.of(doc.split('\n')) } } as unknown as EditorView;
}

describe('mathPreviewSource', () => {
  it('anchors the tooltip on the whole span, delimiters included', () => {
    const tooltip = mathPreviewSource(viewWithDoc('Let $n$ be'), 5);
    expect(tooltip).not.toBeNull();
    expect(tooltip!.pos).toBe(4);
    expect(tooltip!.end).toBe(7);
    expect(tooltip!.above).toBe(true);
    // `create` needs `document`, which this environment lacks (see `hover.test.ts`).
    expect(typeof tooltip!.create).toBe('function');
  });

  it('returns null over prose', () => {
    expect(mathPreviewSource(viewWithDoc('Let $n$ be'), 1)).toBeNull();
  });
});
