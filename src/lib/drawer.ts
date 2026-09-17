// The diagnostics drawer's own logic (S6.3), kept out of `Drawer.svelte` so it can be tested with
// plain arrays: which tab a diagnostic belongs to, how the cards group by file and order within a
// group, what the severity and "this file" filters hide, and where in the raw log a diagnostic's
// own words are. The component renders what these functions return and nothing more — the same
// split `fix.ts`/`document.ts` and `outline.ts`/`DocumentMap.svelte` already use.
//
// Every diagnostic here is one of texlog's *explained* sentences (DESIGN.md §5.2). A raw TeX line
// never becomes a card; the raw log is reachable from each card, one click away, and that is the
// only place TeX's own words appear (DESIGN.md §2, commitment 3).

import type { Diagnostic } from './ipc';

/** Which severities the drawer is showing. `'all'` is the default; the choice lives for the
 * session (`app.drawerFilter`), not per build — an author who asked for "errors only" while
 * chasing one is still chasing it after the next build. */
export type SeverityFilter = 'all' | 'errors' | 'warnings';

export interface DrawerFilter {
  severity: SeverityFilter;
  /** Show only the diagnostics that belong to the active tab (by `diagnosticTarget`, so a
   * diagnostic with no file counts as the root file's, the same as jumping to it does). */
  activeFileOnly: boolean;
}

export const DEFAULT_FILTER: DrawerFilter = { severity: 'all', activeFileOnly: false };

/**
 * Which tab a diagnostic belongs to: its own resolved file when the log parser named one (S5.6's
 * `Diagnostic.file`), falling back to the root file for the diagnostics that have none — a log
 * with nothing open on the resolver's stack at all (`emergency-stop`'s own fixture), or no
 * project open yet. Shared by the drawer's click, the one-click fix (S6.2) and the gutter, so all
 * three agree on where a diagnostic lives.
 *
 * `Diagnostic.file` is spelled the way TeX printed it in the log, project-relative with no `./`
 * prefix for this project's bundled Tectonic — but only *with* an extension when the author wrote
 * one. `\include{sections/background}` opens `sections/background.tex` and the log says so
 * (S6.3's thesis capture); `\input{sections/background}`, the more common spelling, is echoed as
 * `sections/background`, and `texlog` cannot complete it because it never reads the file tree
 * (`resolver.rs`'s own module doc hands exactly this case to "a caller with access to the real
 * file tree"). That caller is here: `documentFiles` is S4.1's include graph, and a bare spelling
 * that is not in it but whose `.tex` sibling is becomes that sibling. Never a guess — a name is
 * only completed to one the graph actually holds. Found by the torture walk's first step (S6.4).
 * A package file (`babel.sty`) arrives as spelled, matches nothing, and stays as spelled; it is
 * not a tab and opening it fails with a notice, which is honest, if unhelpful.
 */
export function diagnosticTarget(
  diagnostic: Diagnostic,
  rootFile: string | null,
  documentFiles: readonly string[] = [],
): string | null {
  if (diagnostic.file === null) return rootFile;
  if (documentFiles.includes(diagnostic.file)) return diagnostic.file;
  const withExtension = `${diagnostic.file}.tex`;
  return documentFiles.includes(withExtension) ? withExtension : diagnostic.file;
}

/** The diagnostics whose target is `path`: what the gutter draws on that tab (S2.7's dots,
 * routed to their own file at last — until S6.3 every dot landed on the root file's tab). */
export function diagnosticsForFile(
  diagnostics: readonly Diagnostic[],
  path: string | null,
  rootFile: string | null,
  documentFiles: readonly string[] = [],
): Diagnostic[] {
  if (path === null) return [];
  return diagnostics.filter((diagnostic) => diagnosticTarget(diagnostic, rootFile, documentFiles) === path);
}

/** One file's worth of cards. */
export interface DiagnosticGroup {
  /** The file the diagnostics are in, spelled as `diagnosticTarget` resolves it (so an
   * extensionless `\input` reads `sections/foo.tex`, the same as its tab), or `null` for
   * diagnostics TeX raised with no file open. Not quite the target: a `null` here is shown as
   * "not inside any file" rather than silently filed under the root, because that is a guess
   * the header should not present as a fact. */
  file: string | null;
  /** Errors first, then warnings; within a severity by line, a diagnostic with no line last. */
  diagnostics: Diagnostic[];
  errorCount: number;
  warningCount: number;
}

/**
 * Group the last build's diagnostics into per-file sections for the drawer, after applying
 * `filter`.
 *
 * Groups appear in the order their file first appears in `diagnostics`, which is the order TeX
 * reported them. `texlog::diagnostics` lists errors before warnings, so the file that stopped the
 * build leads without any rule here saying so — a thesis whose chapter 3 has the error and whose
 * chapter 1 has undefined references shows chapter 3 first.
 *
 * Within a group the order is by severity then line, not the log's order: a file's own warnings
 * read best top to bottom, and the log already reports them roughly that way. The sort is stable,
 * so two diagnostics on one line keep their reported order.
 *
 * The filter decides which cards a group shows, never where the group sits — see the note on
 * `byFile` below.
 */
export function groupDiagnostics(
  diagnostics: readonly Diagnostic[],
  filter: DrawerFilter,
  activePath: string | null,
  rootFile: string | null,
  documentFiles: readonly string[] = [],
): DiagnosticGroup[] {
  // A `Map` keeps insertion order, which is exactly the "first appearance" order wanted. `null`
  // is a legal key, so the no-file diagnostics need no sentinel string that could collide with a
  // real filename.
  //
  // Every diagnostic registers its file here, filtered or not, and the filter is applied when the
  // group is filled. Order is therefore fixed by the build, not by the filter: switching from
  // "all" to "warnings" hides the error's card but does not move its file below one that only
  // ever had warnings. (The first version of this function filtered first, and its own test
  // caught the sections trading places.) A group left empty by the filter is dropped at the end.
  const byFile = new Map<string | null, Diagnostic[]>();
  for (const diagnostic of diagnostics) {
    const heading = diagnostic.file === null ? null : diagnosticTarget(diagnostic, rootFile, documentFiles);
    const list = byFile.get(heading) ?? [];
    if (passesFilter(diagnostic, filter, activePath, rootFile, documentFiles)) list.push(diagnostic);
    byFile.set(heading, list);
  }

  return [...byFile]
    .filter(([, list]) => list.length > 0)
    .map(([file, list]) => {
      const ordered = [...list].sort(compareWithinFile);
      return {
        file,
        diagnostics: ordered,
        errorCount: ordered.filter((d) => d.severity === 'error').length,
        warningCount: ordered.filter((d) => d.severity === 'warning').length,
      };
    });
}

function passesFilter(
  diagnostic: Diagnostic,
  filter: DrawerFilter,
  activePath: string | null,
  rootFile: string | null,
  documentFiles: readonly string[],
): boolean {
  if (filter.severity === 'errors' && diagnostic.severity !== 'error') return false;
  if (filter.severity === 'warnings' && diagnostic.severity !== 'warning') return false;
  if (filter.activeFileOnly && diagnosticTarget(diagnostic, rootFile, documentFiles) !== activePath) return false;
  return true;
}

function compareWithinFile(a: Diagnostic, b: Diagnostic): number {
  if (a.severity !== b.severity) return a.severity === 'error' ? -1 : 1;
  // `Infinity` sorts a missing line after every real one without a special case per branch.
  return (a.line ?? Infinity) - (b.line ?? Infinity);
}

/** A character range in the raw log. */
export interface LogRange {
  from: number;
  to: number;
}

/**
 * Where TeX's own words for a diagnostic sit in the raw log, so the raw view can open scrolled to
 * them rather than at the top of a two-thousand-line transcript.
 *
 * `rawMessage` is the *unwrapped* message — `tokenizer.rs` (S5.1) undoes TeX's 79-column hard
 * wrap before the rules see it — while the log on disk still has the wrap in it. So an exact
 * search can fail for any message long enough to have wrapped; when it does, the search retries
 * with the message cut back a word at a time until some prefix is found, and highlights that.
 * The prefix must keep at least `minimumPrefix` characters: a five-character prefix of an
 * unrelated message would "match" almost anywhere, and a wrong highlight is worse than none.
 * Returns `null` when nothing long enough matches; the view then shows the log from the top.
 */
export function locateInLog(log: string, rawMessage: string, minimumPrefix = 20): LogRange | null {
  let needle = rawMessage.trim();
  while (needle.length > 0) {
    const from = log.indexOf(needle);
    if (from !== -1) return { from, to: from + needle.length };
    const lastSpace = needle.lastIndexOf(' ');
    if (lastSpace === -1) return null;
    needle = needle.slice(0, lastSpace).trimEnd();
    // Below the minimum the match no longer means anything; stop rather than settle for it. The
    // check is after the cut so a message that is itself shorter than the minimum still gets its
    // one exact-match attempt above.
    if (needle.length < minimumPrefix) return null;
  }
  return null;
}
