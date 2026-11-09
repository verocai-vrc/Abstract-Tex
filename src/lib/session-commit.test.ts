import { describe, expect, it } from 'vitest';
import { namesInProse, sessionCommitMessage } from './session-commit';

const day = new Date(2026, 10, 9);

describe('namesInProse', () => {
  it('reads like a sentence', () => {
    expect(namesInProse([])).toBe('');
    expect(namesInProse(['Bea'])).toBe('Bea');
    expect(namesInProse(['Bea', 'Cy'])).toBe('Bea and Cy');
    expect(namesInProse(['Ada', 'Bea', 'Cy'])).toBe('Ada, Bea and Cy');
  });
});

describe('sessionCommitMessage', () => {
  it('names the people and the day', () => {
    expect(sessionCommitMessage(['Bea'], day)).toBe('Live session with Bea, 9 Nov');
  });
  it('drops blanks and repeats, and copes with an empty room', () => {
    expect(sessionCommitMessage(['Bea', ' ', 'Bea'], day)).toBe('Live session with Bea, 9 Nov');
    expect(sessionCommitMessage([], day)).toBe('Live session, 9 Nov');
  });
});
