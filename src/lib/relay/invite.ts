// S14.2c: an invite is a relay address, a room id, and a room secret, combined into one string a
// coauthor pastes into *Join* — never an OS-registered link, so there is no URL-scheme
// registration to ship on three platforms for a feature this small. The key lives after a `#`,
// the one part of a URL a client never sends over the wire, which is the same "a relay operator
// sees only ciphertext" property design-interview.md F6 settled, visible in the format itself.

const DEFAULT_SCHEME = 'ws://';

/** `relay.example.com:8080`, `ws://relay.example.com:8080`, or with a trailing slash — anything
 * an author might type or paste — becomes one `ws://` or `wss://` origin with no trailing slash. */
function normalizeRelayAddress(address: string): string {
  const trimmed = address.trim().replace(/\/+$/, '');
  return /^wss?:\/\//.test(trimmed) ? trimmed : `${DEFAULT_SCHEME}${trimmed}`;
}

/** Whether `host` is this computer: `localhost` or a loopback address. A name that merely resolves
 * there cannot be told from here, so it is not trusted (the assistant's address check does the same). */
function isThisComputer(host: string): boolean {
  const bare = host.replace(/^\[|\]$/g, '').toLowerCase();
  return bare === 'localhost' || bare === '::1' || /^127(\.\d{1,3}){3}$/.test(bare);
}

/**
 * A sentence for an address that reaches another machine without TLS, or `null` when there is
 * nothing to say. An address with no scheme means `ws://` (above), and over `ws://` the room's name,
 * which is the only thing that lets someone into the room, is visible to anyone on the network
 * path. The text itself is safe either way: it is encrypted end to end with a key that stays after
 * the `#` and never reaches the relay. So this informs and does not refuse: the relay binary speaks
 * no TLS, and refusing `ws://` would break running one without a proxy.
 */
export function plaintextRelayNotice(address: string): string | null {
  const trimmed = address.trim();
  if (trimmed === '') return null;
  let url: URL;
  try {
    url = new URL(normalizeRelayAddress(trimmed));
  } catch {
    return null; // not an address yet; the connection attempt will say so
  }
  if (url.protocol !== 'ws:' || isThisComputer(url.hostname)) return null;
  return (
    'This relay is reached without TLS. Your text is still encrypted end to end, but the name of the room ' +
    'can be seen by anyone on the network between you and the relay. Use a wss:// address if the relay has one.'
  );
}

/** 16 bytes of randomness, hex-encoded: long enough that two unrelated authors never collide,
 * short enough to read back while debugging a stuck connection. */
function randomRoomId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function fromBase64Url(text: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]+$/.test(text)) return null;
  const padded = text.replace(/-/g, '+').replace(/_/g, '/');
  const withPadding = padded + '='.repeat((4 - (padded.length % 4)) % 4);
  try {
    const binary = atob(withPadding);
    return Uint8Array.from(binary, (char) => char.charCodeAt(0));
  } catch {
    return null;
  }
}

export interface Invite {
  /** Ready to pass straight to `new WebSocket(...)` — the relay origin plus `/roomId`. */
  relayUrl: string;
  roomId: string;
  /** 32 raw bytes. Never derived from anything that crosses the relay — see `channel.ts`. */
  key: Uint8Array;
}

/** *Share*: a fresh room and a fresh key, every time — a room is never reused across sessions,
 * matching the relay's own "a room is a `Vec<Peer>`, gone the moment it's empty" design. */
export function createInvite(relayAddress: string): Invite {
  const roomId = randomRoomId();
  const key = crypto.getRandomValues(new Uint8Array(32));
  return { relayUrl: `${normalizeRelayAddress(relayAddress)}/${roomId}`, roomId, key };
}

export function inviteToText(invite: Invite): string {
  return `${invite.relayUrl}#${toBase64Url(invite.key)}`;
}

/** *Join*: the inverse of `inviteToText`, or `null` for anything that is not one of this app's
 * own invites — a typo, a stray line copied along with it, anything. */
export function parseInvite(text: string): Invite | null {
  const trimmed = text.trim();
  const hashIndex = trimmed.indexOf('#');
  if (hashIndex === -1) return null;
  const relayUrl = trimmed.slice(0, hashIndex);
  if (!/^wss?:\/\/.+\/.+/.test(relayUrl)) return null;
  const key = fromBase64Url(trimmed.slice(hashIndex + 1));
  if (key === null || key.byteLength !== 32) return null;
  const roomId = relayUrl.slice(relayUrl.lastIndexOf('/') + 1);
  return { relayUrl, roomId, key };
}
