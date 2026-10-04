import { describe, expect, it } from 'vitest';
import { EncryptedChannel, type MessageSocket } from './channel';

/** Two sockets wired directly to each other — close enough to the real relay's fan-out (what one
 * sends, the other receives) that this module never needs a real network to test against. */
function fakeSocketPair(): [MessageSocket & { sent: Uint8Array[] }, MessageSocket & { sent: Uint8Array[] }] {
  const listenersA: Array<(event: { data: Uint8Array }) => void> = [];
  const listenersB: Array<(event: { data: Uint8Array }) => void> = [];
  const a = {
    sent: [] as Uint8Array[],
    send(data: Uint8Array) {
      a.sent.push(data);
      for (const listener of listenersB) listener({ data });
    },
    addEventListener(_type: 'message', listener: (event: { data: Uint8Array }) => void) {
      listenersA.push(listener);
    },
  };
  const b = {
    sent: [] as Uint8Array[],
    send(data: Uint8Array) {
      b.sent.push(data);
      for (const listener of listenersA) listener({ data });
    },
    addEventListener(_type: 'message', listener: (event: { data: Uint8Array }) => void) {
      listenersB.push(listener);
    },
  };
  return [a, b];
}

function randomKeyBytes(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

describe('EncryptedChannel', () => {
  it('round-trips a message between two channels sharing a key', async () => {
    const key = await EncryptedChannel.importKey(randomKeyBytes());
    const [socketA, socketB] = fakeSocketPair();
    const a = new EncryptedChannel(socketA, key);
    const b = new EncryptedChannel(socketB, key);

    const received = new Promise<Uint8Array>((resolve) => {
      b.onMessage = resolve;
    });
    await a.send(new TextEncoder().encode('hello from a'));

    expect(new TextDecoder().decode(await received)).toBe('hello from a');
  });

  it('never puts the plaintext on the wire', async () => {
    const key = await EncryptedChannel.importKey(randomKeyBytes());
    const [socketA, socketB] = fakeSocketPair();
    const a = new EncryptedChannel(socketA, key);
    new EncryptedChannel(socketB, key); // holds the other end of the pair open

    const marker = 'a secret only the two peers should ever see, never the relay';
    await a.send(new TextEncoder().encode(marker));

    expect(socketA.sent).toHaveLength(1);
    const [frame] = socketA.sent;
    if (!frame) throw new Error('expected one frame on the wire');
    const wireText = Buffer.from(frame).toString('latin1');
    expect(wireText).not.toContain(marker);
  });

  it('a channel with the wrong key never raises a decrypted-looking message', async () => {
    const [socketA, socketB] = fakeSocketPair();
    const keyA = await EncryptedChannel.importKey(randomKeyBytes());
    const keyB = await EncryptedChannel.importKey(randomKeyBytes());
    const a = new EncryptedChannel(socketA, keyA);
    const b = new EncryptedChannel(socketB, keyB);

    let gotMessage = false;
    b.onMessage = () => {
      gotMessage = true;
    };
    const failure = new Promise<void>((resolve) => {
      b.onDecryptFailure = resolve;
    });

    await a.send(new TextEncoder().encode('hello'));
    await failure;

    expect(gotMessage).toBe(false);
  });

  it('a message too short to hold an IV is a decrypt failure, not a thrown exception', async () => {
    const [socketA, socketB] = fakeSocketPair();
    const key = await EncryptedChannel.importKey(randomKeyBytes());
    new EncryptedChannel(socketB, key);

    const failure = new Promise<void>((resolve) => {
      new EncryptedChannel(socketA, key).onDecryptFailure = resolve;
    });
    socketB.send(new Uint8Array([1, 2, 3]));

    await failure;
  });
});
