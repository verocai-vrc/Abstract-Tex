// Path helpers shared by the controller and the tree. Pure, so they are unit-tested.

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
