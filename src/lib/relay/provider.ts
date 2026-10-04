// S14.2b: binds one Y.Doc to one EncryptedChannel, using the same sync handshake y-websocket and
// y-webrtc already use (`y-protocols/sync`) — reused, not reimplemented. `channel.ts` (S14.2a) is
// the one place that knows about encryption; this module knows only Yjs and the channel's
// already-decrypted bytes.

import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import * as syncProtocol from 'y-protocols/sync';
import type * as Y from 'yjs';
import type { EncryptedChannel } from './channel';

/** The channel carries two message kinds now that S14.2c adds awareness beside sync — cursor
 * position and a name/colour, broadcast over this same connection rather than a second one. A
 * leading tag byte, written and stripped here, is what keeps the two apart; `y-protocols/sync`'s
 * own messages keep their own inner type byte untouched inside the `MESSAGE_SYNC` payload. */
const MESSAGE_SYNC = 0;
const MESSAGE_AWARENESS = 1;

/**
 * On construction, sends sync-step-1 (this doc's state vector) so a peer that already has
 * content can reply with whatever this doc is missing. From then on: a peer's sync-step-1 gets a
 * sync-step-2 reply, a sync-step-2 or update message is applied to the local doc, and every local
 * change not caused by applying one of those incoming messages is broadcast as its own update.
 */
export class RelayProvider {
  /** Set by `RelayAwareness` (S14.2c) to receive this channel's awareness traffic.
   * `RelayProvider` itself knows nothing about cursors or presence — only that message kind 1
   * belongs to whichever other object has claimed this callback. */
  onAwarenessMessage?: (update: Uint8Array) => void;

  constructor(
    private readonly doc: Y.Doc,
    private readonly channel: EncryptedChannel,
  ) {
    channel.onMessage = (data) => this.receive(data);
    doc.on('update', this.broadcastLocalUpdate);
    const syncEncoder = encoding.createEncoder();
    syncProtocol.writeSyncStep1(syncEncoder, this.doc);
    void this.sendEnvelope(MESSAGE_SYNC, encoding.toUint8Array(syncEncoder));
  }

  /** `RelayAwareness` calls this; `RelayProvider` never reads `update`'s contents, only carries
   * it inside the envelope the other message kind, sync, also uses. Returns the send's own
   * promise so `RelayAwareness.destroy` can await its last, departing broadcast before the
   * channel underneath it closes — see that method's own comment for why that order matters. */
  sendAwarenessUpdate(update: Uint8Array): Promise<void> {
    return this.sendEnvelope(MESSAGE_AWARENESS, update);
  }

  /** Removes the `update` listener this constructor added. Does not close `channel` — whoever
   * opened the socket underneath it owns that; this only stops this doc from broadcasting once
   * the session it belongs to is torn down. */
  destroy(): void {
    this.doc.off('update', this.broadcastLocalUpdate);
  }

  /** A bound field, not an inline arrow passed to `doc.on`, so `destroy` above can remove exactly
   * this listener with `doc.off('update', ...)`. */
  private readonly broadcastLocalUpdate = (update: Uint8Array, origin: unknown): void => {
    if (origin === this) return; // this doc's own change, caused by applying a message below
    const syncEncoder = encoding.createEncoder();
    syncProtocol.writeUpdate(syncEncoder, update);
    void this.sendEnvelope(MESSAGE_SYNC, encoding.toUint8Array(syncEncoder));
  };

  private receive(data: Uint8Array): void {
    const decoder = decoding.createDecoder(data);
    const kind = decoding.readVarUint(decoder);
    const payload = decoding.readVarUint8Array(decoder);
    if (kind === MESSAGE_AWARENESS) {
      this.onAwarenessMessage?.(payload);
      return;
    }
    const syncDecoder = decoding.createDecoder(payload);
    const reply = encoding.createEncoder();
    // `this` as the transaction origin is what lets `broadcastLocalUpdate` tell an update this
    // message caused apart from a change the user actually made — see the check above.
    syncProtocol.readSyncMessage(syncDecoder, reply, this.doc, this);
    if (encoding.length(reply) > 0) {
      // `reply` is non-empty only when the incoming message was a sync-step-1 (the only sync
      // message y-protocols answers automatically) — this is that case, and per sync.js's own
      // documented client-server handshake, the responder replies with sync-step-2 "immediately
      // followed by SyncStep1": sending only the step-2 half (as this did before) leaves the
      // *other* direction of the exchange never requested. A joiner's sync-step-1 prompts this
      // side's content to reach them; without this side's own follow-up sync-step-1, this side
      // never learns what to ask the joiner for in return. Concretely, that meant a joined
      // document's own prior state — even content already deleted but still structurally present,
      // like the clear in `joinSession` leaves behind — stayed a dependency the sharer's side had
      // never been told to fetch, so any edit built on it (anchored to it as a CRDT position, even
      // invisibly) silently stalled as a permanently unresolved reference on their side.
      void this.sendEnvelope(MESSAGE_SYNC, encoding.toUint8Array(reply));
      const ownSyncStep1 = encoding.createEncoder();
      syncProtocol.writeSyncStep1(ownSyncStep1, this.doc);
      void this.sendEnvelope(MESSAGE_SYNC, encoding.toUint8Array(ownSyncStep1));
    }
  }

  /** Never rejects. A live session's channel can close at any moment a message happens to be
   * mid-flight — the far end hung up, the page is closing — and most callers here fire sends
   * without waiting on them; an unhandled rejection from one of those is not a bug to surface,
   * it is a message that no longer has anywhere to go. `RelayAwareness.destroy` is the one
   * caller that *does* await this, to sequence its goodbye broadcast before the socket closes —
   * it needs to know the send was attempted, never whether it landed. */
  private async sendEnvelope(kind: number, payload: Uint8Array): Promise<void> {
    const encoder = encoding.createEncoder();
    encoding.writeVarUint(encoder, kind);
    encoding.writeVarUint8Array(encoder, payload);
    try {
      await this.channel.send(encoding.toUint8Array(encoder));
    } catch {
      // The channel closed before this reached the wire. Nothing to retry: the next sync-step-1
      // or update this doc sends, if the channel reopens, carries this doc's current state anyway.
    }
  }
}
