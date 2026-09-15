import { beforeEach, describe, expect, it, vi } from 'vitest';
import { allCommands, registerCommand, searchCommands, type Command } from './commands';

// The registry is a module-level singleton, so each test registers under ids no other test uses
// rather than trying to reset it between tests.
function command(id: string, title: string): Command {
  return { id, title, category: 'action', run: vi.fn() };
}

describe('command registry', () => {
  it('lists every registered command', () => {
    registerCommand(command('list-a', 'List A'));
    registerCommand(command('list-b', 'List B'));
    const ids = allCommands().map((c) => c.id);
    expect(ids).toContain('list-a');
    expect(ids).toContain('list-b');
  });

  it('registering the same id twice replaces the entry rather than duplicating it', () => {
    registerCommand(command('dup', 'First title'));
    registerCommand(command('dup', 'Second title'));
    const matches = allCommands().filter((c) => c.id === 'dup');
    expect(matches).toHaveLength(1);
    expect(matches[0]!.title).toBe('Second title');
  });
});

describe('searchCommands', () => {
  const build = command('search-build', 'Build');
  const save = command('search-save', 'Save');
  const openFolder = command('search-open-folder', 'Open folder…');

  beforeEach(() => {
    registerCommand(build);
    registerCommand(save);
    registerCommand(openFolder);
  });

  it('finds a command by a subsequence of its title', () => {
    const results = searchCommands('bld', allCommands());
    expect(results.map((c) => c.id)).toContain('search-build');
  });

  it('an empty query returns every command in registration order', () => {
    const results = searchCommands('', [build, save, openFolder]);
    expect(results).toEqual([build, save, openFolder]);
  });

  it('drops commands that do not match', () => {
    const results = searchCommands('zzz', [build, save, openFolder]);
    expect(results).toEqual([]);
  });

  it('searches whatever list is passed instead of only the static registry', () => {
    const dynamic = command('dynamic-section', 'Go to: Introduction');
    const results = searchCommands('intro', [build, dynamic]);
    expect(results.map((c) => c.id)).toEqual(['dynamic-section']);
  });

  it('running a command invokes its callback', () => {
    build.run();
    expect(build.run).toHaveBeenCalledOnce();
  });
});
