// S15.3, the v0.8 exit demo (DESIGN.md §7): "Two authors edit one paragraph simultaneously across
// a network; one goes offline, keeps writing, reconnects, and loses nothing."
//
// Everything real except the clock: the actual `abstract-tex-relay` binary, real TCP sockets, real
// WebSockets and AES-GCM, the real `LiveSession`. The one stand-in is the network cable — Bea's
// traffic goes through a TCP proxy this test can cut and restore. The offline stretch is seconds,
// not ten minutes: nothing in the mechanism depends on its length (there is no timer, queue or
// buffer that could run out), so a longer wait would only make the test slower.
//
// Skipped when the relay binary is not built (`cargo build -p abstract-tex-relay`), or point
// ABSTRACT_TEX_RELAY_BIN at one. `pnpm verify` therefore does not depend on it; the loop that last
// touched live sessions ran it by hand.

import { spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { LiveSession } from './live-session';
import { createInvite, type Invite } from './invite';

const relayBinary = process.env.ABSTRACT_TEX_RELAY_BIN ?? path.resolve('target/debug/abstract-tex-relay');

async function freePort(): Promise<number> {
  return new Promise((resolve) => {
    const probe = net.createServer().listen(0, '127.0.0.1', () => {
      const { port } = probe.address() as net.AddressInfo;
      probe.close(() => resolve(port));
    });
  });
}

async function waitUntil(check: () => boolean, what: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!check()) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

/** A TCP pipe to the relay that can be cut and restored: Bea's cable. */
class Cable {
  private readonly sockets = new Set<net.Socket>();
  private cut = false;
  private readonly server: net.Server;

  constructor(readonly port: number, toPort: number) {
    this.server = net.createServer((client) => {
      if (this.cut) return void client.destroy(); // an unplugged cable: nothing connects
      const upstream = net.connect(toPort, '127.0.0.1');
      for (const socket of [client, upstream]) {
        this.sockets.add(socket);
        socket.on('close', () => this.sockets.delete(socket));
        socket.on('error', () => {});
      }
      client.pipe(upstream);
      upstream.pipe(client);
    });
  }

  listen(): Promise<void> {
    return new Promise((resolve) => this.server.listen(this.port, '127.0.0.1', resolve));
  }

  unplug(): void {
    this.cut = true;
    for (const socket of this.sockets) socket.destroy();
  }

  plugIn(): void {
    this.cut = false;
  }

  close(): void {
    this.unplug();
    this.server.close();
  }
}

describe.skipIf(!existsSync(relayBinary))('v0.8 exit demo, against the real relay', () => {
  let relay: ChildProcess | null = null;
  afterAll(() => {
    relay?.kill();
  });

  it('two authors write one paragraph; one is cut off, keeps writing, reconnects, and nothing is lost', async () => {
    const relayPort = await freePort();
    relay = spawn(relayBinary, [], {
      env: { ...process.env, ABSTRACT_TEX_RELAY_ADDR: `127.0.0.1:${relayPort}` },
      stdio: 'ignore',
    });
    await waitUntil(() => relay!.exitCode === null, 'the relay to start');
    await new Promise((resolve) => setTimeout(resolve, 300));

    const cable = new Cable(await freePort(), relayPort);
    await cable.listen();

    // One room, one key; Ada reaches the relay directly, Bea through the cable.
    const adasInvite: Invite = createInvite(`127.0.0.1:${relayPort}`);
    const beasInvite: Invite = { ...adasInvite, relayUrl: `ws://127.0.0.1:${cable.port}/${adasInvite.roomId}` };

    const adasDoc = new Y.Doc();
    const adasText = adasDoc.getText('content');
    adasText.insert(0, 'Local compilation removes the queue. Everything else follows from that.');
    const ada = await LiveSession.connect(adasDoc, adasInvite, { name: 'Ada', color: '#30bced', colorLight: '#30bced33' }, [100, 200]);

    const beasDoc = new Y.Doc();
    const beasText = beasDoc.getText('content');
    const bea = await LiveSession.connect(beasDoc, beasInvite, { name: 'Bea', color: '#ee6352', colorLight: '#ee635233' }, [100, 200]);
    await waitUntil(() => beasText.toString() === adasText.toString() && beasText.length > 0, 'Bea to receive the paragraph');

    // Together: both type into the same paragraph, at once, in different places.
    adasText.insert(0, 'Abstract. ');
    beasText.insert(beasText.length, ' Latency is the feature.');
    await waitUntil(() => adasText.toString() === beasText.toString() && adasText.toString().includes('Latency is the feature.') && adasText.toString().startsWith('Abstract.'), 'the two to converge');

    // Bea's cable is pulled.
    cable.unplug();
    await waitUntil(() => bea.status === 'reconnecting', 'Bea to notice');

    // Apart: both keep writing into the same paragraph. Neither can see the other.
    const beasWhileAway = ' Offline, Bea wrote this sentence, and then another one after it.';
    beasText.insert(beasText.length, beasWhileAway);
    beasText.insert(beasText.toString().indexOf('removes'), 'entirely ');
    adasText.insert(adasText.toString().indexOf('Everything'), 'Ada adds a clause here. ');
    await new Promise((resolve) => setTimeout(resolve, 1_500)); // long enough for several refused retries
    expect(adasText.toString()).not.toContain('Offline, Bea');
    expect(beasText.toString()).not.toContain('Ada adds');
    expect(bea.status).toBe('reconnecting');

    // The cable is back.
    cable.plugIn();
    await waitUntil(() => bea.status === 'connected', 'Bea to reconnect');
    await waitUntil(() => adasText.toString() === beasText.toString(), 'the two to converge again');

    const final = adasText.toString();
    for (const piece of ['Abstract.', 'Latency is the feature.', 'Offline, Bea wrote this sentence, and then another one after it.', 'Ada adds a clause here.', 'entirely removes', 'Local compilation']) {
      expect(final, `"${piece}" survived`).toContain(piece);
    }

    // And still live afterwards: a further edit crosses in both directions.
    adasText.insert(0, '[A] ');
    beasText.insert(beasText.length, ' [B]');
    await waitUntil(() => adasText.toString() === beasText.toString() && adasText.toString().startsWith('[A] ') && adasText.toString().endsWith(' [B]'), 'a post-reconnect edit to cross both ways');

    await ada.destroy();
    await bea.destroy();
    cable.close();
  }, 30_000);
});
