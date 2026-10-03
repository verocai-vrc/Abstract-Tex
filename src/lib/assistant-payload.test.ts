import { describe, expect, it } from 'vitest';
import { carriesContext, carriesDocument, destinationLine, partLabel, prettyBody, sizeLine } from './assistant-payload';
import type { AssistantPayload } from './ipc';

const payload = (over: Partial<AssistantPayload> = {}): AssistantPayload => ({
  destination: 'api.anthropic.com',
  url: 'https://api.anthropic.com/v1/messages',
  model: 'claude-x',
  parts: [
    { role: 'system', text: 'Instructions.', label: null, cached: false },
    { role: 'user', text: '<selection>\nHi\n</selection>', label: null, cached: false },
  ],
  headers: [],
  body: '{"a":1}',
  characters: 1204,
  approximateTokens: 402,
  staysOnThisComputer: false,
  ...over,
});

describe('the payload inspector’s wording', () => {
  it('labels each part by what it is', () => {
    const parts = [
      { role: 'system' as const, text: 'a', label: null, cached: false },
      { role: 'system' as const, text: 'b', label: null, cached: true },
      { role: 'system' as const, text: 'c', label: null, cached: false },
      { role: 'user' as const, text: 'd', label: null, cached: false },
    ];
    expect(parts.map((part, i) => partLabel(part, i, parts))).toEqual([
      'Instructions to the model',
      'Context the provider is asked to remember',
      'More instructions',
      'Your request',
    ]);
  });

  it('calls the user part the build-log lines when the question is about an error', () => {
    const parts = payload().parts;
    expect(partLabel(parts[1]!, 1, parts, 'explain')).toBe('The lines of your build log');
    expect(partLabel(parts[1]!, 1, parts)).toBe('Your request');
  });

  it('says the size as a guess, and never "1 characters"', () => {
    expect(sizeLine(payload())).toBe('1,204 characters (about 402 tokens)');
    expect(sizeLine(payload({ characters: 1, approximateTokens: 1 }))).toBe('1 character (about 1 token)');
  });

  it('only reassures about the network when the destination is this computer', () => {
    expect(destinationLine(payload())).toContain('over the internet');
    expect(destinationLine(payload())).not.toContain('nothing crosses');
    expect(destinationLine(payload({ destination: 'localhost:11434', staysOnThisComputer: true }))).toContain(
      'nothing crosses a network',
    );
  });

  it('lays the body out for reading and leaves a non-JSON body alone', () => {
    expect(prettyBody('{"a":[1,2]}')).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}');
    expect(prettyBody('not json')).toBe('not json');
  });

  it('says there is context beyond the selection exactly when a part is cached', () => {
    expect(carriesContext(payload())).toBe(false);
    expect(carriesContext(payload({ parts: [{ role: 'system', text: 'doc', label: null, cached: true }] }))).toBe(true);
  });

  it('names the manuscript by its label, and says so when the whole document is in the request', () => {
    const document = { role: 'system' as const, text: '<document>…</document>', label: 'Your whole document', cached: true };
    const parts = [{ role: 'system' as const, text: 'i', label: null, cached: false }, document];
    expect(partLabel(document, 1, parts)).toBe('Your whole document');
    expect(carriesDocument(payload({ parts }))).toBe(true);
    expect(carriesDocument(payload())).toBe(false);
  });
});
