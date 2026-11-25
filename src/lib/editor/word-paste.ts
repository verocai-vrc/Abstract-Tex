// Text pasted from a word processor, written the way LaTeX needs (S17.18). Word's plain text has one
// line break per paragraph and bare `% & # _ $`. To LaTeX one line break is a space, so every paragraph
// became one, and a `%` silently took the rest of its paragraph out of the PDF. Only what certainly
// comes from a word processor is changed: code, a terminal and a web page are pasted as they are.

import { EditorSelection } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';

/** Whether the clipboard's HTML was written by a word processor. WebView2 hands over all of it; WebKit
 * (Linux, macOS) takes the `<head>` away, and with it the generator's name, so each program is known by
 * a mark in its body as well. Marks count inside a tag only, so a page *about* them has none. */
export function isFromWordProcessor(html: string): boolean {
  const start = html.slice(0, 20_000); // the marks are at the top, and there may be megabytes
  // Spreadsheets carry the same marks, but their rows are not paragraphs.
  if (/<[^>]*(?:Excel\.Sheet|Microsoft Excel)|body,div,table,thead/i.test(start)) return false;
  return (
    /<html[^>]*urn:schemas-microsoft-com:office:word/i.test(start) ||
    /<meta[^>]*(?:Word\.Document|Microsoft Word|content=["']?(?:LibreOffice|OpenOffice))/i.test(start) ||
    /<[^>]*\sclass=["']?Mso[A-Z]/.test(start) ||
    /<[^>]*\sstyle=["'][^>]*\bmso-/.test(start) ||
    /<[^>]*\sclass="[^">]*\bSCXW\d/.test(start) || // Word on the web
    /<[^>]*\sid=["']?docs-internal-guid-/.test(start) || // Google Docs
    // LibreOffice without its head: it alone measures the space after a paragraph in inches or centimetres.
    /^(?:\s|<meta[^>]*>)*<(?:p|h[1-6])\s[^>]*margin-bottom: [\d.]+(?:in|cm)\b/.test(start)
  );
}

/** LaTeX that somebody kept in Word: a command with something in braces, or one that begins a document. */
const LATEX_SOURCE = /\\[a-zA-Z]+\*?(?:\[[^\]\n]*\])?\{|\\(?:documentclass|usepackage)\b/;

/** The most common signs the engine's text font does not have: LaTeX leaves them out without an error. */
const MATHS =
  'α alpha β beta γ gamma δ delta ε varepsilon ϵ epsilon ζ zeta η eta θ theta ι iota κ kappa λ lambda μ mu ν nu ξ xi ' +
  'π pi ρ rho ς varsigma σ sigma τ tau υ upsilon φ varphi ϕ phi χ chi ψ psi ω omega Γ Gamma Δ Delta Θ Theta ' +
  'Λ Lambda Ξ Xi Π Pi Σ Sigma Υ Upsilon Φ Phi Ψ Psi Ω Omega ≤ leq ≥ geq ≠ neq ≈ approx ∼ sim ∝ propto ∈ in ∉ notin ' +
  '∩ cap ∪ cup ∅ emptyset ∀ forall ∃ exists ∂ partial ∇ nabla ∑ sum ∫ int ⇒ Rightarrow ↔ leftrightarrow ′ prime';

/** Between two signs that LaTeX would otherwise draw as one. A character no text contains. */
const APART = '\u0000';

/** How a sign is written so that the PDF shows it. Letters with accents need nothing. */
const WRITTEN = new Map<string, string>([
  ...[...'%&#_${}'].map((sign): [string, string] => [sign, `\\${sign}`]),
  ['\\', '\\textbackslash{}'],
  ['~', '\\textasciitilde{}'],
  ['^', '\\textasciicircum{}'],
  // The engine's own fonts have these, but not those of `\usepackage[T1]{fontenc}`, which many documents
  // ask for: there every apostrophe and dash of Word's vanished. This spelling works in both.
  ['‘', '`'],
  ['’', "'"],
  ['“', '``'],
  ['”', "''"],
  ['„', ',,'],
  ['«', '<<'],
  ['»', '>>'],
  ['–', '--'],
  ['—', '---'],
  ['…', '\\dots{}'],
  // `~` for a space that must not end a line, not the character itself: this one can be seen.
  ['\u00a0', '~'],
  ['\u2007', '~'],
  ['\u2009', '\\,'],
  ['\u202f', '\\,'],
  ['\u001e', '-'], // Word's own no-break hyphen
  [APART, '{}'],
  ...[...MATHS.matchAll(/(\S) (\w+)/g)].map((pair): [string, string] => [pair[1]!, `$\\${pair[2]}$`]),
  ...[...'₀₁₂₃₄₅₆₇₈₉'].map((sign, i): [string, string] => [sign, `$_${i}$`]),
  ...[...'⁰⁴⁵⁶⁷⁸⁹⁺⁻'].map((sign, i): [string, string] => [sign, `$^${'0456789+-'[i]}$`]),
]);
WRITTEN.set('′', "$'$"); // a raised sign of its own: `\prime` alone is not

/** The text of a word processor as LaTeX that shows it, or `null` when it is LaTeX already. */
export function latexFromWord(pasted: string): string | null {
  if (LATEX_SOURCE.test(pasted)) return null;
  return pasted
    .replace(/\r\n?|[\f\u000e\u0085\u2029]/g, '\n') // a page or column break ends a paragraph too
    .replace(/\u2028/g, '\v')
    // What is not seen: control characters (some stop the build), marks of where a word may be broken.
    .replace(/[\u0000-\u0008\u000e-\u001d\u001f\u007f\u00ad\u200b\u2060\ufeff]/g, '')
    .replace(/\t+|[\u2000-\u2006\u2008\u200a\u205f]/g, ' ')
    .replace(/([?!`‘“])(?=[‘“])|(['’”])(?=[’”])|([-–—])(?=[-–—])/g, `$&${APART}`)
    .split(/\n|\v{2,}/)
    .map((paragraph) =>
      paragraph
        .split('\v') // Word's line break by hand (Shift Enter)
        .map((line) => line.trim())
        .filter(Boolean)
        // A run of Greek letters is a word of somebody's language unless it is one letter long.
        .map((line) => line.replace(/\p{Script=Greek}+|[^A-Za-z0-9 .,]/gu, (sign) => WRITTEN.get(sign) ?? sign))
        // `\\` would take a `[` or `*` that follows it for its own.
        .reduce((before, line) => (before ? `${before}\\\\${/^[[*]/.test(line) ? '{}' : ''}\n${line}` : line), ''),
    )
    .filter(Boolean)
    .join('\n\n');
}

/** Whether text before the cursor says that what is typed there is not prose: in a verbatim environment,
 * or in the braces of a command that takes a name, a file or an address. */
function isLiteral(before: string): boolean {
  const last = [...before.matchAll(/\\(begin|end)\{(?:[vV]erbatim|lstlisting|minted|comment|alltt)\*?\}/g)].pop();
  if (last?.[1] === 'begin') return true;
  const line = before.slice(before.lastIndexOf('\n') + 1);
  return /\\(?:url|href|path|includegraphics|input|include|bibliography|addbibresource|\w*cite\w*|\w*ref|label)\*?(?:\[[^\]]*\])*\{[^{}]*$/.test(line);
}

/** Answers a paste that comes from a word processor. `stopCapturing` is the undo manager's: what follows
 * is a step of its own, so that the first Ctrl Z gives the text as it was copied, and the second removes
 * it. `tell` is called when signs or paragraphs were changed. */
export function wordPasteHandler(stopCapturing: () => void, tell: () => void) {
  return (event: ClipboardEvent, view: EditorView): boolean => {
    const copied = event.clipboardData?.getData('text/plain') ?? '';
    if (!copied || view.state.readOnly || !isFromWordProcessor(event.clipboardData?.getData('text/html') ?? '')) return false;
    let latex = latexFromWord(copied);
    const { doc, selection } = view.state;
    if (!latex || isLiteral(doc.sliceString(0, selection.main.from))) return false;

    const asCopied = view.state.toText(copied);
    const changed = latex !== asCopied.toString().trim();
    // A paragraph copied with its end stays one when the next is pasted after it, but not in the middle
    // of a line or before a brace: `\title{` cannot hold a blank line.
    const line = doc.lineAt(selection.main.from);
    const ahead = doc.sliceString(selection.main.to, selection.main.to + 200);
    const atAnEdge = !line.text.slice(0, selection.main.from - line.from).trim() || /^[^\S\n]*(?:\n|$)/.test(ahead);
    if (/[\r\n]$/.test(copied) && atAnEdge && !/^\s*[}\]]/.test(ahead)) latex += '\n\n';
    else if (!changed && latex === asCopied.toString()) return false;

    const how = { userEvent: 'input.paste', scrollIntoView: true };
    const written = view.state.toText(latex);
    stopCapturing();
    if (changed) {
      view.dispatch(view.state.replaceSelection(asCopied), how);
      stopCapturing();
      // Each cursor is now at the end of its copy.
      const overTheCopy = view.state.changeByRange(({ head }) => ({
        changes: { from: head - asCopied.length, to: head, insert: written },
        range: EditorSelection.cursor(head - asCopied.length + written.length),
      }));
      view.dispatch(overTheCopy, how);
      tell();
    } else {
      view.dispatch(view.state.replaceSelection(written), how);
    }
    stopCapturing();
    return true;
  };
}
