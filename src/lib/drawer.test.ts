// The drawer's grouping and filtering (S6.3), tested against a real capture rather than invented
// rows: `fixtures/thesis` built by this project's bundled Tectonic 0.17.0 with a stray `_` and an
// undefined macro appended to `sections/background.tex`, then run through `texlog::diagnostics`.
// One error stopped the build in chapter 2; six undefined-reference warnings landed across
// chapters 1 and 2 before it did (the build halted before the second pass that would resolve
// them). Explanations are trimmed to one clause — the grouping never reads them.

import { describe, expect, it } from 'vitest';
import type { Diagnostic } from './ipc';
import { DEFAULT_FILTER, diagnosticTarget, diagnosticsForFile, groupDiagnostics, locateInLog } from './drawer';

function undefinedReference(file: string, line: number, label: string, page: number): Diagnostic {
  return {
    title: `\`${label}\` is referenced but never defined`,
    explanation: `Nothing in this document calls \\label{${label}}.`,
    line,
    file,
    severity: 'warning',
    rule: 'undefined-reference',
    rawMessage: `Reference \`${label}' on page ${page} undefined on input line ${line}.`,
    fix: null,
  };
}

const missingDollar: Diagnostic = {
  title: '_ used outside maths',
  explanation: '`_` means "subscript" and only works inside maths mode.',
  line: 59,
  file: 'sections/background.tex',
  severity: 'error',
  rule: 'missing-dollar',
  rawMessage: 'Missing $ inserted.',
  fix: { description: 'Escape as \\_', find: '_', replace: '\\_' },
};

/** In the order `texlog::diagnostics` returned them: the error, then the warnings in log order. */
const thesisBuild: Diagnostic[] = [
  missingDollar,
  undefinedReference('sections/introduction.tex', 26, 'sec:token-bucket', 1),
  undefinedReference('sections/introduction.tex', 30, 'ch:results', 1),
  undefinedReference('sections/background.tex', 7, 'ch:introduction', 3),
  undefinedReference('sections/background.tex', 28, 'sec:distributed-bucket', 3),
  undefinedReference('sections/background.tex', 54, 'ch:results', 4),
  undefinedReference('sections/background.tex', 56, 'lem:token-bucket-exact', 4),
];

/** `emergency-stop`'s shape: no `l.NN`, and nothing open on the resolver's stack when it fires. */
const emergencyStop: Diagnostic = {
  title: 'The document never ends',
  explanation: 'TeX reached the end of the input without seeing \\end{document}.',
  line: null,
  file: null,
  severity: 'error',
  rule: 'emergency-stop',
  rawMessage: 'Emergency stop',
  fix: null,
};

describe('diagnosticTarget', () => {
  it('is the diagnostic\'s own file when the log named one', () => {
    expect(diagnosticTarget(missingDollar, 'main.tex')).toBe('sections/background.tex');
  });

  it('falls back to the root file when the log named none', () => {
    expect(diagnosticTarget(emergencyStop, 'main.tex')).toBe('main.tex');
  });

  it('is null when there is neither', () => {
    expect(diagnosticTarget(emergencyStop, null)).toBeNull();
  });
});

describe('diagnosticsForFile (gutter routing)', () => {
  it('gives a chapter tab its own diagnostics and nobody else\'s', () => {
    const rows = diagnosticsForFile(thesisBuild, 'sections/introduction.tex', 'main.tex');
    expect(rows.map((d) => d.line)).toEqual([26, 30]);
  });

  it('gives the root tab the diagnostics that named no file', () => {
    const rows = diagnosticsForFile([...thesisBuild, emergencyStop], 'main.tex', 'main.tex');
    expect(rows).toEqual([emergencyStop]);
  });

  it('gives a tab nothing when no diagnostic is about it, and nothing with no tab active', () => {
    expect(diagnosticsForFile(thesisBuild, 'preamble.tex', 'main.tex')).toEqual([]);
    expect(diagnosticsForFile(thesisBuild, null, 'main.tex')).toEqual([]);
  });
});

describe('groupDiagnostics', () => {
  it('groups by file in the order TeX first reported each — the file that stopped the build leads', () => {
    const groups = groupDiagnostics(thesisBuild, DEFAULT_FILTER, null, 'main.tex');
    expect(groups.map((g) => g.file)).toEqual(['sections/background.tex', 'sections/introduction.tex']);
  });

  it('puts the error first within its file, then the warnings by line', () => {
    const [background] = groupDiagnostics(thesisBuild, DEFAULT_FILTER, null, 'main.tex');
    expect(background!.diagnostics.map((d) => [d.severity, d.line])).toEqual([
      ['error', 59],
      ['warning', 7],
      ['warning', 28],
      ['warning', 54],
      ['warning', 56],
    ]);
    expect(background!.errorCount).toBe(1);
    expect(background!.warningCount).toBe(4);
  });

  it('sorts a diagnostic with no line after the ones that have one', () => {
    const noLine = { ...missingDollar, line: null };
    const [group] = groupDiagnostics([noLine, missingDollar], DEFAULT_FILTER, null, 'main.tex');
    expect(group!.diagnostics.map((d) => d.line)).toEqual([59, null]);
  });

  it('keeps a diagnostic with no file in its own group rather than filing it under the root', () => {
    const groups = groupDiagnostics([emergencyStop, ...thesisBuild], DEFAULT_FILTER, null, 'main.tex');
    expect(groups[0]!.file).toBeNull();
    expect(groups[0]!.diagnostics).toEqual([emergencyStop]);
    expect(groups.some((g) => g.file === 'main.tex')).toBe(false);
  });

  it('"errors" hides every warning, leaving only the file with the error', () => {
    const groups = groupDiagnostics(thesisBuild, { severity: 'errors', activeFileOnly: false }, null, 'main.tex');
    expect(groups).toHaveLength(1);
    expect(groups[0]!.diagnostics).toEqual([missingDollar]);
  });

  it('"warnings" hides the error and keeps both files — in the same order as before, not reshuffled', () => {
    // Background led the unfiltered list only because of its error. Hiding that card must not
    // move the file: a filter hides cards, it does not rearrange sections under the reader. The
    // first version of `groupDiagnostics` filtered before it ordered, and this test caught it.
    const groups = groupDiagnostics(thesisBuild, { severity: 'warnings', activeFileOnly: false }, null, 'main.tex');
    expect(groups.map((g) => [g.file, g.diagnostics.length])).toEqual([
      ['sections/background.tex', 4],
      ['sections/introduction.tex', 2],
    ]);
    expect(groups.every((g) => g.errorCount === 0)).toBe(true);
  });

  it('"this file only" keeps the active tab\'s group alone', () => {
    const filter = { severity: 'all' as const, activeFileOnly: true };
    const groups = groupDiagnostics(thesisBuild, filter, 'sections/introduction.tex', 'main.tex');
    expect(groups.map((g) => g.file)).toEqual(['sections/introduction.tex']);
  });

  it('"this file only" on the root tab shows the diagnostics that named no file, since that is where a click takes them', () => {
    const filter = { severity: 'all' as const, activeFileOnly: true };
    const groups = groupDiagnostics([emergencyStop, ...thesisBuild], filter, 'main.tex', 'main.tex');
    expect(groups.map((g) => g.file)).toEqual([null]);
  });

  it('both filters compose', () => {
    const filter = { severity: 'warnings' as const, activeFileOnly: true };
    const groups = groupDiagnostics(thesisBuild, filter, 'sections/background.tex', 'main.tex');
    expect(groups).toHaveLength(1);
    expect(groups[0]!.diagnostics.map((d) => d.line)).toEqual([7, 28, 54, 56]);
  });

  it('is empty, not a group with no cards, when the filter hides everything', () => {
    const filter = { severity: 'errors' as const, activeFileOnly: true };
    expect(groupDiagnostics(thesisBuild, filter, 'sections/introduction.tex', 'main.tex')).toEqual([]);
  });

  it('does not mutate its input', () => {
    const copy = [...thesisBuild];
    groupDiagnostics(thesisBuild, DEFAULT_FILTER, null, 'main.tex');
    expect(thesisBuild).toEqual(copy);
  });
});

describe('locateInLog', () => {
  // Verbatim from the capture's `main.log`: TeX's 79-column wrap splits one warning mid-number
  // (`line 2` / `6.`) and two others between `input` and `line`.
  const log = [
    ' (sections/introduction.tex',
    'Chapter 1.',
    '',
    'LaTeX Warning: Reference `sec:token-bucket\' on page 1 undefined on input line 2',
    '6.',
    '',
    '',
    'LaTeX Warning: Reference `ch:results\' on page 1 undefined on input line 30.',
    '',
    ') [1',
    ' (sections/background.tex',
    'LaTeX Warning: Reference `sec:distributed-bucket\' on page 3 undefined on input ',
    'line 28.',
    '',
    'LaTeX Warning: Reference `ch:results\' on page 4 undefined on input line 54.',
    '',
    '! Missing $ inserted.',
    '<inserted text> ',
    '                $',
    'l.59 A stray underscore_',
    '                        here and \\undefinedmacro too.',
  ].join('\n');

  function highlighted(range: { from: number; to: number } | null): string | null {
    return range ? log.slice(range.from, range.to) : null;
  }

  it('finds a message that did not wrap, exactly', () => {
    expect(highlighted(locateInLog(log, 'Missing $ inserted.'))).toBe('Missing $ inserted.');
  });

  it('finds the longest unwrapped prefix of a message TeX wrapped mid-word', () => {
    const range = locateInLog(log, "Reference `sec:token-bucket' on page 1 undefined on input line 26.");
    expect(highlighted(range)).toBe("Reference `sec:token-bucket' on page 1 undefined on input line");
  });

  it('finds the prefix of a message TeX wrapped between words', () => {
    const range = locateInLog(log, "Reference `sec:distributed-bucket' on page 3 undefined on input line 28.");
    expect(highlighted(range)).toBe("Reference `sec:distributed-bucket' on page 3 undefined on input");
  });

  it('tells two warnings about the same label apart by the rest of the message', () => {
    const first = locateInLog(log, "Reference `ch:results' on page 1 undefined on input line 30.");
    const second = locateInLog(log, "Reference `ch:results' on page 4 undefined on input line 54.");
    expect(first).not.toBeNull();
    expect(second).not.toBeNull();
    expect(second!.from).toBeGreaterThan(first!.to);
  });

  it('gives up rather than highlight a prefix too short to mean anything', () => {
    // "Reference `nosuch' on page 9 undefined on input line 99." shares only "Reference `" with
    // the log, which is far under the minimum — a wrong highlight would be worse than none.
    expect(locateInLog(log, "Reference `nosuch' on page 9 undefined on input line 99.")).toBeNull();
  });

  it('still tries an exact match for a message shorter than the minimum prefix', () => {
    expect(highlighted(locateInLog(log, 'Chapter 1.'))).toBe('Chapter 1.');
    expect(locateInLog(log, 'Chapter 7.')).toBeNull();
  });

  it('returns null for a message nowhere in the log', () => {
    expect(locateInLog(log, 'Undefined control sequence.')).toBeNull();
    expect(locateInLog('', 'Missing $ inserted.')).toBeNull();
  });
});
