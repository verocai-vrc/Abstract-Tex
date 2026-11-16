// Fuzzy matching for the quick-open list (S2.4), and later for the command palette (S4.3).
//
// The kind of matching an author expects from `Ctrl P`: `secres` finds `sections/results.tex`,
// `main` puts `main.tex` above `domain-notes.tex`, and nothing needs to be spelled in full.
// This is the small, readable version — a greedy left-to-right subsequence match with a few
// bonuses — not an optimal-alignment algorithm. It is scored well enough to order a few hundred
// file names sensibly, which is the whole job.

export interface Match {
  score: number;
  /** Where each matched letter starts in the candidate, in UTF-16 units of the candidate *as it is
   * written*, for highlighting. */
  positions: number[];
}

/**
 * One letter as a person sees it: a character and the accents that follow it. Matching letters
 * rather than UTF-16 units is what keeps three things true at once (R7.6): a character made of two
 * units (an emoji, a rare CJK character) is never cut in half; a name a file system stored with
 * decomposed accents (`e` and a combining acute, as macOS does) matches the `é` a keyboard types; and
 * the positions reported are into the candidate as written, not into a lowercased copy whose length
 * differs (`İ` lowercases to two units, which drifted every later highlight by one).
 */
interface Letter {
  /** Where this letter starts in the text it came from. */
  start: number;
  text: string;
  /** What it is compared as: no accents, lower case, so `resume` finds `Résumé`. */
  folded: string;
}

const LETTER = /\P{M}\p{M}*|\p{M}+/gu;

function lettersOf(text: string): Letter[] {
  return [...text.matchAll(LETTER)].map((found) => ({
    start: found.index,
    text: found[0],
    folded: foldLetter(found[0]),
  }));
}

/** A letter as it is compared: no accents, lower case. Most letters even of an accented name are plain
 * ASCII, and for those the Unicode work is skipped. */
function foldLetter(letter: string): string {
  if (letter.length === 1 && letter.charCodeAt(0) < 128) return letter.toLowerCase();
  return letter.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** The query's letters in the form they are compared in. Remembered for the last query, because
 * `rank` asks once per candidate and the query does not change between them. */
let rememberedQuery = '';
let rememberedWanted: string[] = [];
function wantedLetters(query: string): string[] {
  if (query !== rememberedQuery) {
    rememberedQuery = query;
    // A bare combining mark folds to nothing, and has nothing to match against.
    rememberedWanted = lettersOf(query)
      .map((letter) => letter.folded)
      .filter((folded) => folded !== '');
  }
  return rememberedWanted;
}

/**
 * What the scan needs to know about a candidate, answered either by a plain lowercased string
 * (nearly every file name is ASCII, and this is the allocation-free path the matcher always had)
 * or by a list of [`Letter`]s (any name with an accent, an emoji or a non-Latin script). Both give
 * the same answers for the same name, which a test pins.
 */
interface Candidate {
  /** The index of the first letter at or after `from` that is `folded`, or -1. */
  find(folded: string, from: number): number;
  boundaryBefore(index: number): boolean;
  /** Where the letter at `index` starts in the name as written. */
  start(index: number): number;
  /** The index of the first letter after the last `/`. */
  basenameStart(): number;
}

const ASCII_ONLY = /^[\x00-\x7f]*$/;

function asciiCandidate(text: string): Candidate {
  const lower = text.toLowerCase(); // same length as `text`, because it is all ASCII
  return {
    find: (folded, from) => lower.indexOf(folded, from),
    boundaryBefore: (index) => isBoundary(lower[index - 1]!),
    start: (index) => index,
    basenameStart: () => lower.lastIndexOf('/') + 1,
  };
}

/** The letters of names that needed them, kept because the same names are ranked again on every
 * keystroke. Bounded: a project with more accented names than this simply starts the list again. */
const LETTER_CACHE_LIMIT = 4000;
const rememberedLetters = new Map<string, Letter[]>();

function lettersOfName(text: string): Letter[] {
  const known = rememberedLetters.get(text);
  if (known) return known;
  if (rememberedLetters.size >= LETTER_CACHE_LIMIT) rememberedLetters.clear();
  const letters = lettersOf(text);
  rememberedLetters.set(text, letters);
  return letters;
}

function letterCandidate(text: string): Candidate {
  const letters = lettersOfName(text);
  return {
    find: (folded, from) => {
      for (let index = from; index < letters.length; index++) {
        if (letters[index]!.folded === folded) return index;
      }
      return -1;
    },
    boundaryBefore: (index) => isBoundary(letters[index - 1]!.text),
    start: (index) => letters[index]!.start,
    basenameStart: () => {
      for (let index = letters.length - 1; index >= 0; index--) {
        if (letters[index]!.text === '/') return index + 1;
      }
      return 0;
    },
  };
}

/**
 * Match `query` against `candidate` as a subsequence, ignoring case and accents. Returns `null` when
 * some letter of the query appears nowhere in order.
 */
export function fuzzyMatch(query: string, candidate: string): Match | null {
  const wanted = wantedLetters(query);
  if (wanted.length === 0) return { score: 0, positions: [] };
  const target = ASCII_ONLY.test(candidate) ? asciiCandidate(candidate) : letterCandidate(candidate);

  // The basename is what an author usually types. Try to land the whole query inside it first;
  // fall back to the full path only when that fails, so `res` prefers `results.tex` over
  // `resources/figure.tex` even though both contain the letters in order.
  return scan(wanted, target, candidate.length, target.basenameStart(), 6) ?? scan(wanted, target, candidate.length, 0, 0);
}

/** Greedy subsequence scan starting at letter `from`, with `bonus` added if the scan succeeds. */
function scan(wanted: string[], target: Candidate, candidateLength: number, from: number, bonus: number): Match | null {
  const matched: number[] = [];
  let score = bonus;
  let at = from;
  for (const folded of wanted) {
    const found = target.find(folded, at);
    if (found === -1) return null;
    score += 1;
    // A letter that starts a word (`sections/`**r**`esults`) or continues a run the author is
    // clearly spelling out (`re`**s**`ults`) is worth more than one buried mid-word.
    if (found === 0 || target.boundaryBefore(found)) score += 3;
    else if (matched.length > 0 && matched[matched.length - 1] === found - 1) score += 2;
    matched.push(found);
    at = found + 1;
  }
  // Shorter candidates win ties: `main.tex` over `main-old.tex` for `main`.
  score -= candidateLength / 100;
  return { score, positions: matched.map((index) => target.start(index)) };
}

function isBoundary(previous: string): boolean {
  return previous === '/' || previous === '.' || previous === '_' || previous === '-' || previous === ' ';
}

export interface Ranked<T> {
  item: T;
  match: Match;
}

/** Filter and order `items` by how well `query` matches `keyOf(item)`. Empty query keeps order. */
export function rank<T>(query: string, items: readonly T[], keyOf: (item: T) => string): Ranked<T>[] {
  const ranked: Ranked<T>[] = [];
  for (const item of items) {
    const match = fuzzyMatch(query, keyOf(item));
    if (match) ranked.push({ item, match });
  }
  if (query.length > 0) ranked.sort((a, b) => b.match.score - a.match.score);
  return ranked;
}

export interface HighlightSegment {
  text: string;
  hit: boolean;
}

/**
 * Split `text` into runs of matched/unmatched characters, for a template to wrap the matched
 * runs in `<mark>`. Shared by every list that renders a `Match`'s `positions` (quick-open's file
 * list, S2.4; the command palette, S4.3) so there is one highlighting rule to get right rather
 * than a copy per component.
 */
export function highlightMatch(text: string, positions: readonly number[]): HighlightSegment[] {
  const hits = new Set(positions);
  const segments: HighlightSegment[] = [];
  // By letter, so a character of two units, or a letter with its accent, is hit or not as a whole.
  for (const letter of lettersOf(text)) {
    const hit = hits.has(letter.start);
    const last = segments[segments.length - 1];
    if (last && last.hit === hit) last.text += letter.text;
    else segments.push({ text: letter.text, hit });
  }
  return segments;
}
