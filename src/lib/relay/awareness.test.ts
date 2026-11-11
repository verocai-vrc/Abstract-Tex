import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { EncryptedChannel, type MessageSocket } from './channel';
import { RelayProvider } from './provider';
import * as encoding from 'lib0/encoding';
import { RelayAwareness, sanitisePeerUpdate } from './awareness';

/** Two sockets wired directly to each other — the same fan-out shape as the real relay, proven
 * for real in `abstract-tex-relay`'s own tests. */
function fakeSocketPair(): [MessageSocket, MessageSocket] {
  const listenersA: Array<(event: { data: Uint8Array }) => void> = [];
  const listenersB: Array<(event: { data: Uint8Array }) => void> = [];
  const a: MessageSocket = {
    send(data) {
      for (const listener of listenersB) listener({ data });
    },
    addEventListener(_type, listener) {
      listenersA.push(listener);
    },
  };
  const b: MessageSocket = {
    send(data) {
      for (const listener of listenersA) listener({ data });
    },
    addEventListener(_type, listener) {
      listenersB.push(listener);
    },
  };
  return [a, b];
}

function sharedKey(): Promise<CryptoKey> {
  return EncryptedChannel.importKey(crypto.getRandomValues(new Uint8Array(32)));
}

/** Polls rather than asserting straight after a call, for the reason `provider.test.ts`'s own
 * `waitForText` does: every hop across the fake channel is async. */
function waitUntil(check: () => boolean): Promise<void> {
  if (check()) return Promise.resolve();
  return new Promise((resolve) => {
    const interval = setInterval(() => {
      if (!check()) return;
      clearInterval(interval);
      resolve();
    }, 5);
  });
}

describe('RelayAwareness', () => {
  it("a peer's cursor state reaches the other side, without disturbing the document sync sharing the channel", async () => {
    const key = await sharedKey();
    const [socketA, socketB] = fakeSocketPair();

    const docA = new Y.Doc();
    const textA = docA.getText('content');
    textA.insert(0, 'hello');
    const providerA = new RelayProvider(docA, new EncryptedChannel(socketA, key));
    const awarenessA = new RelayAwareness(docA, providerA, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' });

    const docB = new Y.Doc();
    const textB = docB.getText('content');
    const providerB = new RelayProvider(docB, new EncryptedChannel(socketB, key));
    const awarenessB = new RelayAwareness(docB, providerB, { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' });

    await waitUntil(() => textB.toString() === 'hello');

    awarenessA.awareness.setLocalStateField('cursor', { anchor: 2, head: 2 });
    await waitUntil(() => awarenessB.awareness.getStates().get(docA.clientID)?.cursor !== undefined);
    expect(awarenessB.awareness.getStates().get(docA.clientID)).toEqual({
      user: { name: 'Ada', color: '#30bced', colorLight: '#30bced33' },
      cursor: { anchor: 2, head: 2 },
    });

    // The document itself still syncs over the same channel, unaffected by the awareness traffic.
    textB.insert(textB.length, ', world');
    await waitUntil(() => textA.toString() === 'hello, world');

    expect(awarenessA.awareness.getStates().get(docB.clientID)?.user?.name).toBe('Bea');
  });

  it('destroy tells the other peer this client is gone', async () => {
    const key = await sharedKey();
    const [socketA, socketB] = fakeSocketPair();

    const docA = new Y.Doc();
    const providerA = new RelayProvider(docA, new EncryptedChannel(socketA, key));
    const awarenessA = new RelayAwareness(docA, providerA, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' });

    const docB = new Y.Doc();
    const providerB = new RelayProvider(docB, new EncryptedChannel(socketB, key));
    const awarenessB = new RelayAwareness(docB, providerB, { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' });

    await waitUntil(() => awarenessB.awareness.getStates().has(docA.clientID));

    await awarenessA.destroy();
    await waitUntil(() => !awarenessB.awareness.getStates().has(docA.clientID));
  });
});

// Review finding R5.21. A peer's presence is shown by y-codemirror.next, which writes the peer's
// `color` straight into a `style` attribute (`background-color: ${color}`), so a peer who sends
// `red; position: fixed; inset: 0` draws over the whole window. Anyone holding the invite is a
// peer; what they send about themselves has to be checked before it is applied.
describe('a peer\'s presence is checked before it is applied', () => {
  /** What `RelayAwareness` would receive from a peer whose state is `state`, as the wire carries it. */
  function updateFor(clientId: number, state: unknown): Uint8Array {
    const encoder = encoding.createEncoder();
    encoding.writeVarUint(encoder, 1);
    encoding.writeVarUint(encoder, clientId);
    encoding.writeVarUint(encoder, 1); // clock
    encoding.writeVarString(encoder, JSON.stringify(state));
    return encoding.toUint8Array(encoder);
  }

  /** The `user` field of the one client an update describes, read back through y-protocols. */
  function userIn(update: Uint8Array): unknown {
    const doc = new Y.Doc();
    const probe = new RelayAwareness(
      doc,
      { onAwarenessMessage: undefined, sendAwarenessUpdate: async () => {} } as unknown as RelayProvider,
      { name: 'me', color: '#30bced', colorLight: '#30bced33' },
    );
    probe.applyPeerUpdate(update);
    const states = [...probe.awareness.getStates()].filter(([id]) => id !== probe.awareness.clientID);
    return (states[0]?.[1] as { user?: unknown } | undefined)?.user;
  }

  it('lets an ordinary name and colour through unchanged', () => {
    const user = { name: 'Ada', color: '#30bced', colorLight: '#30bced33' };
    expect(userIn(updateFor(7, { user }))).toEqual(user);
  });

  it('replaces a colour that is anything but a hex colour, whatever it tries to do', () => {
    const hostile = {
      name: 'Eve',
      color: 'red; position: fixed; inset: 0; z-index: 99999',
      colorLight: 'url(data:image/svg+xml,<svg/>)',
    };
    expect(userIn(updateFor(7, { user: hostile }))).toEqual({
      name: 'Eve',
      color: '#30bced',
      colorLight: '#30bced33',
    });
  });

  it('shortens a very long name and drops control characters from it', () => {
    const user = userIn(updateFor(7, { user: { name: `Eve\u0000\n${'x'.repeat(500)}`, color: '#fff', colorLight: '#fff' } }));
    const name = (user as { name: string }).name;
    expect(name.length).toBeLessThanOrEqual(40);
    expect(name).not.toMatch(/[\u0000-\u001f]/);
    expect(name.startsWith('Eve')).toBe(true);
  });

  it('keeps what it does not understand out: a user that is not an object is dropped, a state that is not JSON is skipped', () => {
    expect(userIn(updateFor(7, { user: 'red' }))).toBeUndefined();
    const encoder = encoding.createEncoder();
    encoding.writeVarUint(encoder, 1);
    encoding.writeVarUint(encoder, 8);
    encoding.writeVarUint(encoder, 1);
    encoding.writeVarString(encoder, '{ not json');
    expect(userIn(sanitisePeerUpdate(encoding.toUint8Array(encoder)))).toBeUndefined();
  });

  it('passes a departure (a null state) through, so a leaving peer still disappears', () => {
    const doc = new Y.Doc();
    const awareness = new RelayAwareness(
      doc,
      { onAwarenessMessage: undefined, sendAwarenessUpdate: async () => {} } as unknown as RelayProvider,
      { name: 'me', color: '#30bced', colorLight: '#30bced33' },
    );
    awareness.applyPeerUpdate(updateFor(7, { user: { name: 'Ada', color: '#fff', colorLight: '#fff' } }));
    expect([...awareness.awareness.getStates().keys()]).toContain(7);
    const leaving = encoding.createEncoder();
    encoding.writeVarUint(leaving, 1);
    encoding.writeVarUint(leaving, 7);
    encoding.writeVarUint(leaving, 2);
    encoding.writeVarString(leaving, 'null');
    awareness.applyPeerUpdate(encoding.toUint8Array(leaving));
    expect([...awareness.awareness.getStates().keys()]).not.toContain(7);
  });

  it('is what the live channel does: a hostile colour sent by one side is a safe one on the other', async () => {
    const key = await sharedKey();
    const [socketA, socketB] = fakeSocketPair();
    const docA = new Y.Doc();
    const providerA = new RelayProvider(docA, new EncryptedChannel(socketA, key));
    const awarenessA = new RelayAwareness(docA, providerA, { name: 'Eve', color: '#30bced', colorLight: '#30bced33' });
    const docB = new Y.Doc();
    const providerB = new RelayProvider(docB, new EncryptedChannel(socketB, key));
    const awarenessB = new RelayAwareness(docB, providerB, { name: 'Bob', color: '#6eeb83', colorLight: '#6eeb8333' });

    awarenessA.awareness.setLocalStateField('user', { name: 'Eve', color: 'red; inset: 0', colorLight: 'blue; inset: 0' });
    await waitUntil(() => awarenessB.awareness.getStates().has(awarenessA.awareness.clientID));

    const seen = awarenessB.awareness.getStates().get(awarenessA.awareness.clientID) as { user: { color: string } };
    expect(seen.user.color).toBe('#30bced');
  });
});
