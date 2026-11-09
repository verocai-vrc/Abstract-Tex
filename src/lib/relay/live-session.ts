// S14.2c: a live co-editing session for one open document — a relay connection with a
// `RelayProvider` and a `RelayAwareness` layered on top, opened by *Share* or *Join*. Framework-
// agnostic, the same split `document.ts` makes: callbacks carry a change out to whatever is
// reactive (`live.svelte.ts`), rather than this module knowing Svelte exists.

import type * as Y from 'yjs';
import { EncryptedChannel, type MessageSocket } from './channel';
import { RelayProvider } from './provider';
import { RelayAwareness, type LocalPresence } from './awareness';
import type { Invite } from './invite';

/** `reconnecting` (S15.1): the connection dropped and the session is trying to come back. The
 * document is still editable and nothing typed is lost; it reaches peers when the link returns. */
export type LiveStatus = 'connecting' | 'connected' | 'reconnecting' | 'closed' | 'error';

/** How long to wait before each reconnection attempt; the last value repeats. Short at first,
 * because most drops are a network blip, then patient, because a relay that is down should not be
 * hammered by every open window. */
export const RETRY_DELAYS_MS: readonly number[] = [500, 1_000, 2_000, 5_000, 10_000];

/** The eight colours y-codemirror.next's own README suggests for cursors — there is no reason to
 * invent a second palette for the one thing it already ships a fitting default for. */
export const PRESENCE_COLORS: ReadonlyArray<{ color: string; light: string }> = [
  { color: '#30bced', light: '#30bced33' },
  { color: '#6eeb83', light: '#6eeb8333' },
  { color: '#ffbc42', light: '#ffbc4233' },
  { color: '#ecd444', light: '#ecd44433' },
  { color: '#ee6352', light: '#ee635233' },
  { color: '#9ac2c9', light: '#9ac2c933' },
  { color: '#8acb88', light: '#8acb8833' },
  { color: '#1be7ff', light: '#1be7ff33' },
];

export function randomPresenceColor(): { color: string; light: string } {
  const choice = PRESENCE_COLORS[Math.floor(Math.random() * PRESENCE_COLORS.length)];
  return choice ?? PRESENCE_COLORS[0]!;
}

/** Resolves once `socket` reaches `OPEN`, or rejects if it errors first — `connect` cannot
 * construct `RelayProvider`/`RelayAwareness` (which each send immediately) until this settles. */
function waitForOpen(socket: WebSocket): Promise<void> {
  if (socket.readyState === WebSocket.OPEN) return Promise.resolve();
  return new Promise((resolve, reject) => {
    socket.addEventListener('open', () => resolve(), { once: true });
    socket.addEventListener('error', () => reject(new Error('Could not reach the relay.')), { once: true });
  });
}

/** Wraps a real `WebSocket` to the shape `EncryptedChannel` needs. Nothing here reads a
 * message's bytes, only forwards them — there is no decryption boundary to get wrong by hand,
 * `channel.ts` is still the only place that holds a key. */
function socketChannel(socket: WebSocket): MessageSocket {
  socket.binaryType = 'arraybuffer';
  return {
    send: (data) => socket.send(data),
    addEventListener: (type, listener) => socket.addEventListener(type, listener),
  };
}

/**
 * One document's live session. `connect` is the only way to make one — it needs the key import
 * `channel.ts` itself made async, so there is no synchronous constructor to call instead.
 */
export class LiveSession {
  readonly roomId: string;
  readonly awareness: RelayAwareness;

  status: LiveStatus = 'connecting';
  /** Fired on every status change, including the first (`'connecting'` is the constructor's own
   * state, not pushed through this — callers already know they just started one). */
  onStatusChange?: (status: LiveStatus, error: string | null) => void;
  /** Fired whenever a peer's presence appears, changes, or disappears. Always the *current* full
   * list, not a diff — small enough, and a consumer wanting "what changed" would have to
   * reconstruct it from this anyway. */
  onPeersChange?: (peers: LocalPresence[]) => void;

  private socket: WebSocket;
  private readonly provider: RelayProvider;
  private readonly invite: Invite;
  private readonly key: CryptoKey;
  private readonly retryDelaysMs: readonly number[];
  /** Set by `destroy` before anything closes, so the socket's own `close` event is told apart from
   * a connection that dropped under us. */
  private leaving = false;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private attempts = 0;

  // `status` starts `'connected'`: by the time this constructor runs, `connect` has already
  // awaited the socket's own `open` event — see the comment there for why that order is not
  // optional. `RelayProvider`'s and `RelayAwareness`'s constructors each send a message
  // immediately (sync-step-1; this client's own presence), and a real `WebSocket.send` throws if
  // the socket has not yet reached `OPEN`, silently dropping that first message as an unhandled
  // rejection — which is exactly the message the other side needs to ever hear from this one.
  private constructor(
    ydoc: Y.Doc,
    invite: Invite,
    key: CryptoKey,
    presence: LocalPresence,
    channel: EncryptedChannel,
    socket: WebSocket,
    retryDelaysMs: readonly number[],
  ) {
    this.roomId = invite.roomId;
    this.invite = invite;
    this.key = key;
    this.retryDelaysMs = retryDelaysMs;
    this.socket = socket;
    this.provider = new RelayProvider(ydoc, channel);
    this.awareness = new RelayAwareness(ydoc, this.provider, presence);
    this.awareness.awareness.on('change', this.refreshPeers);
    this.watch(socket);
  }

  /** Listen for `socket` closing. Each handler checks the socket is still the current one, so a
   * late event from a connection already replaced cannot disturb the one that replaced it. */
  private watch(socket: WebSocket): void {
    socket.addEventListener('close', (event) => {
      if (socket !== this.socket) return;
      if (this.leaving) {
        this.setStatus('closed', null);
        return;
      }
      // Anything else — the relay restarting, a laptop changing network, the far end closing — is
      // a drop, not an ending: the document is still here and the room is still there to rejoin.
      const closeEvent = event as CloseEvent;
      const why = `Connection closed (${closeEvent.code}${closeEvent.reason ? `: ${closeEvent.reason}` : ''}).`;
      this.setStatus('reconnecting', why);
      this.scheduleReconnect();
    });
  }

  private scheduleReconnect(): void {
    if (this.leaving || this.retryTimer !== null) return;
    const index = Math.min(this.attempts, this.retryDelaysMs.length - 1);
    const delay = this.retryDelaysMs[index] ?? 1_000;
    this.attempts += 1;
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      void this.reconnect();
    }, delay);
  }

  private async reconnect(): Promise<void> {
    if (this.leaving) return;
    const socket = new WebSocket(this.invite.relayUrl);
    this.socket = socket;
    try {
      await waitForOpen(socket);
    } catch {
      // Could not get through: try again later. The failed socket's own `close` is ignored by
      // `watch` below only if it is no longer current, so retire it first.
      if (this.socket === socket) this.scheduleReconnect();
      return;
    }
    if (this.leaving) {
      socket.close();
      return;
    }
    this.watch(socket);
    this.attempts = 0;
    this.provider.attach(new EncryptedChannel(socketChannel(socket), this.key));
    this.awareness.announce();
    this.setStatus('connected', null);
  }

  static async connect(
    ydoc: Y.Doc,
    invite: Invite,
    presence: LocalPresence,
    retryDelaysMs: readonly number[] = RETRY_DELAYS_MS,
  ): Promise<LiveSession> {
    const socket = new WebSocket(invite.relayUrl);
    const [key] = await Promise.all([EncryptedChannel.importKey(invite.key), waitForOpen(socket)]);
    const channel = new EncryptedChannel(socketChannel(socket), key);
    const session = new LiveSession(ydoc, invite, key, presence, channel, socket, retryDelaysMs);
    session.status = 'connected';
    return session;
  }

  private setStatus(status: LiveStatus, error: string | null): void {
    this.status = status;
    this.onStatusChange?.(status, error);
  }

  private readonly refreshPeers = (): void => {
    const peers: LocalPresence[] = [];
    for (const [clientId, state] of this.awareness.awareness.getStates()) {
      if (clientId === this.awareness.awareness.clientID) continue;
      const user = (state as { user?: LocalPresence }).user;
      if (user) peers.push(user);
    }
    this.onPeersChange?.(peers);
  };

  /**
   * Leaves the room and stops syncing. Does not touch the document itself — closing a session
   * never discards anything the document already has, live or not.
   *
   * Awaits `awareness.destroy()` before closing the socket — not a formality. That call
   * broadcasts "this client is gone", and the broadcast finishes asynchronously (it still has to
   * encrypt); closing the socket first would throw the send away the instant it tried to go out,
   * the one message every remaining peer most needs to receive.
   */
  async destroy(): Promise<void> {
    this.leaving = true;
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.awareness.awareness.off('change', this.refreshPeers);
    await this.awareness.destroy();
    this.provider.destroy();
    this.socket.close();
    // A session left while it was reconnecting has no live socket to report its closing.
    if (this.status !== 'closed') this.setStatus('closed', null);
  }
}
