import { describe, expect, it } from 'vitest';
import { compileAnnouncement } from './announce';

describe('compileAnnouncement', () => {
  it('says each stage once, in words', () => {
    expect(compileAnnouncement('idle', 0, 0)).toBe('');
    expect(compileAnnouncement('running', 0, 0)).toBe('Compiling');
    expect(compileAnnouncement('ok', 700, 0)).toBe('Build finished in 0.7 seconds');
    expect(compileAnnouncement('error', 0, 1)).toBe('Build failed with 1 error');
    expect(compileAnnouncement('error', 0, 3)).toBe('Build failed with 3 errors');
    expect(compileAnnouncement('failed', 0, 0)).toBe('Could not build');
  });
});
