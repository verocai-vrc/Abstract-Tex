// `\cite{`-family completion, backed by the bibliography index (S7.2) rather than a language
// server round trip: the whole index is already sitting in memory (`bibliography.svelte.ts`), so
// answering "what keys fit here" is a synchronous, in-process lookup, not an IPC call — the same
// reason `fuzzy.ts`'s quick-open never asks Rust either. `Editor.svelte` wires `citeThenLsp`
// into `setup.ts`'s one `override` slot: inside a `\cite`-family argument it answers on its own
// (with author/year/title, DESIGN.md §5.4 — "not a bare key"), and everywhere else it defers to
// TexLab, so CodeMirror's autocomplete only ever shows one list at any position.
//
// This file knows nothing about `.bib` files or IPC: it takes `BibEntrySummary[]` and a
// CodeMirror document, the same "protocol lives elsewhere, this file only builds CodeMirror
// types" split `completion.ts` and `hover.ts` already draw.

import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { BibEntrySummary } from '../ipc';
import { fuzzyMatch } from '../fuzzy';

/** One BibTeX name, split into the parts a label wants. `von`/`jr` are folded into `last` and
 * `first` respectively rather than given their own fields — nothing here renders "de la Cruz, Jr,
 * Maria" differently from "de la Cruz, Maria", and a fuller `von`/`jr`/`last`/`first` split earns
 * its complexity only once a caller needs to re-render a name in a different order (S7.6's key
 * generation, when it exists, is exactly that caller and can grow its own splitter then). */
export interface Name {
  first: string;
  last: string;
}

/** Split one BibTeX name (`"von Last, Jr, First"`, `"First von Last"`, or `"{Corporate Name}"`)
 * into `{ first, last }`. BibTeX's own rule: a name with one or more commas is already
 * `last-name-first` (the parts before the first comma are the last name, including any `von`;
 * everything after is the first name and any `jr`, joined back with a space); a name with no
 * comma is `first-name-last`, and the last *word* is the last name unless it is wrapped in
 * `{braces}` (a corporate author, kept whole so "Consortium" is not mistaken for a surname). */
function splitName(raw: string): Name {
  const trimmed = raw.trim();
  if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
    return { first: '', last: trimmed.slice(1, -1).trim() };
  }
  if (trimmed.includes(',')) {
    const [last, ...rest] = trimmed.split(',').map((part) => part.trim());
    return { first: rest.filter(Boolean).join(' '), last: last ?? '' };
  }
  const words = trimmed.split(/\s+/).filter(Boolean);
  if (words.length <= 1) return { first: '', last: trimmed };
  return { first: words.slice(0, -1).join(' '), last: words[words.length - 1]! };
}

/** A raw BibTeX `author`/`editor` field (`"Smith, Jane and Doe, John and others"`) into one
 * `Name` per person. `and` is BibTeX's own separator, matched whole-word so it cannot fire
 * inside a name like `"Anderson"`. A trailing `"others"` (BibTeX's et-al marker) is dropped here;
 * `authorLabel` below is what turns its *absence* into "et al." */
export function splitNames(raw: string | null): Name[] {
  if (!raw) return [];
  return raw
    .split(/\s+and\s+/i)
    .map((part) => part.trim())
    .filter((part) => part.length > 0 && part.toLowerCase() !== 'others')
    .map(splitName);
}

/** "Smith" · "Smith & Doe" · "Smith et al." · "" for no author at all — what a completion label
 * shows before the year. Two names are spelled out in full (a reader can hold two surnames), and
 * a raw field ending in `"and others"` always earns "et al." even when only one named author
 * came before it, since that is the field's own claim that more exist. */
export function authorLabel(raw: string | null): string {
  if (!raw) return '';
  const names = splitNames(raw);
  if (names.length === 0) return '';
  const surnames = names.map((name) => name.last).filter((last) => last.length > 0);
  if (surnames.length === 0) return '';
  const etAl = /\band\s+others\b/i.test(raw);
  if (etAl) return `${surnames[0]} et al.`;
  if (surnames.length === 1) return surnames[0]!;
  if (surnames.length === 2) return `${surnames[0]} & ${surnames[1]}`;
  return `${surnames[0]} et al.`;
}

/** "Smith (2019)" · "Smith et al. (2019)" · "Smith" (no year) · "2019" (no author) · "" (neither)
 * — the completion list's label, before the em dash and title. */
export function citeLabel(entry: BibEntrySummary): string {
  const who = authorLabel(entry.author);
  if (who && entry.year) return `${who} (${entry.year})`;
  return who || entry.year || '';
}

/** Where in `text`, scanning backward from `pos`, the cursor sits inside an *open* `{…}` argument
 * of a `\…cite…`-family command — and the partial key already typed since the last `{` or `,`.
 * `null` for everywhere else: outside any such command, past its closing `}`, or inside its
 * `[…]` options.
 *
 * Deliberately a single-line scan, matching `bibliography.rs`'s `scan_citations`: an argument
 * "almost never spans a line break," and stopping at one is the reading that fails safe rather
 * than guessing across a blank line into unrelated text.
 */
export interface CiteContext {
  /** Offset where the current key starts (after the last `{` or `,`, plus any spaces). */
  from: number;
  /** The partial key typed so far, `from` to `pos`. */
  partial: string;
}

/** Whether `before` ends in a cite-family command name — the letters right before it contain
 * "cite" — that a single, *unescaped* backslash actually starts, allowing an optional `*` and any
 * number of `[…]` option groups (with spaces) between the name and wherever `before` ends. Scans
 * right to left, one bounded run at a time, rather than a single regex: a regex anchored only at
 * the end (`...$`) matches the *leftmost* position that still lets the rest succeed, which for a
 * run of several backslashes before "cite" is not necessarily the last one — exactly the case
 * that needs telling apart from a real command. `\\cite` (an escaped backslash — TeX's own
 * line-break command — followed by the plain word "cite") must be rejected; `\\\cite` (a line
 * break, then a real `\cite`) must not be. Mirrors `bibliography.rs`'s `find_commands`, which
 * never has this ambiguity because it only ever moves forward one character at a time. */
function endsInCiteCommand(before: string): boolean {
  let i = before.length;
  while (i > 0 && /\s/.test(before[i - 1]!)) i--; // trailing space before the brace
  while (i > 0 && before[i - 1] === ']') {
    // A `[...]` options group, right to left; `\cites[opt]` is not real LaTeX today but costing
    // nothing to allow it costs nothing either, and keeps this symmetric with `[...]*` order.
    const open = before.lastIndexOf('[', i - 2);
    if (open === -1) return false; // an unmatched `]` means this is not a command at all
    i = open;
  }
  if (i > 0 && before[i - 1] === '*') i--; // a starred variant, `\cite*`
  const nameEnd = i;
  while (i > 0 && /[A-Za-z]/.test(before[i - 1]!)) i--;
  if (!/cite/i.test(before.slice(i, nameEnd))) return false;

  // `i` now sits right after the run of backslashes leading into the command name. The command
  // is real only if that run's *length* is odd: each pair of backslashes is one escaped
  // backslash (TeX's line break), and it is the one backslash left over, if any, that starts an
  // actual command.
  let backslashes = 0;
  while (i > 0 && before[i - 1] === '\\') {
    backslashes++;
    i--;
  }
  return backslashes % 2 === 1;
}

export function citeContextAt(text: string, pos: number): CiteContext | null {
  const lineStart = text.lastIndexOf('\n', pos - 1) + 1;
  const line = text.slice(lineStart, pos);

  // Walk left from the cursor over an unterminated brace group: a `}` before any `{` means the
  // cursor sits after a closed argument (or outside one entirely), not inside one.
  let depth = 0;
  let openAt = -1;
  for (let i = line.length - 1; i >= 0; i--) {
    const ch = line[i];
    if (ch === '}') {
      depth++;
    } else if (ch === '{') {
      if (depth === 0) {
        openAt = i;
        break;
      }
      depth--;
    }
  }
  if (openAt === -1) return null;

  // Immediately before that `{` (skipping any `[…]` options and the optional `*`) must be a
  // cite-family command name, or this `{` belongs to something else entirely (`\textbf{`, a
  // plain group, `\cites{a}` before starting its second `{b}`).
  if (!endsInCiteCommand(line.slice(0, openAt))) return null;

  const inner = line.slice(openAt + 1);
  const lastComma = inner.lastIndexOf(',');
  const partial = lastComma === -1 ? inner : inner.slice(lastComma + 1);
  const from = lineStart + openAt + 1 + (lastComma === -1 ? 0 : lastComma + 1) + (partial.length - partial.trimStart().length);
  return { from, partial: partial.trim() };
}

/** One `BibEntrySummary` → the `Completion` CodeMirror renders: label carries the author/year (or
 * as much of it as exists), `detail` the title, `apply` the bare key — DESIGN.md §5.4's
 * "author, year and title, not a bare key" describes what is *shown*, not what is *inserted*; the
 * buffer still needs to hold the citation key BibTeX/Biber can resolve. */
function toCompletion(entry: BibEntrySummary): Completion {
  const label = citeLabel(entry);
  return {
    label: entry.key,
    displayLabel: label ? `${label} — ${entry.key}` : entry.key,
    detail: entry.title ?? undefined,
    apply: entry.key,
    type: 'text',
  };
}

/** `found`'s entries, fuzzy-ranked against its partial key on author, year and title together
 * (DESIGN.md §5.4) — the best of the three, so a query like `"smith19"` still finds an entry
 * whose title happens to start with "Smith" ahead of one merely mentioning 2019 in its title —
 * and built into the one `CompletionResult` both `citeSource` and `citeThenLsp` return. */
function resultAt(found: CiteContext, entries: readonly BibEntrySummary[], pos: number): CompletionResult {
  const options = entries
    .map((entry) => {
      const haystack = `${citeLabel(entry)} ${entry.title ?? ''} ${entry.key}`;
      const match = fuzzyMatch(found.partial, haystack);
      return match ? { entry, score: match.score } : null;
    })
    .filter((row): row is { entry: BibEntrySummary; score: number } => row !== null)
    .sort((a, b) => b.score - a.score)
    .map((row) => toCompletion(row.entry));

  return {
    from: found.from,
    to: pos,
    options,
    // A key never contains a space or brace; once either appears after `from`, the list is for a
    // different key entirely (`,` starts a new one, handled by re-running this source, since
    // CodeMirror re-triggers a source once `validFor` stops matching).
    validFor: /^[^,{}\s]*$/,
  };
}

/**
 * Build a CodeMirror `CompletionSource` for `\cite`-family keys, backed by whatever
 * `BibEntrySummary[]` the caller currently has (`bibliography.entries`'s values, refreshed
 * whenever the index changes — this function reads the array fresh on every call rather than
 * capturing it once, so a completion mid-type never answers from a stale bibliography).
 *
 * `null` for no matches too, not an empty-but-open popup — the same rule `lspCompletionSource`
 * follows and for the same reason (an open popup with nothing in it reads as a glitch, not as
 * "nothing matched yet"). Prefer `citeThenLsp` over calling this directly: on its own, a `null`
 * here is ambiguous between "not a cite position" and "a cite position with no matches," and only
 * `citeThenLsp` knows which one means "let the language server answer instead."
 */
export function citeSource(entries: () => readonly BibEntrySummary[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const found = citeContextAt(context.state.sliceDoc(0, context.pos), context.pos);
    if (!found) return null;
    const result = resultAt(found, entries(), context.pos);
    return result.options.length === 0 ? null : result;
  };
}

/**
 * Compose the cite source ahead of another (TexLab's), so there is exactly one popup at a
 * `\cite{` position rather than two competing ones — `setup.ts`'s `autocompletion({ override })`
 * takes one list of sources and merges whatever each returns, and without this, both would fire
 * inside a cite argument since TexLab has no idea the command is special. Suppression is decided
 * by position (`citeContextAt`), not by whether the cite list has any matches: a `\cite{` with no
 * matching entry yet must still show "no matches" rather than silently falling through to
 * whatever TexLab thinks a bare word inside `{}` should complete to. `fallback` is what a
 * `\ref{`, a `\begin{`, or plain prose still gets.
 */
export function citeThenLsp(
  entries: () => readonly BibEntrySummary[],
  fallback: (context: CompletionContext) => Promise<CompletionResult | null>,
): (context: CompletionContext) => Promise<CompletionResult | null> {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const found = citeContextAt(context.state.sliceDoc(0, context.pos), context.pos);
    if (!found) return fallback(context);
    return resultAt(found, entries(), context.pos);
  };
}
