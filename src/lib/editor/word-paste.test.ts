import { EditorSelection, EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { isFromWordProcessor, latexFromWord, wordPasteHandler } from './word-paste';

const WORD =
  '<html xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" ' +
  'xmlns:w="urn:schemas-microsoft-com:office:word"><head><meta name=ProgId content=Word.Document>' +
  '<meta name=Generator content="Microsoft Word 15"></head><body><!--StartFragment--><p class=MsoNormal>x</p></body></html>';

describe('what was copied in a word processor', () => {
  it.each([
    ['Word, as WebView2 gives it', WORD],
    ['Word’s generator alone', '<head><meta name=Generator content="Microsoft Word 15"></head>'],
    // WebKit (Linux, macOS) takes the head away from what another program put on the clipboard. These
    // are what WebKitGTK 2.4x was seen to hand to a paste handler.
    ['Word, as WebKit gives it', '<p class="MsoNormal" style="caret-color: rgb(0, 0, 0);">x<o:p></o:p></p>'],
    ['a style of Word’s', "<p style='margin:0;mso-pagination:none'>x</p>"],
    ['Word on the web', '<div class="OutlineElement Ltr SCXW74839182 BCX0"><p class="Paragraph SCXW74839182 BCX0">x</p></div>'],
    ['LibreOffice', '<html><head><meta name="generator" content="LibreOffice 24.2.7.2 (Linux)"/><style>p { line-height: 115% }</style></head><body><p>x</p>'],
    ['LibreOffice, as WebKit gives it', '<p style="line-height: 18.4px; margin-bottom: 0.1in; background: none; caret-color: rgb(0, 0, 0);">x</p>'],
    ['OpenOffice', '<META NAME="GENERATOR" CONTENT="OpenOffice 4.1.14  (Win32)">'],
    ['Google Docs', '<meta charset="utf-8"><b style="font-weight:normal;" id="docs-internal-guid-1b2c3d4e-7fff-1111"><p dir="ltr">x</p></b>'],
  ])('%s', (_name, html) => {
    expect(isFromWordProcessor(html)).toBe(true);
  });

  it.each([
    ['plain text: our editor, a terminal', ''],
    ['VS Code', '<meta charset="utf-8"><div style="color: #ccc;font-family: \'Fira Code\', monospace;white-space: pre;"><div>x_1 &amp; 5%</div></div>'],
    ['a web page', '<meta charset="utf-8"><p style="box-sizing: border-box; margin: 0px 0px 10px;">x</p>'],
    ['a web page about Word’s HTML', '<pre>&lt;p class=MsoNormal style="mso-x:y"&gt; xmlns:w="urn:schemas-microsoft-com:office:word"</pre>'],
    ['a web page that names Word', '<p>Generator content="Microsoft Word 15"</p>'],
    ['Excel', '<html xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><head><meta name=ProgId content=Excel.Sheet><meta name=Generator content="Microsoft Excel 15"><style>td {mso-style-parent:style0}</style></head><td style="mso-x:y">'],
    ['LibreOffice Calc', '<html><head><meta name="generator" content="LibreOffice 24.2"/><style type="text/css">\nbody,div,table,thead,tbody,tfoot,tr,th,td,p { font-size:x-small }</style></head><body><table>'],
    ['LibreOffice Calc, as WebKit gives it', '<table cellspacing="0" border="0" style="font-family: x;">'],
    ['a program for Macs (Xcode says the same)', '<meta name="Generator" content="Cocoa HTML Writer"><p class="p1">x</p>'],
  ])('is not %s', (_name, html) => {
    expect(isFromWordProcessor(html)).toBe(false);
  });
});

describe('the text of a word processor, as LaTeX', () => {
  it.each([
    ['one blank line between paragraphs', 'One.\r\nTwo.\r\n\r\n\r\nThree.\r\n', 'One.\n\nTwo.\n\nThree.'],
    ['the same with bare line feeds', 'One.\nTwo.', 'One.\n\nTwo.'],
    ['an empty paragraph that holds a space', 'One.\n \u00a0\nTwo.', 'One.\n\nTwo.'],
    ['the signs LaTeX keeps for itself', '50% & #1 a_b $5', '50\\% \\& \\#1 a\\_b \\$5'],
    ['braces, tilde, caret and backslash', '{a} ~ ^ C:\\x', '\\{a\\} \\textasciitilde{} \\textasciicircum{} C:\\textbackslash{}x'],
    ['accents and ordinary signs stay', 'Ação, naïve, ½ § 10° €3 → ±', 'Ação, naïve, ½ § 10° €3 → ±'],
    ['quotes and dashes, in the spelling every font understands', '“It’s ‘so’” – a—b…', "``It's `so'{}'' -- a---b\\dots{}"],
    ['quotes of other languages', '«oui» „Was?“ ‚ja!‘', "<<oui>> ,,Was?{}`` ‚ja!{}`"],
    ['quotes inside quotes', '“‘a’” ‘“b”’', "``{}`a'{}'' `{}``b''{}'"],
    ['hyphens that were not a dash', 'a--b –- c', 'a-{}-b --{}- c'],
    ['tabs and no-break spaces', '\t1.\tOne\t10\u00a0kg', '1. One 10~kg'],
    ['thin and other wide spaces', '10\u202f000\u2009x a\u2003b', '10\\,000\\,x a b'],
    ['what is not seen goes', 'hy\u00adphen\u200b \ufeffbom\u0007 non\u001ebreaking op\u001ftional', 'hyphen bom non-breaking optional'],
    ['a line broken by hand', 'Line one\u000bLine two\n[x]\u000b[y]', 'Line one\\\\\nLine two\n\n[x]\\\\{}\n[y]'],
    ['a line break with nothing to break', '\u000bOne\u000b\u000bTwo\u000b\nThree', 'One\n\nTwo\n\nThree'],
    ['a page break', 'One\u000cTwo', 'One\n\nTwo'],
    ['signs that the font of the text does not have', 'p ≤ 0.05, α-helix, CO₂, 10⁻³', 'p $\\leq$ 0.05, $\\alpha$-helix, CO$_2$, 10$^-$³'],
    ['but Greek words are words', 'η λέξη', '$\\eta$ λέξη'],
    ['nothing', ' \r\n', ''],
  ])('%s', (_name, pasted, latex) => {
    expect(latexFromWord(pasted)).toBe(latex);
  });

  it.each(['\\section{One}\r\n50% & more', 'see \\cite[p. 3]{knuth}', '\\begin{itemize}', '\\documentclass', '\\usepackage'])(
    'leaves LaTeX that somebody kept in Word as it is: %s',
    (pasted) => {
      expect(latexFromWord(pasted)).toBeNull();
    },
  );

  it('does not take a Windows address for LaTeX', () => {
    expect(latexFromWord('C:\\Users\\me')).toBe('C:\\textbackslash{}Users\\textbackslash{}me');
  });

  it('is quick with a megabyte', () => {
    const paragraph = `${'Some “words” of 50% & more, with a_b. '.repeat(13)}\r\n`;
    const pasted = paragraph.repeat(2100);
    expect(pasted.length).toBeGreaterThan(1_000_000);
    const before = performance.now();
    latexFromWord(pasted);
    expect(performance.now() - before).toBeLessThan(1000);
  });
});

describe('pasting it', () => {
  /** A view reduced to what the handler uses, its text kept in a Y.Text with the undo manager of a document. */
  function editor(doc: string, ranges: [number, number][], readOnly = false) {
    const ytext = new Y.Doc().getText();
    ytext.insert(0, doc);
    const origin = {};
    const undo = new Y.UndoManager(ytext, { captureTimeout: 500, trackedOrigins: new Set([origin]) });
    const view = {
      state: EditorState.create({
        doc,
        selection: EditorSelection.create(ranges.map(([from, to]) => EditorSelection.range(from, to))),
        extensions: [EditorState.allowMultipleSelections.of(true), EditorState.readOnly.of(readOnly)],
      }),
      dispatch(...specs: Parameters<EditorState['update']>) {
        const transaction = view.state.update(...specs);
        view.state = transaction.state;
        ytext.doc!.transact(() => {
          let moved = 0;
          transaction.changes.iterChanges((from, to, _fromB, _toB, insert) => {
            ytext.delete(from + moved, to - from);
            ytext.insert(from + moved, insert.toString());
            moved += insert.length - (to - from);
          });
        }, origin);
      },
    } as unknown as EditorView & { state: EditorState };
    return { view, undo, text: () => ytext.toString() };
  }
  function pasteEvent(text: string, html = WORD): ClipboardEvent {
    return {
      clipboardData: { getData: (type: string) => (type === 'text/plain' ? text : type === 'text/html' ? html : '') },
    } as unknown as ClipboardEvent;
  }
  function paste(doc: string, ranges: [number, number][], text: string, html = WORD, readOnly = false) {
    const it = editor(doc, ranges, readOnly);
    const told = vi.fn();
    const handled = wordPasteHandler(() => it.undo.stopCapturing(), told)(pasteEvent(text, html), it.view);
    return { ...it, told, handled };
  }

  it('writes it as LaTeX, says so, and leaves the cursor after it', () => {
    const done = paste('ab', [[1, 1]], '50%\r\nmore');
    expect(done.handled).toBe(true);
    expect(done.text()).toBe('a50\\%\n\nmoreb');
    expect(done.view.state.doc.toString()).toBe(done.text());
    expect(done.view.state.selection.main.head).toBe(done.text().length - 1);
    expect(done.told).toHaveBeenCalledOnce();
  });

  it('is undone in two steps: as it was copied, then not at all', () => {
    const { undo, text, view } = editor('ab', [[1, 1]]);
    view.dispatch({ changes: { from: 0, insert: 'x' } });
    wordPasteHandler(() => undo.stopCapturing(), () => {})(pasteEvent('5%\r\n6'), view);
    view.dispatch({ changes: { from: 0, insert: 'y' } });
    expect(text()).toBe('yxa5\\%\n\n6b');
    undo.undo();
    expect(text()).toBe('xa5\\%\n\n6b');
    undo.undo();
    expect(text()).toBe('xa5%\n6b');
    undo.undo();
    expect(text()).toBe('xab');
  });

  it('replaces each of several selections', () => {
    const done = paste('one two three', [[0, 3], [4, 7], [8, 8]], '5%');
    expect(done.text()).toBe('5\\% 5\\% 5\\%three');
    expect(done.view.state.selection.ranges.map((range) => [range.from, range.to])).toEqual([[3, 3], [7, 7], [11, 11]]);
  });

  // A whole paragraph copied in Word ends with its paragraph mark. Two pasted one after the other must
  // stay two; inside \title{ a blank line would be an error.
  it.each([
    ['at the end of a line', 'x\n', 1, 'xOne 1\n\n\n'],
    ['before a paragraph', 'x', 0, 'One 1\n\nx'],
    ['in a title', '\\title{}', 7, '\\title{One 1}'],
    ['in a title over several lines', '\\title{\n\n}', 8, '\\title{\nOne 1\n}'],
    ['in a sentence', 'a b', 2, 'a One 1b'],
  ])('the paragraph mark at its end: %s', (_where, doc, at, after) => {
    const done = paste(doc, [[at, at]], 'One\u00a01\r\n');
    expect(done.text().replace('~', ' ')).toBe(after);
  });

  it('says nothing when only the paragraph mark was minded', () => {
    const done = paste('', [[0, 0]], 'One.\r\n');
    expect(done.text()).toBe('One.\n\n');
    expect(done.told).not.toHaveBeenCalled();
  });

  it.each([
    ['not from a word processor', '', [0, 0], '50%', '', false],
    ['nothing to change', '', [0, 0], 'word', WORD, false],
    ['LaTeX kept in Word', '', [0, 0], '\\section{A}\r\n50%', WORD, false],
    ['a picture', '', [0, 0], '', WORD, false],
    ['a file that cannot be written', '', [0, 0], '50%', WORD, true],
    ['inside verbatim', '\\begin{verbatim}\n\n\\end{verbatim}', [17, 17], '50%', WORD, false],
    ['inside a listing', '\\begin{lstlisting}[language=C]\n', [31, 31], '50%', WORD, false],
    ['inside an address', '\\url{}', [5, 5], 'https://a.b/c_d%20', WORD, false],
    ['inside a name', 'see \\parencite[3]{}', [18, 18], 'a_b', WORD, false],
    ['a picture’s file name', '\\includegraphics[width=3cm]{}', [28, 28], 'a_b', WORD, false],
  ] as const)('leaves the paste alone: %s', (_why, doc, range, text, html, readOnly) => {
    const done = paste(doc, [[range[0], range[1]]], text, html, readOnly);
    expect(done.handled).toBe(false);
    expect(done.text()).toBe(doc);
    expect(done.told).not.toHaveBeenCalled();
  });

  it('is LaTeX again after the verbatim has ended', () => {
    const doc = '\\begin{verbatim}\nx\n\\end{verbatim}\n';
    expect(paste(doc, [[doc.length, doc.length]], '50%').text()).toBe(`${doc}50\\%`);
  });
});
