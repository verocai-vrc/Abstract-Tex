import { describe, expect, it } from 'vitest';
import {
  assistantReady,
  blankForm,
  describeStatus,
  formFromProvider,
  keyIsOptional,
  providerFromForm,
} from './assistant';
import type { AssistantStatus } from './ipc';

const anthropic = { kind: 'anthropic', model: 'claude-x', address: 'https://api.anthropic.com' } as const;
const local = { kind: 'openAiCompatible', model: 'llama3', address: 'http://localhost:11434/v1' } as const;

const status = (over: Partial<AssistantStatus> = {}): AssistantStatus => ({
  provider: anthropic,
  hasKey: true,
  enabled: true,
  inspectFirst: true,
  ...over,
});

describe('providerFromForm', () => {
  it('builds an Anthropic provider at Anthropic’s own address whatever the address field says', () => {
    expect(providerFromForm({ kind: 'anthropic', model: ' claude-x ', address: 'https://elsewhere.example' })).toEqual(
      anthropic,
    );
  });

  it('builds an OpenAI-compatible provider from the address and model, trimmed', () => {
    expect(providerFromForm({ kind: 'openAiCompatible', model: ' llama3', address: ' http://localhost:11434/v1 ' })).toEqual(
      local,
    );
  });

  it('says what is missing instead of building half a provider', () => {
    expect(providerFromForm({ kind: 'anthropic', model: '  ', address: '' })).toMatch(/Choose a model/);
    expect(providerFromForm({ kind: 'openAiCompatible', model: 'm', address: ' ' })).toMatch(/address/);
  });
});

describe('the form', () => {
  it('starts blank for each kind with a sensible address, and a suggested model only for Anthropic', () => {
    expect(blankForm('anthropic').address).toBe('https://api.anthropic.com');
    expect(blankForm('anthropic').model).not.toBe('');
    expect(blankForm('openAiCompatible')).toEqual({ kind: 'openAiCompatible', model: '', address: 'http://localhost:11434/v1' });
  });

  it('is filled from the saved provider, and falls back to Anthropic’s blank form', () => {
    expect(formFromProvider(local)).toEqual({ kind: 'openAiCompatible', model: 'llama3', address: 'http://localhost:11434/v1' });
    expect(formFromProvider(null).kind).toBe('anthropic');
  });

  it('knows only a model on the computer may go without a key', () => {
    expect(keyIsOptional('anthropic')).toBe(false);
    expect(keyIsOptional('openAiCompatible')).toBe(true);
  });
});

describe('describeStatus', () => {
  it('says each state in a sentence, and never "0" of anything', () => {
    expect(describeStatus(null, true)).toMatch(/Not set up/);
    expect(describeStatus(status({ provider: null }), true)).toMatch(/Not set up/);
    expect(describeStatus(status({ hasKey: false }), true)).toMatch(/Add an API key/);
    expect(describeStatus(status(), false)).toMatch(/Open a project/);
    expect(describeStatus(status({ enabled: false }), true)).toMatch(/off for this project/);
    expect(describeStatus(status(), true)).toMatch(/Nothing is sent until you ask/);
  });

  it('does not ask for a key a local model does not use', () => {
    expect(describeStatus(status({ provider: local, hasKey: false, enabled: false }), true)).toMatch(/off for this project/);
  });
});

describe('assistantReady', () => {
  it('is false until a provider is chosen, a key is there if one is needed, and the project is on', () => {
    expect(assistantReady(null)).toBe(false);
    expect(assistantReady(status({ provider: null }))).toBe(false);
    expect(assistantReady(status({ hasKey: false }))).toBe(false);
    expect(assistantReady(status({ enabled: false }))).toBe(false);
    expect(assistantReady(status())).toBe(true);
    expect(assistantReady(status({ provider: local, hasKey: false }))).toBe(true);
  });
});
