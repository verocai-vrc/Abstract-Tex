// Turning a real Git conflict into "two paragraphs and a choice" (S11.2b, DESIGN.md §5.7).
//
// The file this reads already has real `<<<<<<<`/`=======`/`>>>>>>>` markers in it — S11.2a's own
// `sync` wrote them, exactly as a terminal `git merge` would have. Nothing here ever shows that
// text back to anyone; it exists purely to turn it into structured data `ConflictResolver.svelte`
// can render as prose. Pure and Svelte-free, like `outline.ts` and `paths.ts`, so `conflict.test.ts`
// exercises it with string literals.

/** A run of lines neither side disagreed about. */
export interface CleanSection {
  kind: 'clean';
  text: string;
}

/** One conflicting hunk: what the local branch had, and what the remote had, for the same lines. */
export interface ConflictHunk {
  kind: 'conflict';
  ours: string;
  theirs: string;
}

export type ConflictSection = CleanSection | ConflictHunk;

// The four markers Git writes, matched the way Git writes them (review finding R6.10): seven of the
// character, then either the end of the line or a space and a label. `=======` has no label. A line
// of nine equals signs, or a heading underline, is text. A trailing `\r` (a file with Windows line
// endings) is ignored for the match only; the lines themselves keep it.
const OPENS = /^<{7}(?: |$)/;
const BASE = /^\|{7}(?: |$)/;
const SEPARATES = /^={7}$/;
const CLOSES = /^>{7}(?: |$)/;

const isMarker = (marker: RegExp, line: string): boolean => marker.test(line.replace(/\r$/, ''));

/**
 * Split a file already containing real conflict markers into clean runs and hunks, in the order
 * they appear.
 *
 * A hunk written in `diff3` or `zdiff3` style (`merge.conflictStyle`, which the app meets whenever a
 * conflict was made by a `git` in a terminal) has a third part between the two sides, the common
 * ancestor, introduced by a `|||||||` line. It is not either side's text and is left out of both.
 *
 * `null` rather than a guess when the markers do not nest the way a plain modify/modify text
 * conflict does — an add/add or delete/modify conflict can leave a file with no markers in it at
 * all, or a marker with no matching close. The caller shows a sentence instead of something wrong.
 */
export function parseConflictMarkers(text: string): ConflictSection[] | null {
  const lines = text.split('\n');
  const sections: ConflictSection[] = [];
  let clean: string[] = [];
  let i = 0;

  const flushClean = () => {
    if (clean.length > 0) {
      sections.push({ kind: 'clean', text: clean.join('\n') });
      clean = [];
    }
  };

  while (i < lines.length) {
    const line = lines[i]!;
    if (!isMarker(OPENS, line)) {
      clean.push(line);
      i++;
      continue;
    }
    flushClean();
    i++; // past the `<<<<<<< ours-label` line itself

    const ours: string[] = [];
    while (i < lines.length && !isMarker(SEPARATES, lines[i]!) && !isMarker(BASE, lines[i]!)) {
      ours.push(lines[i]!);
      i++;
    }
    if (i >= lines.length) return null; // no `=======`: not a conflict this can read
    if (isMarker(BASE, lines[i]!)) {
      i++; // past `||||||| base-label`, then the ancestor's lines, which belong to neither side
      while (i < lines.length && !isMarker(SEPARATES, lines[i]!)) i++;
      if (i >= lines.length) return null;
    }
    i++; // past `=======`

    const theirs: string[] = [];
    while (i < lines.length && !isMarker(CLOSES, lines[i]!)) {
      theirs.push(lines[i]!);
      i++;
    }
    if (i >= lines.length) return null; // no closing `>>>>>>>` either
    i++; // past `>>>>>>> theirs-label`

    sections.push({ kind: 'conflict', ours: ours.join('\n'), theirs: theirs.join('\n') });
  }
  flushClean();
  return sections;
}

/** How many hunks a parse found — what decides whether `ConflictResolver` has anything to ask. */
export function hunkCount(sections: ConflictSection[]): number {
  return sections.filter((section) => section.kind === 'conflict').length;
}

/**
 * Rebuild the file from `sections`, replacing each hunk in order with the matching entry of
 * `resolutions`.
 *
 * Lines, not text, are what gets concatenated: a resolution of `''` contributes no line at all
 * rather than a blank one, so choosing to delete a paragraph outright leaves no trace of it —
 * `reassembleConflictSections.test.ts`'s own name for this is "nothing is not a blank line".
 */
export function reassembleConflictSections(sections: ConflictSection[], resolutions: readonly string[]): string {
  const lines: string[] = [];
  let hunkIndex = 0;
  for (const section of sections) {
    if (section.kind === 'clean') {
      lines.push(...section.text.split('\n'));
      continue;
    }
    const resolved = resolutions[hunkIndex] ?? '';
    hunkIndex++;
    if (resolved.length > 0) lines.push(...resolved.split('\n'));
  }
  return lines.join('\n');
}
