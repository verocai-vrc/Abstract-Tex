// Path helpers shared by the controller and the tree. Pure, so they are unit-tested.

import type { ProjectInfo, TreeNode } from './ipc';

/** Absolute path → project-relative with forward slashes, or null if outside the project. */
export function toRelative(absolutePath: string, rootDir: string): string | null {
  const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '');
  const abs = norm(absolutePath);
  const root = norm(rootDir);
  // Windows paths are case-insensitive and the watcher may report a different case than the
  // dialog returned; comparing case-insensitively costs nothing on other platforms.
  if (abs.toLowerCase() === root.toLowerCase()) return '';
  if (!abs.toLowerCase().startsWith(root.toLowerCase() + '/')) return null;
  return abs.slice(root.length + 1);
}

export function baseName(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] ?? path;
}

export function isTexSource(path: string): boolean {
  return /\.(tex|bib|sty|cls|bbx|cbx|def|ltx)$/i.test(path);
}

/** Whether a change to this project-relative path should trigger a recompile (S4.1).
 *
 * A non-`.tex` compile input (`.bib`/`.sty`/`.cls`/…) always does, as before this loop. A `.tex`
 * file only does when it is actually part of the document: listed in `documentFiles`, or when
 * the include graph could not be fully resolved (`documentFilesComplete` is false) — in that
 * case we do not know everything the document includes, and compiling too often is the direction
 * to fail in, not silently skipping a file that turns out to be part of it. */
export function shouldCompileFor(
  relative: string,
  project: Pick<ProjectInfo, 'documentFiles' | 'documentFilesComplete'>,
): boolean {
  if (!isTexSource(relative)) return false;
  if (!/\.tex$/i.test(relative)) return true;
  if (!project.documentFilesComplete) return true;
  return project.documentFiles.includes(relative);
}

/** Every file in the tree, depth-first in the order the tree shows them. Directories are not
 * files: the quick-open list has nothing to do with one. */
export function listFiles(nodes: readonly TreeNode[]): string[] {
  const files: string[] = [];
  for (const node of nodes) {
    if (node.isDir) files.push(...listFiles(node.children));
    else files.push(node.path);
  }
  return files;
}
