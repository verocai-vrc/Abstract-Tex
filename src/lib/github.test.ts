// The GitHub sign-in store (S10.4b): the three stages an event can move it to, and the one
// number the panel shows while a code counts down. No Svelte runtime is needed for `timeLeft`;
// `applySignInEvent` writes to the store, which is exercised through the controller as well.

import { beforeEach, describe, expect, it } from 'vitest';
import type { SignInEvent } from './ipc';
import { applySignInEvent, github, suggestedRepositoryName, timeLeft } from './github.svelte';

const now = Date.UTC(2026, 8, 29, 12, 0, 0);

beforeEach(() => {
  github.account = undefined;
  github.stage = 'idle';
  github.code = null;
  github.error = null;
});

describe('a code on screen', () => {
  const codeEvent: SignInEvent = {
    stage: 'code',
    userCode: 'WDJB-MJHT',
    verificationUri: 'https://github.com/login/device',
    expiresInSeconds: 900,
  };

  it('puts the code up with an absolute expiry, so the countdown cannot drift', () => {
    applySignInEvent(codeEvent, now);
    expect(github.stage).toBe('waiting');
    expect(github.code).toEqual({
      userCode: 'WDJB-MJHT',
      verificationUri: 'https://github.com/login/device',
      expiresAt: now + 900_000,
    });
  });

  it('counts down in the units a person waiting would use', () => {
    expect(timeLeft(now + 900_000, now)).toBe('15 min left');
    expect(timeLeft(now + 61_000, now)).toBe('2 min left');
    expect(timeLeft(now + 45_000, now)).toBe('45s left');
    expect(timeLeft(now - 5_000, now)).toBe('expired');
  });
});

describe('the end of a sign-in', () => {
  it('signed in: the account arrives and the code goes away', () => {
    applySignInEvent({ stage: 'code', userCode: 'A', verificationUri: 'B', expiresInSeconds: 900 }, now);
    applySignInEvent({ stage: 'signedIn', login: 'ada' }, now);
    expect(github.account).toEqual({ login: 'ada' });
    expect(github.code).toBeNull();
    expect(github.stage).toBe('idle');
    expect(github.error).toBeNull();
  });

  it('failed: the sentence is shown and the panel goes back to offering', () => {
    applySignInEvent({ stage: 'failed', message: 'The sign-in code expired.', cancelled: false }, now);
    expect(github.error).toBe('The sign-in code expired.');
    expect(github.stage).toBe('idle');
    expect(github.code).toBeNull();
  });

  it('cancelled: nothing is shown, because a decision is not a failure', () => {
    applySignInEvent({ stage: 'code', userCode: 'A', verificationUri: 'B', expiresInSeconds: 900 }, now);
    applySignInEvent({ stage: 'failed', message: 'The sign-in was cancelled.', cancelled: true }, now);
    expect(github.error).toBeNull();
    expect(github.code).toBeNull();
    expect(github.stage).toBe('idle');
  });

  it('a later code replaces an earlier one rather than showing two', () => {
    applySignInEvent({ stage: 'code', userCode: 'FIRST', verificationUri: 'B', expiresInSeconds: 900 }, now);
    applySignInEvent({ stage: 'code', userCode: 'SECOND', verificationUri: 'B', expiresInSeconds: 900 }, now);
    expect(github.code?.userCode).toBe('SECOND');
  });
});

describe('a name GitHub will take (S10.5b)', () => {
  it('leaves a name that is already fine alone', () => {
    expect(suggestedRepositoryName('thesis')).toBe('thesis');
    expect(suggestedRepositoryName('phd-thesis_2026.v2')).toBe('phd-thesis_2026.v2');
  });

  it('turns what GitHub would rename into what it will be called', () => {
    // GitHub does this substitution itself and says nothing, so the field shows the real name.
    expect(suggestedRepositoryName('My Thesis (final)')).toBe('My-Thesis-final');
    expect(suggestedRepositoryName('  spaces  everywhere  ')).toBe('spaces-everywhere');
    expect(suggestedRepositoryName('a//b')).toBe('a-b');
  });

  it('never offers a name GitHub would reject outright', () => {
    expect(suggestedRepositoryName('')).toBe('paper');
    expect(suggestedRepositoryName('!!!')).toBe('paper');
  });
});
