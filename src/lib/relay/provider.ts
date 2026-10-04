// S14.2b: binds one Y.Doc to one EncryptedChannel, using the same sync handshake y-websocket and
// y-webrtc already use (`y-protocols/sync`) — reused, not reimplemented. `channel.ts` (S14.2a) is
// the one place that knows about encryption; this module knows only Yjs and the channel's
// already-decrypted bytes.

import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import * as syncProtocol from 'y-protocols/sync';
import type * as Y from 'yjs';
import type { EncryptedChannel } from './channel';

/**
 * On construction, sends sync-step-1 (this doc's state vector) so a peer that already has
 * content can reply with whatever this doc is missing. From then on: a peer's sync-step-1 gets a
 * sync-step-2 reply, a sync-step-2 or update message is applied to the local doc, and every local
 * change not caused by applying one of those incoming messages is broadcast as its own update.
 */
export class RelayProvider {
  constructor(
    private readonly doc: Y.Doc,
    private readonly channel: EncryptedChannel,
  ) {
    channel.onMessage = (data) => this.receive(data);
    doc.on('update', this.broadcastLocalUpdate);
    const encoder = encoding.createEncoder();
    syncProtocol.writeSyncStep1(encoder, this.doc);
    void this.channel.send(encoding.toUint8Array(encoder));
  }

  /** A bound field, not an inline arrow passed to `doc.on`, so a future teardown loop can remove
   * exactly this listener with `doc.off('update', provider.broadcastLocalUpdate)`. */
  private readonly broadcastLocalUpdate = (update: Uint8Array, origin: unknown): void => {
    if (origin === this) return; // this doc's own change, caused by applying a message below
    const encoder = encoding.createEncoder();
    syncProtocol.writeUpdate(encoder, update);
    void this.channel.send(encoding.toUint8Array(encoder));
  };

  private receive(data: Uint8Array): void {
    const decoder = decoding.createDecoder(data);
    const encoder = encoding.createEncoder();
    // `this` as the transaction origin is what lets `broadcastLocalUpdate` tell an update this
    // message caused apart from a change the user actually made — see the check above.
    syncProtocol.readSyncMessage(decoder, encoder, this.doc, this);
    if (encoding.length(encoder) > 0) {
      void this.channel.send(encoding.toUint8Array(encoder));
    }
  }
}
