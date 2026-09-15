// The single registry every keyboard-triggerable action is listed in (S4.3). Plain data and
// functions, no Tauri or Svelte import here on purpose: `controller.svelte.ts` registers each
// action once, against the same callback its shortcut in `shortcuts.ts` already dispatches to,
// so the palette and the raw keybinding are two views of one action rather than two independent
// code paths that could drift apart.
//
// This registry only holds *actions* (save, build, open folder, go-to-file). Files and outline
// sections are not registered here: they change with whatever project or file is open, so
// `CommandPalette.svelte` builds a fresh `Command` for each of those at render time and searches
// them together with `searchCommands`'s second argument, rather than this module trying to keep
// a live mirror of `app.project`/`app.outline` in sync.

import { rank } from './fuzzy';

export type CommandCategory = 'file' | 'section' | 'action';

export interface Command {
  id: string;
  title: string;
  category: CommandCategory;
  /** Shown next to the entry so the palette also teaches the raw keybinding. Actions built from
   * `shortcuts.ts`'s table have one; file and section entries do not. */
  shortcut?: string;
  run: () => void;
}

// A `Map` keyed by id rather than an array: registering the same id twice (a hot-reloaded module,
// or a loop that runs its registration code more than once) replaces the old entry instead of
// listing the action twice.
const registry = new Map<string, Command>();

export function registerCommand(cmd: Command): void {
  registry.set(cmd.id, cmd);
}

export function allCommands(): Command[] {
  return [...registry.values()];
}

/**
 * Rank commands by how well `query` matches their title, using the same subsequence matcher as
 * quick-open (`fuzzy.ts`). Defaults to searching the static registry alone; the command palette
 * passes its own combined list (registry plus this file's open files and this file's outline
 * sections) as `commands` so that one ranking pass orders all three categories together.
 */
export function searchCommands(query: string, commands: readonly Command[] = allCommands()): Command[] {
  return rank(query, commands, (cmd) => cmd.title).map((ranked) => ranked.item);
}
