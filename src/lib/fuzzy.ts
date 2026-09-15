// Fuzzy matching for the quick-open list (S2.4), and later for the command palette (S4.3).
//
// The kind of matching an author expects from `Ctrl P`: `secres` finds `sections/results.tex`,
// `main` puts `main.tex` above `domain-notes.tex`, and nothing needs to be spelled in full.
// This is the small, readable version — a greedy left-to-right subsequence match with a few
// bonuses — not an optimal-alignment algorithm. It is scored well enough to order a few hundred
// file names sensibly, which is the whole job.

export interface Match {
  score: number;
  /** Indices into the candidate that matched, for highlighting. */
  positions: number[];
}

/**
 * Match `query` against `candidate` as a subsequence, case-insensitively. Returns `null` when
 * some character of the query appears nowhere in order.
 */
export function fuzzyMatch(query: string, candidate: string): Match | null {
  if (query.length === 0) return { score: 0, positions: [] };
  const q = query.toLowerCase();
  const c = candidate.toLowerCase();

  // The basename is what an author usually types. Try to land the whole query inside it first;
  // fall back to the full path only when that fails, so `res` prefers `results.tex` over
  // `resources/figure.tex` even though both contain the letters in order.
  const basenameStart = c.lastIndexOf('/') + 1;
  return scan(q, c, basenameStart, 6) ?? scan(q, c, 0, 0);
}

/** Greedy subsequence scan starting at `from`, with `bonus` added if the scan succeeds. */
function scan(q: string, c: string, from: number, bonus: number): Match | null {
  const positions: number[] = [];
  let score = bonus;
  let at = from;
  for (const ch of q) {
    const found = c.indexOf(ch, at);
    if (found === -1) return null;
    score += 1;
    // A letter that starts a word (`sections/`**r**`esults`) or continues a run the author is
    // clearly spelling out (`re`**s**`ults`) is worth more than one buried mid-word.
    if (found === 0 || isBoundary(c[found - 1]!)) score += 3;
    else if (positions.length > 0 && positions[positions.length - 1] === found - 1) score += 2;
    positions.push(found);
    at = found + 1;
  }
  // Shorter candidates win ties: `main.tex` over `main-old.tex` for `main`.
  score -= c.length / 100;
  return { score, positions };
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
  for (let i = 0; i < text.length; i++) {
    const hit = hits.has(i);
    const last = segments[segments.length - 1];
    if (last && last.hit === hit) last.text += text[i];
    else segments.push({ text: text[i]!, hit });
  }
  return segments;
}
