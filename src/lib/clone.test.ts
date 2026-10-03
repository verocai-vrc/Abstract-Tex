import { describe, expect, it } from 'vitest';
import { filterRepositories } from './clone.svelte';
import type { GitHubRepository } from './ipc';

const repository = (fullName: string): GitHubRepository => ({
  fullName,
  cloneUrl: `https://github.com/${fullName}.git`,
  htmlUrl: `https://github.com/${fullName}`,
  private: true,
});

const all = [repository('ada/phd-thesis'), repository('lab/Grant-Proposal'), repository('ada/notes')];

describe('filterRepositories', () => {
  it('keeps everything, in order, for an empty query', () => {
    expect(filterRepositories(all, '')).toEqual(all);
    expect(filterRepositories(all, '   ')).toEqual(all);
  });

  it('matches by substring, ignoring case', () => {
    expect(filterRepositories(all, 'GRANT').map((r) => r.fullName)).toEqual(['lab/Grant-Proposal']);
  });

  it('wants every word, in any order, across owner and name', () => {
    expect(filterRepositories(all, 'thesis ada').map((r) => r.fullName)).toEqual(['ada/phd-thesis']);
    expect(filterRepositories(all, 'ada').map((r) => r.fullName)).toEqual(['ada/phd-thesis', 'ada/notes']);
  });

  it('is not a fuzzy match: letters scattered through a name are not a hit', () => {
    expect(filterRepositories(all, 'ath')).toEqual([]);
  });
});
