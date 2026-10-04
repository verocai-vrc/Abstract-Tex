import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { EncryptedChannel, type MessageSocket } from './channel';
import { RelayProvider } from './provider';

/** Two sockets wired directly to each other — the same fan-out shape as the real relay (what one
 * sends, the other receives), proven for real in `abstract-tex-relay`'s own tests. */
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

/** Every hop across the fake channel is async (the `crypto.subtle` call in `channel.ts`), so a
 * test waits for the doc's `update` event to say the text has actually converged rather than
 * asserting straight after sending. */
function waitForText(doc: Y.Doc, text: Y.Text, expected: string): Promise<void> {
  if (text.toString() === expected) return Promise.resolve();
  return new Promise((resolve) => {
    const check = (): void => {
      if (text.toString() !== expected) return;
      doc.off('update', check);
      resolve();
    };
    doc.on('update', check);
  });
}

describe('RelayProvider', () => {
  it('a joiner catches up to text the other peer already had, then edits reach both ways', async () => {
    const key = await sharedKey();
    const [socketA, socketB] = fakeSocketPair();

    const docA = new Y.Doc();
    docA.getText('content').insert(0, 'hello');
    new RelayProvider(docA, new EncryptedChannel(socketA, key));

    const docB = new Y.Doc();
    const textB = docB.getText('content');
    new RelayProvider(docB, new EncryptedChannel(socketB, key));

    await waitForText(docB, textB, 'hello');

    textB.insert(textB.length, ', world');
    await waitForText(docA, docA.getText('content'), 'hello, world');

    docA.getText('content').insert(0, '>> ');
    await waitForText(docB, textB, '>> hello, world');
  });
});
