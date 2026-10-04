// S14.2a: the one place in the frontend that holds a live session's decryption key.
//
// DESIGN.md §5.6 and the design interview's F6 settle the shape: an invite carries a relay
// address and a random room secret, and every update crossing the relay is end-to-end encrypted
// with a key derived from that secret — "a relay operator sees only ciphertext". This module is
// that encryption, and nothing else: it knows no Yjs, no rooms, no invite format. `provider.ts`
// (S14.2b) is the thing that actually has something worth encrypting to say.

/** However a WebSocket reaches this module — the real one, or a test double. */
export interface MessageSocket {
  send(data: Uint8Array): void;
  addEventListener(type: 'message', listener: (event: { data: ArrayBuffer | Uint8Array }) => void): void;
}

/** AES-GCM's recommended IV length. One is drawn fresh for every message — reusing an IV under
 * the same key is the one mistake that breaks AES-GCM's guarantees, so nothing here ever does. */
const IV_BYTES = 12;

/** TypeScript's DOM types ask Web Crypto for a `Uint8Array` concretely backed by an
 * `ArrayBuffer` (never a `SharedArrayBuffer`), but a caller holding a view onto someone else's
 * buffer — `TextEncoder.encode`'s own return type is one example — has no reason to know that.
 * Copying is the one line that satisfies both: callers keep accepting any `Uint8Array`, and this
 * module still only ever hands Web Crypto exactly the shape it asks for. */
function toArrayBufferBacked(view: Uint8Array): Uint8Array<ArrayBuffer> {
  return new Uint8Array(view);
}

/**
 * Encrypts everything sent through `socket` and decrypts everything received from it, under one
 * fixed key. The wire format is `iv || ciphertext` — the IV is not a secret, only the key is, so
 * sending it alongside the message costs nothing a relay could use.
 */
export class EncryptedChannel {
  /** Called with the plaintext of every message that decrypted successfully. */
  onMessage?: (data: Uint8Array) => void;
  /** Called when a received message failed to decrypt under this channel's key — a wrong key, or
   * bytes from something that is not one of this session's peers at all. Never calling
   * `onMessage` instead is the point: AES-GCM's authentication tag means decryption either
   * returns exactly what was encrypted or fails outright, so there is no "garbled" message for
   * code downstream to mistake for a real one. */
  onDecryptFailure?: () => void;

  constructor(
    private readonly socket: MessageSocket,
    private readonly key: CryptoKey,
  ) {
    socket.addEventListener('message', (event) => {
      void this.receive(event.data);
    });
  }

  /** Import 32 raw key bytes (derived from the invite's room secret — S14.2c's job) as an
   * AES-GCM key. A separate async step rather than folding into the constructor because
   * `crypto.subtle.importKey` is itself async and a constructor cannot be. */
  static importKey(rawKey: Uint8Array): Promise<CryptoKey> {
    return crypto.subtle.importKey('raw', toArrayBufferBacked(rawKey), 'AES-GCM', false, ['encrypt', 'decrypt']);
  }

  async send(plaintext: Uint8Array): Promise<void> {
    const iv = crypto.getRandomValues(new Uint8Array(IV_BYTES));
    const ciphertext = new Uint8Array(
      await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, this.key, toArrayBufferBacked(plaintext)),
    );
    const framed = new Uint8Array(IV_BYTES + ciphertext.byteLength);
    framed.set(iv, 0);
    framed.set(ciphertext, IV_BYTES);
    this.socket.send(framed);
  }

  private async receive(data: ArrayBuffer | Uint8Array): Promise<void> {
    const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
    if (bytes.byteLength < IV_BYTES) {
      this.onDecryptFailure?.(); // too short to even hold an IV: not a message this ever sent
      return;
    }
    const iv = toArrayBufferBacked(bytes.slice(0, IV_BYTES));
    const ciphertext = toArrayBufferBacked(bytes.slice(IV_BYTES));
    try {
      const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, this.key, ciphertext);
      this.onMessage?.(new Uint8Array(plaintext));
    } catch {
      this.onDecryptFailure?.();
    }
  }
}
