import { describe, expect, it } from 'vitest';
import { build } from './diagnostics';
import type { Diagnostic } from '../ipc';

function diag(line: number | null, severity: Diagnostic['severity'], title = 't'): Diagnostic {
  return { title, explanation: '', line, severity, rule: null, rawMessage: '' };
}

/** Pretend every line is 10 characters long. */
const lineStart = (line: number) => (line - 1) * 10;

function lines(set: ReturnType<typeof build>): Array<[number, string, string]> {
  const out: Array<[number, string, string]> = [];
  const cursor = set.iter();
  while (cursor.value) {
    out.push([cursor.from / 10 + 1, cursor.value.severity, cursor.value.title]);
    cursor.next();
  }
  return out;
}

describe('diagnostic gutter markers', () => {
  it('places one dot per line, sorted, and skips diagnostics with no line', () => {
    const set = build([diag(5, 'warning'), diag(null, 'error'), diag(2, 'error')], 10, lineStart);
    expect(lines(set)).toEqual([
      [2, 'error', 't'],
      [5, 'warning', 't'],
    ]);
  });

  it('an error wins over a warning on the same line', () => {
    const set = build([diag(3, 'warning', 'w'), diag(3, 'error', 'e')], 10, lineStart);
    expect(lines(set)).toEqual([[3, 'error', 'e']]);
    const reversed = build([diag(3, 'error', 'e'), diag(3, 'warning', 'w')], 10, lineStart);
    expect(lines(reversed)).toEqual([[3, 'error', 'e']]);
  });

  it('clamps a line TeX claims past the end of the file onto the last line', () => {
    const set = build([diag(99, 'error')], 10, lineStart);
    expect(lines(set)).toEqual([[10, 'error', 't']]);
  });
});
