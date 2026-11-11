// S14.2c: presence — cursor position, a per-session name and colour — shared with peers over the
// same `RelayProvider` the document text already syncs through, using y-protocols' own
// `Awareness` class (reused, not reimplemented, the same shape S14.2b took with sync).

import * as decoding from 'lib0/decoding';
import * as encoding from 'lib0/encoding';
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

const DEFAULT_COLOR = '#30bced';
const DEFAULT_COLOR_LIGHT = '#30bced33';
const MAX_NAME_CHARACTERS = 40;
/** A hex colour and nothing else: the value is written into a `style` attribute by
 * y-codemirror.next, so anything more permissive lets a peer add CSS of their own. */
const HEX_COLOR = /^#[0-9a-fA-F]{3,8}$/;

/** A peer's name as it may be shown: no control characters, at most [`MAX_NAME_CHARACTERS`]
 * characters, and `Anonymous` when nothing is left. Counted in characters, not UTF-16 units, so an
 * emoji is not cut in half. */
function cleanName(name: unknown): string {
  const text = typeof name === 'string' ? name : '';
  // A control-character range on purpose: this is what is being removed.
  const printable = text.replace(/[\u0000-\u001f\u007f]/g, '').trim();
  const shortened = Array.from(printable).slice(0, MAX_NAME_CHARACTERS).join('');
  return shortened === '' ? 'Anonymous' : shortened;
}

/** The `user` a peer announced, reduced to what this client is willing to draw: a clean name and two
 * hex colours. Anything that is not an object is not a user at all. */
function cleanUser(user: unknown): LocalPresence | null {
  if (typeof user !== 'object' || user === null || Array.isArray(user)) return null;
  const { name, color, colorLight } = user as Record<string, unknown>;
  const safeColor = typeof color === 'string' && HEX_COLOR.test(color) ? color : DEFAULT_COLOR;
  const safeLight = typeof colorLight === 'string' && HEX_COLOR.test(colorLight) ? colorLight : DEFAULT_COLOR_LIGHT;
  return { name: cleanName(name), color: safeColor, colorLight: safeLight };
}

/**
 * A peer's awareness update with every `user` checked (review finding R5.21). Anyone who holds the
 * invite is a peer, and the relay cannot vouch for what a peer says about itself; y-codemirror.next
 * writes a peer's colour straight into CSS, so the check has to happen before the update is applied.
 *
 * The wire format is y-protocols': a count, then for each client its id, its clock and its state as
 * a JSON string. A state that is not JSON is skipped (the rest of the update still applies); a
 * `null` state, which is how a peer says it is leaving, passes through; a state that carries a
 * `user` has that `user` replaced by [`cleanUser`]'s version. An update that cannot be read at all
 * becomes an empty one.
 */
export function sanitisePeerUpdate(update: Uint8Array): Uint8Array {
  const kept: Array<{ clientId: number; clock: number; state: string }> = [];
  try {
    const decoder = decoding.createDecoder(update);
    const count = decoding.readVarUint(decoder);
    for (let index = 0; index < count; index++) {
      const clientId = decoding.readVarUint(decoder);
      const clock = decoding.readVarUint(decoder);
      const stateText = decoding.readVarString(decoder);
      let state: unknown;
      try {
        state = JSON.parse(stateText);
      } catch {
        continue; // not JSON: nothing to apply for this client
      }
      if (typeof state === 'object' && state !== null && !Array.isArray(state) && 'user' in state) {
        const record = state as Record<string, unknown>;
        const user = cleanUser(record.user);
        if (user) record.user = user;
        else delete record.user;
      }
      kept.push({ clientId, clock, state: JSON.stringify(state) });
    }
  } catch {
    // A truncated update: keep what was read before it ran out, apply nothing more.
  }
  const encoder = encoding.createEncoder();
  encoding.writeVarUint(encoder, kept.length);
  for (const entry of kept) {
    encoding.writeVarUint(encoder, entry.clientId);
    encoding.writeVarUint(encoder, entry.clock);
    encoding.writeVarString(encoder, entry.state);
  }
  return encoding.toUint8Array(encoder);
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
    provider.onAwarenessMessage = (update) => this.applyPeerUpdate(update);
    // Listening before this first `setLocalStateField` matters: that call itself fires 'update',
    // and a listener attached after it would miss broadcasting this client's own presence.
    this.awareness.on('update', this.broadcastLocalChange);
    this.awareness.setLocalStateField('user', presence);
  }

  /** A peer's presence, checked ([`sanitisePeerUpdate`]) and then applied. `this` is the origin, which
   * is how `broadcastLocalChange` tells it from a change this client made and does not echo it. */
  applyPeerUpdate(update: Uint8Array): void {
    applyAwarenessUpdate(this.awareness, sanitisePeerUpdate(update), this);
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

  /** Say again that this client is here. After a reconnect (S15.1) the relay has forgotten us and
   * peers may have timed our cursor out; nothing else would make them look again until the next
   * keystroke moved it. */
  announce(): void {
    this.pendingBroadcast = this.provider.sendAwarenessUpdate(
      encodeAwarenessUpdate(this.awareness, [this.awareness.clientID]),
    );
  }

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
