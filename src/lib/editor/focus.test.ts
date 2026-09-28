import { EditorSelection, EditorState, Text } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { currentParagraphRange, focusField } from './focus';

describe('currentParagraphRange', () => {
  it('spans a single-line paragraph surrounded by blank lines', () => {
    const doc = 'Intro line.\n\nOne paragraph.\n\nOutro line.';
    const cursor = doc.indexOf('One paragraph.') + 3;
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(doc.slice(from, to)).toBe('One paragraph.');
  });

  it('spans every contiguous non-blank line around the cursor', () => {
    const doc = ['First para line one.', 'First para line two.', '', 'Second para.'].join('\n');
    const cursor = doc.indexOf('line two') + 4;
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(doc.slice(from, to)).toBe('First para line one.\nFirst para line two.');
  });

  it('treats a blank cursor line as its own paragraph', () => {
    const doc = 'Para one.\n\nPara two.';
    const cursor = doc.indexOf('\n\n') + 1; // the blank line itself
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(doc.slice(from, to)).toBe('');
  });

  it('stops at the start of the document', () => {
    const doc = 'Line one.\nLine two.\n\nLine four.';
    const cursor = 2; // inside "Line one."
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(from).toBe(0);
    expect(doc.slice(from, to)).toBe('Line one.\nLine two.');
  });

  it('stops at the end of the document with no trailing blank line', () => {
    const doc = 'Para one.\n\nLast line one.\nLast line two.';
    const cursor = doc.length - 2;
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(to).toBe(doc.length);
    expect(doc.slice(from, to)).toBe('Last line one.\nLast line two.');
  });

  it('handles a cursor at offset 0 in a single-line document', () => {
    const doc = 'Just one line.';
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), 0);
    expect(doc.slice(from, to)).toBe('Just one line.');
  });

  it('treats a whitespace-only line as blank', () => {
    const doc = 'Para one.\n   \nPara two.';
    const cursor = doc.indexOf('Para two.') + 2;
    const { from, to } = currentParagraphRange(Text.of(doc.split('\n')), cursor);
    expect(doc.slice(from, to)).toBe('Para two.');
  });
});

describe('focusField (S9.6)', () => {
  const doc = ['Para one, line one.', 'Para one, line two.', '', 'Para two.', '', 'Para three.'].join('\n');

  function stateAt(offset: number): EditorState {
    return EditorState.create({ doc, selection: EditorSelection.cursor(offset), extensions: [focusField] });
  }

  function dimmedLines(state: EditorState): number[] {
    const lines: number[] = [];
    for (const cursor = state.field(focusField).decorations.iter(); cursor.value; cursor.next()) {
      lines.push(state.doc.lineAt(cursor.from).number);
    }
    return lines;
  }

  it('dims every line outside the paragraph holding the cursor', () => {
    const state = stateAt(doc.indexOf('Para two.'));
    expect(dimmedLines(state)).toEqual([1, 2, 3, 5, 6]);
  });

  it('keeps everything, and rebuilds nothing, when the cursor moves within its paragraph', () => {
    const state = stateAt(0);
    const moved = state.update({ selection: EditorSelection.cursor(doc.indexOf('line two')) }).state;
    expect(moved.field(focusField)).toBe(state.field(focusField));
  });

  it('rebuilds when the cursor moves to another paragraph', () => {
    const state = stateAt(0);
    const moved = state.update({ selection: EditorSelection.cursor(doc.indexOf('Para three.')) }).state;
    expect(moved.field(focusField).rebuilds).toBe(state.field(focusField).rebuilds + 1);
    expect(dimmedLines(moved)).toEqual([1, 2, 3, 4, 5]);
  });

  it('shifts the dimming instead of rebuilding while typing inside the paragraph', () => {
    let state = stateAt(doc.indexOf('Para two.') + 4);
    const before = state.field(focusField).rebuilds;
    for (const char of ' and more') {
      const at = state.selection.main.head;
      state = state.update({ changes: { from: at, insert: char }, selection: EditorSelection.cursor(at + 1) }).state;
    }
    expect(state.field(focusField).rebuilds).toBe(before);
    expect(state.doc.line(4).text).toBe('Para and more two.');
    expect(dimmedLines(state)).toEqual([1, 2, 3, 5, 6]);
  });

  it('keeps a new line typed inside the paragraph lit, without rebuilding', () => {
    let state = stateAt(doc.indexOf('Para two.') + 'Para two.'.length);
    const before = state.field(focusField).rebuilds;
    const at = state.selection.main.head;
    state = state.update({ changes: { from: at, insert: '\nA second line.' }, selection: EditorSelection.cursor(at + 15) }).state;
    expect(state.field(focusField).rebuilds).toBe(before);
    expect(dimmedLines(state)).toEqual([1, 2, 3, 6, 7]);
  });

  it('rebuilds when typing splits the paragraph with a blank line', () => {
    let state = stateAt(doc.indexOf('line two') - 1);
    const before = state.field(focusField).rebuilds;
    const at = state.selection.main.head;
    state = state.update({ changes: { from: at, insert: '\n\n' }, selection: EditorSelection.cursor(at + 2) }).state;
    expect(state.field(focusField).rebuilds).toBe(before + 1);
  });

  it('rebuilds when an edit lands outside the lit paragraph (a collaborator, or a file change)', () => {
    let state = stateAt(0);
    const before = state.field(focusField).rebuilds;
    const elsewhere = state.doc.line(6).from;
    state = state.update({ changes: { from: elsewhere, insert: 'Edited: ' } }).state;
    expect(state.field(focusField).rebuilds).toBe(before + 1);
    expect(dimmedLines(state)).toEqual([3, 4, 5, 6]);
  });
});
