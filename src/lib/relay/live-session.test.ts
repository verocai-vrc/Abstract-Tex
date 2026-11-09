import { afterEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { createInvite } from './invite';
import { LiveSession } from './live-session';

/**
 * A `WebSocket` that broadcasts to every other instance constructed against the same room —
 * the real fan-out shape, proven for real in `abstract-tex-relay`'s own tests — so this file
 * exercises the whole stack `LiveSession.connect` wires together (invite → socket → encrypted
 * channel → sync + awareness) without a real relay process or real network.
 */
class FakeWebSocket {
  // Real values, not just truthy placeholders: `live-session.ts`'s `waitForOpen` reads the global
  // `WebSocket.OPEN` to compare against `readyState`, and this class stands in for that global
  // under `vi.stubGlobal` below.
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
  static created = 0;

  static rooms = new Map<string, FakeWebSocket[]>();
  binaryType = '';
  readyState = FakeWebSocket.CONNECTING;
  private readonly room: string;
  private readonly listeners = new Map<string, Array<(event: { data?: Uint8Array; code?: number }) => void>>();

  constructor(url: string) {
    FakeWebSocket.created += 1;
    this.room = new URL(url).pathname.slice(1);
    const peers = FakeWebSocket.rooms.get(this.room) ?? [];
    peers.push(this);
    FakeWebSocket.rooms.set(this.room, peers);
    // A macrotask, not a microtask: `LiveSession.connect` still has an `await` (importing the
    // room key) and a constructor call to run after this before it registers its own 'open'
    // listener, and a microtask here would fire before any of that — unlike a real WebSocket's
    // handshake, which never resolves that fast regardless.
    setTimeout(() => {
      this.readyState = FakeWebSocket.OPEN;
      this.dispatch('open', {});
    }, 0);
  }

  addEventListener(type: string, listener: (event: { data?: Uint8Array; code?: number }) => void): void {
    const forType = this.listeners.get(type) ?? [];
    forType.push(listener);
    this.listeners.set(type, forType);
  }

  /** A real `WebSocket` throws `InvalidStateError` for a send before `OPEN` — the exact
   * production bug this fixture exists to catch (S14.2c, see `bugs-issues-fixes.md`): a message
   * sent this early does not queue, it is simply gone. */
  send(data: Uint8Array): void {
    if (this.readyState !== FakeWebSocket.OPEN) {
      throw new DOMException('WebSocket is not open', 'InvalidStateError');
    }
    for (const peer of FakeWebSocket.rooms.get(this.room) ?? []) {
      if (peer !== this) peer.dispatch('message', { data });
    }
  }

  /** The network or the relay ending the connection, as opposed to `close()`, the page doing it:
   * code 1006, "abnormal closure", which is what a browser reports when the link just dies. */
  dropFromServer(): void {
    this.readyState = FakeWebSocket.CLOSED;
    const peers = FakeWebSocket.rooms.get(this.room) ?? [];
    FakeWebSocket.rooms.set(this.room, peers.filter((peer) => peer !== this));
    this.dispatch('close', { code: 1006 });
  }

  close(): void {
    this.readyState = FakeWebSocket.CLOSED;
    const peers = FakeWebSocket.rooms.get(this.room) ?? [];
    FakeWebSocket.rooms.set(this.room, peers.filter((peer) => peer !== this));
    this.dispatch('close', {});
  }

  private dispatch(type: string, event: { data?: Uint8Array; code?: number }): void {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
}

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

describe('LiveSession', () => {
  afterEach(() => {
    FakeWebSocket.rooms.clear();
    FakeWebSocket.created = 0;
    vi.unstubAllGlobals();
  });

  it('two sessions on the same invite converge, see each other in awareness, and tear down cleanly', async () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const invite = createInvite('fake-relay.example:1234');

    const docA = new Y.Doc();
    docA.getText('content').insert(0, 'hello');
    const sessionA = await LiveSession.connect(docA, invite, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' });
    // Attached before B ever joins: B's arrival is the only 'change' event this callback needs
    // to see, and a listener attached afterward would simply never fire again.
    let peersSeenByA: string[] = [];
    sessionA.onPeersChange = (peers) => (peersSeenByA = peers.map((peer) => peer.name));

    const docB = new Y.Doc();
    const textB = docB.getText('content');
    const sessionB = await LiveSession.connect(docB, invite, { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' });

    await waitUntil(() => sessionA.status === 'connected' && sessionB.status === 'connected');
    await waitUntil(() => textB.toString() === 'hello');
    await waitUntil(() => peersSeenByA.includes('Bea'));

    await sessionB.destroy();
    await waitUntil(() => sessionB.status === 'closed');
    await waitUntil(() => !peersSeenByA.includes('Bea'));

    await sessionA.destroy();
  });

  it('a joiner who cleared independently-loaded content before connecting can still edit, and the sharer sees it', async () => {
    // S14.2c found this the hard way, and only under a *room* (this harness), never a pre-paired
    // socket pair: `joinSession` clears the joiner's buffer before connecting, because two docs
    // that loaded the same file independently share no CRDT history (syncing two non-empty
    // buffers concatenates instead of converging — see `joinSession` in controller.svelte.ts).
    // Clearing leaves a tombstoned item in the joiner's own history. The sharer's own sync-step-1,
    // broadcast the moment they shared, reached nobody (the room was empty — exactly what this
    // harness, unlike a pre-paired socket, actually models) and was never resent, so the sharer
    // never asked the joiner what *they* had — including that tombstone's identity. Any edit the
    // joiner later made that anchored to it, even only structurally, was a dependency the sharer
    // could never resolve and silently never integrated, with no error anywhere. The fix was
    // `RelayProvider.receive` sending its own sync-step-1 right after replying to one, the "server
    // replies with SyncStep2 immediately followed by SyncStep1" handshake `y-protocols/sync.js`'s
    // own module doc already asked for.
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const invite = createInvite('fake-relay.example:1234');

    const docA = new Y.Doc();
    const textA = docA.getText('content');
    textA.insert(0, 'shared starting text');
    const sessionA = await LiveSession.connect(docA, invite, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' });

    const docB = new Y.Doc();
    const textB = docB.getText('content');
    textB.insert(0, 'shared starting text'); // loaded independently, same bytes, no shared history
    textB.delete(0, textB.length); // the join-clear
    const sessionB = await LiveSession.connect(docB, invite, { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' });

    await waitUntil(() => sessionA.status === 'connected' && sessionB.status === 'connected');
    await waitUntil(() => textB.toString() === 'shared starting text');

    textB.insert(textB.length, ' — edited by the joiner');
    await waitUntil(() => textA.toString() === 'shared starting text — edited by the joiner');
  });

  it('a session on a room nobody else has joined connects and stays open with no error', async () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const invite = createInvite('fake-relay.example:1234');
    const doc = new Y.Doc();

    const session = await LiveSession.connect(doc, invite, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' });
    await waitUntil(() => session.status === 'connected');

    await session.destroy();
  });

  const ada = { name: 'Ada', color: '#30bced', colorLight: '#30bced33' };
  const bea = { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' };

  /** The socket a session is currently using, found through the room it joined. */
  function socketsInRoom(): FakeWebSocket[] {
    return [...FakeWebSocket.rooms.values()].flat();
  }

  it('after a drop, edits made on both sides while apart are all there once the link returns (S15.1)', async () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const invite = createInvite('fake-relay.example:1234');

    const docA = new Y.Doc();
    const textA = docA.getText('content');
    textA.insert(0, 'The paragraph. ');
    const sessionA = await LiveSession.connect(docA, invite, ada, [5]);
    const docB = new Y.Doc();
    const textB = docB.getText('content');
    const sessionB = await LiveSession.connect(docB, invite, bea, [5]);
    await waitUntil(() => textB.toString() === 'The paragraph. ');

    const statusesOfB: string[] = [];
    sessionB.onStatusChange = (status) => statusesOfB.push(status);

    // Bea's connection dies. Ada does not hear of it.
    const beasSocket = socketsInRoom().find((socket) => socket !== socketsInRoom()[0])!;
    beasSocket.dropFromServer();
    await waitUntil(() => sessionB.status === 'reconnecting');

    // Both go on writing, to the same paragraph, not able to hear each other.
    textA.insert(0, 'Ada writes first. ');
    textB.insert(textB.length, 'Bea writes last.');
    expect(textA.toString()).not.toContain('Bea writes last.');

    await waitUntil(() => sessionB.status === 'connected');
    await waitUntil(() => textA.toString() === textB.toString());
    expect(textA.toString()).toContain('Ada writes first.');
    expect(textA.toString()).toContain('Bea writes last.');
    expect(textA.toString()).toContain('The paragraph.');
    expect(statusesOfB).toEqual(['reconnecting', 'connected']);

    await sessionA.destroy();
    await sessionB.destroy();
  });

  it('keeps trying while the relay is unreachable, and stops for good once the session is left', async () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const invite = createInvite('fake-relay.example:1234');
    const session = await LiveSession.connect(new Y.Doc(), invite, ada, [5]);

    socketsInRoom()[0]!.dropFromServer();
    await waitUntil(() => FakeWebSocket.created >= 2);
    await session.destroy();
    const createdWhenLeft = FakeWebSocket.created;
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(FakeWebSocket.created).toBe(createdWhenLeft);
    expect(session.status).toBe('closed');
  });
});
