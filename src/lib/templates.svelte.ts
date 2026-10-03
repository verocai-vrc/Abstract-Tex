// The New project window's state (S11.11; DESIGN.md §6 "Start a document"): the catalog, which card
// is highlighted, what has been typed, and what the last attempt said.
//
// Only the controller writes to `newProject`, as with `clone`, `git` and `github`. The filter is a
// plain function so `templates.test.ts` exercises it with literals and no Svelte runtime.

import type { TemplateInfo } from './ipc';

/** The categories present in `catalog`, in the order the catalog first uses them. A category with
 * no template is never offered: a chip that filters to nothing is a dead end. */
export function categoriesOf(catalog: readonly TemplateInfo[]): string[] {
  const seen: string[] = [];
  for (const template of catalog) if (!seen.includes(template.category)) seen.push(template.category);
  return seen;
}

/** A category as a chip says it: "Essay", and "CV" rather than "Cv". Plain CSS capitalising cannot
 * know an initialism. */
export function categoryLabel(category: string): string {
  if (category === 'all') return 'All';
  if (category.toLowerCase() === 'cv') return 'CV';
  return category.charAt(0).toUpperCase() + category.slice(1);
}

/**
 * The templates in `category` (or all of them, for `'all'`) whose name, category or description
 * contains every word of `query`. Case-insensitive; an empty query keeps everything; the
 * catalog's own order is kept, so the cards do not shuffle while someone types.
 */
export function filterTemplates(catalog: readonly TemplateInfo[], category: string, query: string): TemplateInfo[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return catalog.filter((template) => {
    if (category !== 'all' && template.category !== category) return false;
    const haystack = `${template.name} ${template.category} ${template.description}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

/** The id one step along `delta` (+1 / -1) from `current` among `shown`, stopping at the ends; the
 * first card when nothing is highlighted yet or the highlighted one has been filtered away. */
export function neighbour(shown: readonly TemplateInfo[], current: string | null, delta: number): string | null {
  if (shown.length === 0) return null;
  const index = shown.findIndex((template) => template.id === current);
  if (index === -1) return shown[0]!.id;
  return shown[Math.min(shown.length - 1, Math.max(0, index + delta))]!.id;
}

class NewProjectState {
  visible = $state(false);
  /** `choose`: the cards. `fill`: the questions for the highlighted card. Esc steps back. */
  step = $state<'choose' | 'fill'>('choose');

  /** The catalog. `null` until asked. Replaced whole, never edited, like the other lists. */
  catalog = $state.raw<TemplateInfo[] | null>(null);
  loadError = $state<string | null>(null);

  category = $state('all');
  query = $state('');
  /** The highlighted card, and in the `fill` step the chosen one. */
  selectedId = $state<string | null>(null);

  /** What has been typed, by field id. Kept when the author steps back and picks another template,
   * because a title typed once is still the title. */
  values = $state<Record<string, string>>({});

  /** True from picking the folder until the project exists, so *Create* cannot be pressed twice. */
  creating = $state(false);
  /** Why the last attempt did not happen, as a sentence for the window. */
  error = $state<string | null>(null);
}

export const newProject = new NewProjectState();
