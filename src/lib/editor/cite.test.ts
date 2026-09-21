import { autocompletion, CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it, vi } from 'vitest';
import type { BibEntrySummary } from '../ipc';
import { authorLabel, citeContextAt, citeLabel, citeSource, citeThenLsp, splitNames } from './cite';

/** A minimal entry with only the fields a test cares about; every other field gets a harmless
 * default so call sites read as a diff against the one thing under test. */
function entry(fields: Partial<BibEntrySummary> & { key: string }): BibEntrySummary {
  return {
    entryType: 'article',
    author: null,
    year: null,
    title: null,
    file: 'refs.bib',
    span: { start: 0, end: 0 },
    ...fields,
  };
}

/** Same helper `completion.test.ts` uses: a bare `CompletionContext` over a real `EditorState`,
 * no mounted editor needed since `citeSource` only reads `state.sliceDoc`/`context.pos`. */
function contextAt(doc: string, pos: number, explicit = true): CompletionContext {
  const state = EditorState.create({ doc, extensions: [autocompletion()] });
  return new CompletionContext(state, pos, explicit);
}

describe('splitNames', () => {
  it('splits "Last, First" form', () => {
    expect(splitNames('Smith, Jane')).toEqual([{ first: 'Jane', last: 'Smith' }]);
  });

  it('splits "First Last" form, taking the final word as the surname', () => {
    expect(splitNames('Jane Smith')).toEqual([{ first: 'Jane', last: 'Smith' }]);
  });

  it('splits multiple names joined by "and"', () => {
    expect(splitNames('Smith, Jane and Doe, John')).toEqual([
      { first: 'Jane', last: 'Smith' },
      { first: 'John', last: 'Doe' },
    ]);
  });

  it('is case-insensitive and whitespace-tolerant about the "and" separator', () => {
    expect(splitNames('Jane Smith AND John Doe')).toEqual([
      { first: 'Jane', last: 'Smith' },
      { first: 'John', last: 'Doe' },
    ]);
  });

  it('does not split inside a surname that merely contains the letters "and"', () => {
    expect(splitNames('Anderson, Paul')).toEqual([{ first: 'Paul', last: 'Anderson' }]);
  });

  it('keeps a braced corporate name whole, as the surname, with no first name', () => {
    expect(splitNames('{World Health Organization}')).toEqual([{ first: '', last: 'World Health Organization' }]);
  });

  it('drops a trailing "and others" rather than treating it as a name', () => {
    expect(splitNames('Smith, Jane and others')).toEqual([{ first: 'Jane', last: 'Smith' }]);
  });

  it('carries a "Jr"-style suffix back into the first-name half of a "Last, Suffix, First" name', () => {
    expect(splitNames('King, Jr, Martin Luther')).toEqual([{ first: 'Jr Martin Luther', last: 'King' }]);
  });

  it('returns an empty list for null or empty input', () => {
    expect(splitNames(null)).toEqual([]);
    expect(splitNames('')).toEqual([]);
  });
});

describe('authorLabel', () => {
  it('shows a single surname plain', () => {
    expect(authorLabel('Smith, Jane')).toBe('Smith');
  });

  it('joins two surnames with an ampersand', () => {
    expect(authorLabel('Smith, Jane and Doe, John')).toBe('Smith & Doe');
  });

  it('collapses three or more surnames to "First et al."', () => {
    expect(authorLabel('Smith, Jane and Doe, John and Lee, Amy')).toBe('Smith et al.');
  });

  it('says "et al." for a field that explicitly ends "and others", even with only one named author', () => {
    expect(authorLabel('Smith, Jane and others')).toBe('Smith et al.');
  });

  it('returns empty for no author', () => {
    expect(authorLabel(null)).toBe('');
  });
});

describe('citeLabel', () => {
  it('combines author and year', () => {
    expect(citeLabel(entry({ key: 'k', author: 'Smith, Jane', year: '2019' }))).toBe('Smith (2019)');
  });

  it('falls back to the author alone with no year', () => {
    expect(citeLabel(entry({ key: 'k', author: 'Smith, Jane', year: null }))).toBe('Smith');
  });

  it('falls back to the year alone with no author', () => {
    expect(citeLabel(entry({ key: 'k', author: null, year: '2019' }))).toBe('2019');
  });

  it('is empty with neither', () => {
    expect(citeLabel(entry({ key: 'k' }))).toBe('');
  });
});

describe('citeContextAt', () => {
  it('finds the context right after \\cite{', () => {
    const text = '\\cite{';
    const found = citeContextAt(text, text.length);
    expect(found).toEqual({ from: text.length, partial: '' });
  });

  it('finds the partial key typed so far', () => {
    const text = '\\cite{smi';
    const found = citeContextAt(text, text.length);
    expect(found).toEqual({ from: 6, partial: 'smi' });
  });

  it('completes only the key under the cursor in a multi-key list, not the whole list', () => {
    const text = '\\cite{smith2019,do';
    const found = citeContextAt(text, text.length);
    expect(found).toEqual({ from: 16, partial: 'do' });
  });

  it('skips a leading space after a comma when reporting where the partial key starts', () => {
    const text = '\\cite{smith2019, do';
    const found = citeContextAt(text, text.length);
    expect(found).toEqual({ from: 17, partial: 'do' });
  });

  it('matches the whole cite family, not just \\cite itself', () => {
    for (const command of ['\\citep', '\\parencite', '\\Textcite', '\\autocite', '\\footcites', '\\nocite']) {
      const text = `${command}{`;
      expect(citeContextAt(text, text.length)).toEqual({ from: text.length, partial: '' });
    }
  });

  it('accepts a starred variant', () => {
    const text = '\\cite*{a';
    expect(citeContextAt(text, text.length)).toEqual({ from: 7, partial: 'a' });
  });

  it('accepts one or more [...] options before the brace', () => {
    const text = '\\citep[see][12]{a';
    expect(citeContextAt(text, text.length)).toEqual({ from: 16, partial: 'a' });
  });

  it('returns null once the argument has been closed', () => {
    const text = '\\cite{a} ';
    expect(citeContextAt(text, text.length)).toBeNull();
  });

  it('returns null for a brace that is not a cite command at all', () => {
    const text = '\\textbf{a';
    expect(citeContextAt(text, text.length)).toBeNull();
  });

  it('is not fooled by an escaped backslash: "\\\\cite{" is a line break then the word "cite{", not a command', () => {
    // Mirrors `bibliography.rs`'s `scan_citations_is_not_fooled_by_commented_or_escaped_text`:
    // the pair of backslashes is TeX's own line-break command, and "cite{" that follows is
    // ordinary text, not `\cite` with its backslash escaped.
    const text = '\\\\cite{a';
    expect(citeContextAt(text, text.length)).toBeNull();
  });

  it('still recognises a real cite command right after a genuine line-break command', () => {
    // The backslash run before the *real* `\cite` here is exactly one (`\\` then `\cite`), which
    // is odd — the opposite parity from the case above — so this must still match.
    const text = '\\\\\\cite{a';
    expect(citeContextAt(text, text.length)).toEqual({ from: 8, partial: 'a' });
  });

  it('returns null for a second, unrelated group after a closed cite (prose, not a second key)', () => {
    const text = '\\cite{a} {\\bf b';
    expect(citeContextAt(text, text.length)).toBeNull();
  });

  it('returns null with no open brace on the line at all', () => {
    expect(citeContextAt('just some text', 9)).toBeNull();
  });

  it('does not reach across a line break into a cite command on the previous line', () => {
    const text = '\\cite{a\nb';
    expect(citeContextAt(text, text.length)).toBeNull();
  });

  it('finds the context mid-document, not only at the end of the string', () => {
    const text = '\\cite{smi} and more text after';
    expect(citeContextAt(text, 9)).toEqual({ from: 6, partial: 'smi' });
  });
});

describe('citeSource', () => {
  const bibliography = [
    entry({ key: 'smith2019', author: 'Smith, Jane', year: '2019', title: 'On Widgets' }),
    entry({ key: 'doe2020', author: 'Doe, John', year: '2020', title: 'Gadgets Reconsidered' }),
  ];

  it('offers every entry, ranked, right after an open \\cite{', async () => {
    const source = citeSource(() => bibliography);
    const text = '\\cite{';
    const result = await source(contextAt(text, text.length));

    expect(result).not.toBeNull();
    expect(result!.from).toBe(text.length);
    expect(result!.to).toBe(text.length);
    const keys = result!.options.map((option) => option.apply);
    expect(keys).toContain('smith2019');
    expect(keys).toContain('doe2020');
  });

  it('inserts the bare key, with the author/year/title only in the displayed label', async () => {
    const source = citeSource(() => bibliography);
    const text = '\\cite{smith';
    const result = await source(contextAt(text, text.length));

    const option = result!.options.find((o) => o.apply === 'smith2019')!;
    expect(option.apply).toBe('smith2019');
    expect(option.label).toBe('smith2019');
    expect(option.displayLabel).toContain('Smith (2019)');
    expect(option.detail).toBe('On Widgets');
  });

  it('fuzzy-matches on author, year and title together', async () => {
    const source = citeSource(() => bibliography);
    const text = '\\cite{gadg';
    const result = await source(contextAt(text, text.length));

    expect(result!.options.map((o) => o.apply)).toEqual(['doe2020']);
  });

  it('returns null outside a cite command, leaving other completion sources to answer', async () => {
    const source = citeSource(() => bibliography);
    const text = 'plain text';
    const result = await source(contextAt(text, text.length));

    expect(result).toBeNull();
  });

  it('completes only the key under the cursor in \\cite{a,b}, replacing "b" and leaving "a" alone', async () => {
    const source = citeSource(() => bibliography);
    const text = '\\cite{smith2019,do';
    const result = await source(contextAt(text, text.length));

    expect(result!.from).toBe(16);
    expect(result!.to).toBe(text.length);
    expect(result!.options.map((o) => o.apply)).toEqual(['doe2020']);
  });
});

describe('citeThenLsp', () => {
  const bibliography = [
    entry({ key: 'smith2019', author: 'Smith, Jane', year: '2019', title: 'On Widgets' }),
    entry({ key: 'doe2020', author: 'Doe, John', year: '2020', title: 'Gadgets Reconsidered' }),
  ];

  it('reads the entries array fresh on every call rather than capturing it once', async () => {
    // Goes through `citeThenLsp` rather than `citeSource`: `citeSource` returns null for an
    // empty match list (see the `citeSource` describe block above), which would make the first
    // call here indistinguishable from "not a cite position" at all. `citeThenLsp`'s result is
    // never null inside an open `\cite{`, so the empty list is observable directly.
    let current: BibEntrySummary[] = [];
    const source = citeThenLsp(() => current, async () => null);
    const text = '\\cite{';

    expect((await source(contextAt(text, text.length)))!.options).toEqual([]);

    current = bibliography;
    const second = await source(contextAt(text, text.length));
    expect(second!.options.map((o) => o.apply).sort()).toEqual(['doe2020', 'smith2019']);
  });

  it('answers from the bibliography inside a cite argument, never calling the fallback', async () => {
    const fallback = vi.fn(async () => ({ from: 0, to: 0, options: [{ label: 'itemize' }] }));
    const source = citeThenLsp(() => bibliography, fallback);
    const text = '\\cite{smith';

    const result = await source(contextAt(text, text.length));

    expect(result!.options.map((o) => o.apply)).toEqual(['smith2019']);
    expect(fallback).not.toHaveBeenCalled();
  });

  it('defers to the fallback everywhere else, so \\begin{ and plain LSP completion still work', async () => {
    const lspResult = { from: 7, to: 7, options: [{ label: 'itemize' }] };
    const fallback = vi.fn(async () => lspResult);
    const source = citeThenLsp(() => bibliography, fallback);
    const text = '\\begin{';

    const result = await source(contextAt(text, text.length));

    expect(result).toBe(lspResult);
    expect(fallback).toHaveBeenCalledOnce();
  });

  it('defers to the fallback when the cite list is legitimately empty (no matching entry)', async () => {
    // An empty options array is still a non-null CompletionResult from `citeSource` — it must
    // win over the fallback, not be mistaken for "not a cite position" and fall through.
    const fallback = vi.fn(async () => null);
    const source = citeThenLsp(() => [], fallback);
    const text = '\\cite{nomatch';

    const result = await source(contextAt(text, text.length));

    expect(result!.options).toEqual([]);
    expect(fallback).not.toHaveBeenCalled();
  });
});
