import { describe, expect, it } from 'vitest';
import { createInvite, inviteToText, parseInvite } from './invite';

describe('invite', () => {
  it('round-trips through text with the same relay URL and key', () => {
    const invite = createInvite('relay.example.com:8080');
    const parsed = parseInvite(inviteToText(invite));

    expect(parsed).not.toBeNull();
    expect(parsed?.relayUrl).toBe(invite.relayUrl);
    expect(parsed?.roomId).toBe(invite.roomId);
    expect(parsed?.key).toEqual(invite.key);
  });

  it('defaults to ws:// when the address has no scheme, and keeps wss:// when given one', () => {
    expect(createInvite('relay.example.com:8080').relayUrl).toMatch(/^ws:\/\/relay\.example\.com:8080\//);
    expect(createInvite('wss://relay.example.com').relayUrl).toMatch(/^wss:\/\/relay\.example\.com\//);
  });

  it('two invites for the same address never reuse a room or a key', () => {
    const a = createInvite('relay.example.com:8080');
    const b = createInvite('relay.example.com:8080');

    expect(a.roomId).not.toBe(b.roomId);
    expect(a.key).not.toEqual(b.key);
  });

  it('the key never appears before the "#" — the part a client never sends to the relay', () => {
    const text = inviteToText(createInvite('relay.example.com:8080'));
    const [beforeHash] = text.split('#');

    expect(text).toContain('#');
    expect(beforeHash).not.toContain('#');
  });

  it('rejects text with no "#", a key of the wrong length, or a non-ws(s) scheme', () => {
    expect(parseInvite('not an invite at all')).toBeNull();
    expect(parseInvite('ws://relay.example.com:8080/room#dG9vc2hvcnQ')).toBeNull(); // "tooshort"
    expect(parseInvite(`https://relay.example.com:8080/room#${'A'.repeat(43)}`)).toBeNull();
  });
});
