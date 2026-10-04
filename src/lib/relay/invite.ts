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
