import { describe, expect, it } from 'vitest';
import { createInvite, inviteToText, parseInvite, plaintextRelayNotice } from './invite';

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

  // Review finding (relay, second pass): an invite with no scheme means `ws://`, and the room id in
  // its path is the only thing that gates a room, in the clear. The text stays private (the key is
  // after the `#` and never leaves the machines), so this is a sentence, not a refusal: the relay
  // binary speaks no TLS, and refusing `ws://` would break the zero-setup way of running one.
  describe('plaintextRelayNotice', () => {
    it('says so for a relay on another machine reached without TLS', () => {
      for (const address of ['relay.example.com:8080', 'ws://relay.example.com', '203.0.113.7:4444', 'ws://192.168.1.20:4444']) {
        const notice = plaintextRelayNotice(address);
        expect(notice, address).not.toBeNull();
        expect(notice).toContain('end to end');
      }
    });

    it('stays quiet for TLS, for this computer, and for nothing typed yet', () => {
      for (const address of ['wss://relay.example.com', 'localhost:4444', 'ws://127.0.0.1:4444', 'ws://[::1]:4444', '', '   ']) {
        expect(plaintextRelayNotice(address), address).toBeNull();
      }
    });

    it('reads the host, not the look of the text: a name that merely starts with localhost is not local', () => {
      expect(plaintextRelayNotice('ws://localhost.example.com:4444')).not.toBeNull();
      expect(plaintextRelayNotice('ws://user@localhost:4444')).toBeNull();
    });
  });
});
