// S14.2c: presence — cursor position, a per-session name and colour — shared with peers over the
// same `RelayProvider` the document text already syncs through, using y-protocols' own
// `Awareness` class (reused, not reimplemented, the same shape S14.2b took with sync).

import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate, removeAwarenessStates } from 'y-protocols/awareness';
import type * as Y from 'yjs';
import type { RelayProvider } from './provider';

/** y-codemirror.next's `yCollab` reads exactly this shape off `awareness.getLocalState().user` to
 * draw a remote cursor and its label. */
export interface LocalPresence {
  name: string;
  color: string;
  colorLight: string;
}

/**
 * One `Awareness` per open document's `RelayProvider`. Local state set with
 * `setLocalStateField`/`setLocalState` (`yCollab` itself calls this for cursor position; `name`
 * is set once, below) is broadcast to peers; a peer's broadcast is applied to this `Awareness` so
 * `yCollab` can render it.
 */
export class RelayAwareness {
  readonly awareness: Awareness;

  /** The most recent broadcast's own promise — tracked only so `destroy` can await the specific
   * one it triggers (the "I'm leaving" message) before anything closes the channel underneath
   * it. `broadcastLocalChange` fires synchronously inside `removeAwarenessStates` below, so there
   * is no other way to get a handle on that particular send. */
  private pendingBroadcast: Promise<void> = Promise.resolve();

  constructor(
    doc: Y.Doc,
    private readonly provider: RelayProvider,
    presence: LocalPresence,
  ) {
    this.awareness = new Awareness(doc);
    provider.onAwarenessMessage = (update) => applyAwarenessUpdate(this.awareness, update, this);
    // Listening before this first `setLocalStateField` matters: that call itself fires 'update',
    // and a listener attached after it would miss broadcasting this client's own presence.
    this.awareness.on('update', this.broadcastLocalChange);
    this.awareness.setLocalStateField('user', presence);
  }

  /** A bound field for the same reason `RelayProvider.broadcastLocalUpdate` is one: `destroy`
   * below needs a stable reference to remove with `awareness.off('update', ...)`. */
  private readonly broadcastLocalChange = (
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ): void => {
    if (origin === this) return; // this client's own state, written by applying a peer's message
    const changedClients = added.concat(updated, removed);
    this.pendingBroadcast = this.provider.sendAwarenessUpdate(encodeAwarenessUpdate(this.awareness, changedClients));
  };

  /**
   * Tells peers this client is gone, rather than leaving a stale cursor on screen until
   * y-protocols' own 30-second timeout clears it. Called when the session this belongs to ends —
   * the editor for this document closing, or *Leave* — not on every tab switch.
   *
   * Async, and awaited by `LiveSession.destroy`, for a real reason: `removeAwarenessStates` below
   * broadcasts synchronously, but the broadcast itself — like every send on this channel — finishes
   * asynchronously (it still has to encrypt). A `WebSocket` throws if `send` is called after
   * `close`, so closing the socket before this settles would silently drop the one message every
   * peer most needs to receive, the same way an unopened socket silently dropped this client's
   * first presence announcement before S14.2c's `connect` learned to wait for `open` first.
   */
  async destroy(): Promise<void> {
    removeAwarenessStates(this.awareness, [this.awareness.clientID], 'destroy');
    await this.pendingBroadcast.catch(() => {}); // best-effort: a goodbye that fails to send changes nothing a 30-second timeout would not already have fixed
    this.awareness.off('update', this.broadcastLocalChange);
    this.awareness.destroy();
  }
}
