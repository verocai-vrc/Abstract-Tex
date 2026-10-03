// Placeholders: the lines a template asks the author to replace (S11.12). A template marks each
// with an ordinary LaTeX comment, `% FILL IN: …`, so the file never depends on this app and
// nothing needs rewriting when the author deletes the comment — a line without the marker simply
// stops being a placeholder.
//
// Two halves. The top of the file is pure — find the marker lines, order the project's files, pick
// the next one — and is what `placeholders.test.ts` exercises with literals. The bottom is the
// CodeMirror extension that draws a gutter mark and a quiet highlight on a marker line, and
// reports where the cursor is so *Go to next placeholder* knows where "next" starts.

import { RangeSetBuilder } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, gutter, GutterMarker, ViewPlugin, type ViewUpdate } from '@codemirror/view';
import type { TreeNode } from '../ipc';

/** `% FILL IN:` in a comment — not `\% FILL IN:`, which is a percent sign in the text. */
// Written without a look-behind (`(?<!\\)`) so it runs on every webview version the app can meet.
const MARKER = /(^|[^\\])%\s*FILL IN:/;

export function isPlaceholderLine(text: string): boolean {
  return MARKER.test(text);
}

/** The 1-based numbers of the lines in `text` that carry the marker, in order. */
export function findPlaceholderLines(text: string): number[] {
  const lines: number[] = [];
  const all = text.split('\n');
  for (let index = 0; index < all.length; index++) if (isPlaceholderLine(all[index]!)) lines.push(index + 1);
  return lines;
}

/** Every `.tex` file in a project tree, in the order the tree lists them. */
export function texPathsOf(tree: readonly TreeNode[]): string[] {
  const paths: string[] = [];
  for (const node of tree) {
    if (node.isDir) paths.push(...texPathsOf(node.children));
    else if (node.path.endsWith('.tex')) paths.push(node.path);
  }
  return paths;
}

/** The order *next* walks the files in: the document's own files first, in reading order (root,
 * then what it includes), then any other `.tex` file. A placeholder in the preamble is met before
 * one in chapter 3 because that is the order the author would fill them in. */
export function orderedTexFiles(documentFiles: readonly string[], allTexFiles: readonly string[]): string[] {
  const ordered = documentFiles.filter((path) => path.endsWith('.tex'));
  for (const path of allTexFiles) if (!ordered.includes(path)) ordered.push(path);
  return ordered;
}

export interface PlaceholderSpot {
  path: string;
  line: number;
}

/**
 * The first placeholder strictly after `from` in `order`, wrapping round to the start; the first
 * one anywhere when `from` is `null` (a fresh project, no cursor yet). Wrapping can land on
 * `from` itself when it is the only one left. `null` when there is none at all.
 *
 * A file the cursor is in but `order` does not list is treated as coming last, so *next* from
 * such a file still goes somewhere.
 */
export function nextPlaceholder(
  order: readonly string[],
  byFile: ReadonlyMap<string, readonly number[]>,
  from: PlaceholderSpot | null,
): PlaceholderSpot | null {
  const spots: PlaceholderSpot[] = [];
  for (const path of order) for (const line of byFile.get(path) ?? []) spots.push({ path, line });
  if (spots.length === 0) return null;
  if (from === null) return spots[0]!;
  const rank = (path: string) => {
    const index = order.indexOf(path);
    return index === -1 ? order.length : index;
  };
  const after = spots.find(
    (spot) => rank(spot.path) > rank(from.path) || (spot.path === from.path && spot.line > from.line),
  );
  return after ?? spots[0]!;
}

/** The status bar's words: "3 left to fill in", and nothing at all for none (never "0 left"). */
export function leftToFillIn(count: number): string {
  return count > 0 ? `${count} left to fill in` : '';
}

// ---- The CodeMirror half ------------------------------------------------------------------------

class PlaceholderMarker extends GutterMarker {
  toDOM(): Node {
    const mark = document.createElement('span');
    mark.className = 'cm-placeholder-mark';
    mark.title = 'Replace this: it is a placeholder (F8 jumps to the next)';
    mark.textContent = '✎';
    return mark;
  }
}
const marker = new PlaceholderMarker();
const lineHighlight = Decoration.line({ class: 'cm-placeholder-line' });

/** A line highlight on the visible marker lines. Only the viewport is scanned, so the cost of a
 * keystroke is the screen, not the document. */
const highlights = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) this.decorations = this.build(update.view);
    }
    build(view: EditorView): DecorationSet {
      const builder = new RangeSetBuilder<Decoration>();
      let last = -1;
      for (const { from, to } of view.visibleRanges) {
        for (let position = from; position <= to; ) {
          const line = view.state.doc.lineAt(position);
          if (line.from > last && isPlaceholderLine(line.text)) builder.add(line.from, line.from, lineHighlight);
          last = line.from;
          position = line.to + 1;
        }
      }
      return builder.finish();
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const theme = EditorView.baseTheme({
  '.cm-placeholder-line': { backgroundColor: 'color-mix(in srgb, var(--accent) 9%, transparent)' },
  '.cm-placeholder-gutter': { width: '1.4em' },
  '.cm-placeholder-mark': { color: 'var(--accent)', cursor: 'default' },
});

/** Reports the cursor's 1-based line to `onCursorLine` on creation and whenever it moves line. */
function cursorReporter(onCursorLine: (line: number) => void) {
  return ViewPlugin.fromClass(
    class {
      line = 0;
      constructor(view: EditorView) {
        this.report(view);
      }
      update(update: ViewUpdate) {
        if (update.selectionSet || update.docChanged) this.report(update.view);
      }
      report(view: EditorView) {
        const line = view.state.doc.lineAt(view.state.selection.main.head).number;
        if (line === this.line) return;
        this.line = line;
        onCursorLine(line);
      }
    },
  );
}

/** The gutter mark, the line highlight, and (when `onCursorLine` is given) the cursor's line. */
export function placeholderMarks(onCursorLine?: (line: number) => void) {
  return [
    highlights,
    gutter({
      class: 'cm-placeholder-gutter',
      lineMarker: (view, block) => (isPlaceholderLine(view.state.doc.lineAt(block.from).text) ? marker : null),
      lineMarkerChange: (update) => update.docChanged || update.viewportChanged,
      initialSpacer: () => marker,
    }),
    theme,
    ...(onCursorLine ? [cursorReporter(onCursorLine)] : []),
  ];
}
