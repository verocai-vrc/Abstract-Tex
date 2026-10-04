import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { EncryptedChannel, type MessageSocket } from './channel';
import { RelayProvider } from './provider';
import { RelayAwareness } from './awareness';

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
