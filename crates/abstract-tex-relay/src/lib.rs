//! S14.1: the v0.8 relay. `DESIGN.md` §5.6 calls it "a small self-hostable WebSocket relay" and
//! says "the relay stores nothing durable"; §10 and design-interview.md F6 settle the rest of the
//! shape — an invite link carries the relay's address and a random room secret, and updates are
//! end-to-end encrypted with a key derived from that secret before they ever reach this process.
//!
//! That last decision is what makes this crate small. A relay that could read Yjs's sync and
//! awareness protocol would need to speak it; a relay that only ever sees ciphertext cannot, and
//! the design says it must not even try. So this relay does exactly one thing: a client connects
//! at a path naming a room (`ws://host:port/<room>`), and any message it sends is broadcast,
//! unread, to every *other* client currently connected to that same room. Nothing is parsed,
//! nothing is stored — a message to an empty room is simply gone, and a client that joins a
//! room after a message was sent never sees it. That is the whole contract.
//!
//! **What this must never do:** keep a message once every current reader has (or will never)
//! receive it, persist anything to disk, or inspect a payload's bytes beyond the WebSocket frame
//! header tungstenite already has to read to deliver it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("the WebSocket handshake failed: {0}")]
    Handshake(#[source] tokio_tungstenite::tungstenite::Error),

    #[error("the client did not finish the WebSocket handshake in time")]
    HandshakeTimedOut,
}

/// What one relay will carry. The relay runs on a person's own server, reachable from anywhere, and
/// used to accept anything: a connection that never finished the upgrade held a task for ever, a room
/// had no size, a message had no cap beyond the library's 64 MiB, and a peer that stopped reading made
/// every sender's copy pile up in memory. The defaults are far above what a few authors editing one
/// thesis ever meet, and every one is a field so a test (or a bigger deployment) can change it.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// How long a new connection has to complete the WebSocket upgrade.
    pub handshake_timeout: std::time::Duration,
    /// How many peers one room holds; the next one is turned away.
    pub peers_per_room: usize,
    /// How many rooms exist at once; a connection that would open another is turned away.
    pub rooms: usize,
    /// The largest message, and the largest frame, accepted from a peer. Bigger ends that connection.
    pub message_bytes: usize,
    /// How many messages may wait for one slow peer before that peer is dropped.
    pub queued_messages: usize,
    /// How many connections [`Relay::run`] serves at once.
    pub connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake_timeout: std::time::Duration::from_secs(10),
            peers_per_room: 16,
            rooms: 1_000,
            // A whole thesis' worth of Yjs state in one first sync is a few megabytes.
            message_bytes: 16 * 1024 * 1024,
            queued_messages: 1_024,
            connections: 2_048,
        }
    }
}

/// One connected peer: an id unique within this process (so a room can tell two connections from
/// the same room apart) and the channel its own task reads from to learn what to send it.
struct Peer {
    id: u64,
    /// Bounded: when a peer stops reading and this fills, the peer is dropped (see `broadcast`),
    /// which is what stops one stalled reader from growing the relay's memory without limit.
    outbox: mpsc::Sender<Message>,
}

/// Every room currently in use, keyed by the path a client connected on.
///
/// `tokio::sync::Mutex`, not `std`'s: the same choice `abstract-tex-lsp::bridge::Pending` already
/// made, and for the same reason — this map is read and written from inside async tasks, and a
/// `std::sync::Mutex` held across an `.await` can deadlock the executor. Nothing here holds the
/// lock across an `.await` either, but one kind of mutex throughout an async codebase is the
/// easier rule to keep.
type Rooms = Arc<Mutex<HashMap<String, Vec<Peer>>>>;

/// The relay's state, cheap to clone: every clone shares the same rooms, so one value is made in
/// `main` and handed to every accepted connection's task.
#[derive(Clone, Default)]
pub struct Relay {
    rooms: Rooms,
    next_id: Arc<AtomicU64>,
    limits: Limits,
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
    }

    /// A relay with other limits than the defaults.
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// Accept connections on `listener` for ever, at most `limits.connections` at once; one more
    /// than that is closed at once, before anything is read from it.
    pub async fn run(&self, listener: tokio::net::TcpListener) -> std::io::Result<()> {
        let permits = Arc::new(tokio::sync::Semaphore::new(self.limits.connections));
        loop {
            let (stream, peer) = listener.accept().await?;
            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                tracing::debug!(%peer, "too many connections; turning one away");
                drop(stream);
                continue;
            };
            let relay = self.clone();
            tokio::spawn(async move {
                if let Err(error) = relay.accept(stream).await {
                    tracing::debug!(%peer, %error, "connection ended before it joined a room");
                }
                drop(permit); // held until this connection is over
            });
        }
    }

    /// Handshake `stream` as a WebSocket connection and serve it until it closes.
    ///
    /// Takes a plain `TcpStream` rather than something already upgraded, so the caller (`main.rs`,
    /// or a test standing in for it) owns exactly one job each: accepting TCP connections and
    /// answering what each one turns out to want.
    pub async fn accept(&self, stream: TcpStream) -> Result<(), RelayError> {
        let mut room = String::new();
        // The callback tungstenite calls with the client's handshake request, before the 101
        // response is sent. It is the only place the request's path is visible — once the
        // upgrade completes there is no HTTP request left, only a WebSocket connection — so the
        // room name is read here and stashed in `room` for `serve` to use afterwards.
        //
        // The `Result`'s error side is `tungstenite::handshake::server::ErrorResponse`, a type
        // this crate did not choose — it is `Callback::on_request`'s own signature — so the
        // closure is never going to return the small-error shape clippy's default lint wants.
        #[allow(clippy::result_large_err)]
        let read_room_from_path = |request: &Request, response: Response| {
            room = request.uri().path().trim_start_matches('/').to_string();
            Ok(response)
        };
        // Capped in size (a message over `message_bytes` ends the connection) and in time (a client
        // that never finishes the upgrade is dropped, not held for ever).
        let config = WebSocketConfig::default()
            .max_message_size(Some(self.limits.message_bytes))
            .max_frame_size(Some(self.limits.message_bytes));
        let handshake =
            tokio_tungstenite::accept_hdr_async_with_config(stream, read_room_from_path, Some(config));
        let socket = tokio::time::timeout(self.limits.handshake_timeout, handshake)
            .await
            .map_err(|_| RelayError::HandshakeTimedOut)?
            .map_err(RelayError::Handshake)?;
        self.serve(room, socket).await;
        Ok(())
    }

    async fn serve(&self, room: String, socket: tokio_tungstenite::WebSocketStream<TcpStream>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (mut write, mut read) = socket.split();
        let (outbox, mut inbox) = mpsc::channel(self.limits.queued_messages.max(1));

        {
            let mut rooms = self.rooms.lock().await;
            let room_is_new = !rooms.contains_key(&room);
            let full = rooms
                .get(&room)
                .is_some_and(|peers| peers.len() >= self.limits.peers_per_room);
            if full || (room_is_new && rooms.len() >= self.limits.rooms) {
                drop(rooms);
                // Turned away: the client sees an ordinary close and may try another room.
                let mut socket = write.reunite(read).expect("both halves came from this socket");
                let _ = socket.close(None).await;
                return;
            }
            rooms.entry(room.clone()).or_default().push(Peer { id, outbox });
        }

        // This connection's write half lives in its own task so that a slow reader in the same
        // room cannot block every other peer's broadcast — `broadcast` only ever has to put a
        // message on this unbounded channel, never wait for a socket write.
        let relay_to_client = tokio::spawn(async move {
            while let Some(message) = inbox.recv().await {
                if write.send(message).await.is_err() {
                    return;
                }
            }
            // The queue closed because the relay dropped this peer (it stopped reading): say so
            // with an ordinary close instead of leaving the connection hanging.
            let _ = write.close().await;
        });

        while let Some(received) = read.next().await {
            let Ok(message) = received else { break };
            if message.is_close() {
                break;
            }
            // Pings, pongs and raw frames carry nothing a peer needs; only an actual payload is
            // worth a broadcast. tungstenite answers pings on this connection by itself.
            if message.is_text() || message.is_binary() {
                self.broadcast(&room, id, message).await;
            }
        }

        self.leave(&room, id).await;
        relay_to_client.abort();
    }

    async fn broadcast(&self, room: &str, from: u64, message: Message) {
        let mut rooms = self.rooms.lock().await;
        let Some(peers) = rooms.get_mut(room) else { return };
        // A peer whose queue is full has stopped reading, and one whose queue is closed is gone:
        // both are dropped from the room, which ends their connection (see `serve`). Waiting for
        // them, or queueing without a limit, is how one stalled reader used to grow the relay's
        // memory without bound.
        peers.retain(|peer| peer.id == from || peer.outbox.try_send(message.clone()).is_ok());
        if peers.is_empty() {
            rooms.remove(room);
        }
    }

    async fn leave(&self, room: &str, id: u64) {
        let mut rooms = self.rooms.lock().await;
        if let Some(peers) = rooms.get_mut(room) {
            peers.retain(|peer| peer.id != id);
            if peers.is_empty() {
                rooms.remove(room);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

    /// A relay listening on an OS-assigned port, accepting connections in the background for as
    /// long as the test runs.
    async fn start() -> SocketAddr {
        let relay = Relay::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let relay = relay.clone();
                tokio::spawn(async move {
                    let _ = relay.accept(stream).await;
                });
            }
        });
        addr
    }

    async fn join(addr: SocketAddr, room: &str) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
        let (socket, _) = connect_async(format!("ws://{addr}/{room}")).await.unwrap();
        socket
    }

    /// Nothing here signals "the room now has two members" back to the test, so every test gives
    /// both ends of a join a moment to land before relying on the room's membership — the same
    /// kind of short, explicit wait `abstract-tex-engine`'s cancel tests already use for "has the
    /// other side noticed yet?".
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn two_peers_in_the_same_room_see_each_others_messages() {
        let addr = start().await;
        let mut a = join(addr, "room-1").await;
        let mut b = join(addr, "room-1").await;
        settle().await;

        a.send(Message::binary(b"hello".to_vec())).await.unwrap();

        let received = b.next().await.unwrap().unwrap();
        assert_eq!(received.into_data().as_ref(), b"hello");
    }

    #[tokio::test]
    async fn a_peer_in_a_different_room_sees_nothing() {
        let addr = start().await;
        let mut a = join(addr, "room-1").await;
        let mut c = join(addr, "room-2").await;
        settle().await;

        a.send(Message::binary(b"only for room-1".to_vec()))
            .await
            .unwrap();

        let saw_nothing = tokio::time::timeout(Duration::from_millis(200), c.next()).await;
        assert!(
            saw_nothing.is_err(),
            "a different room must not receive this message"
        );
    }

    #[tokio::test]
    async fn a_message_to_a_room_with_no_other_peer_raises_no_error() {
        let addr = start().await;
        let mut alone = join(addr, "empty-room").await;
        alone
            .send(Message::binary(b"nobody is listening".to_vec()))
            .await
            .unwrap();
        // The relay must still be answering afterwards — proven by a second, ordinary exchange.
        let mut also_alone = join(addr, "another-empty-room").await;
        also_alone
            .send(Message::binary(b"still fine".to_vec()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_reconnecting_peer_never_receives_what_was_sent_while_it_was_away() {
        let addr = start().await;
        let mut a = join(addr, "room-1").await;
        let mut b = join(addr, "room-1").await;
        settle().await;

        a.close(None).await.unwrap();
        settle().await;

        b.send(Message::binary(b"sent while a was away".to_vec()))
            .await
            .unwrap();
        settle().await;

        let mut a_again = join(addr, "room-1").await;
        settle().await;
        b.send(Message::binary(b"sent after a came back".to_vec()))
            .await
            .unwrap();

        let received = a_again.next().await.unwrap().unwrap();
        assert_eq!(
            received.into_data().as_ref(),
            b"sent after a came back",
            "nothing from before the reconnect was queued for the new connection"
        );
    }

    // ---- Review finding (relay, second pass): no resource limits. ----
    //
    // It binds 0.0.0.0 on somebody's own server and accepted anything: sockets that never finished the
    // upgrade held a task each for ever, a room had no size, a message had no cap beyond the library's
    // 64 MiB default, and a peer that stopped reading made every sender's copy pile up in an unbounded
    // queue. All four are limits now, with defaults a small group of authors will never meet.

    async fn start_with(limits: Limits) -> SocketAddr {
        let relay = Relay::with_limits(limits);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let relay = relay.clone();
                tokio::spawn(async move {
                    let _ = relay.accept(stream).await;
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn a_connection_that_never_finishes_the_handshake_is_dropped() {
        let relay = Relay::with_limits(Limits {
            handshake_timeout: Duration::from_millis(200),
            ..Limits::default()
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _silent = tokio::net::TcpStream::connect(addr).await.unwrap(); // connects and says nothing
        let (stream, _) = listener.accept().await.unwrap();

        let outcome = tokio::time::timeout(Duration::from_secs(5), relay.accept(stream))
            .await
            .expect("the relay must give up on its own");

        assert!(
            matches!(outcome, Err(RelayError::HandshakeTimedOut)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_full_room_turns_the_next_peer_away() {
        let addr = start_with(Limits {
            peers_per_room: 2,
            ..Limits::default()
        })
        .await;
        let mut a = join(addr, "full").await;
        let mut b = join(addr, "full").await;
        let mut c = join(addr, "full").await;
        settle().await;

        // The third is closed rather than admitted: it sees the end of the stream, and what the
        // first sends reaches the second only.
        let third = tokio::time::timeout(Duration::from_secs(2), c.next())
            .await
            .expect("c was not closed");
        assert!(
            matches!(third, None | Some(Ok(Message::Close(_))) | Some(Err(_))),
            "{third:?}"
        );
        a.send(Message::binary(b"hello".to_vec())).await.unwrap();
        assert_eq!(b.next().await.unwrap().unwrap().into_data().as_ref(), b"hello");
    }

    #[tokio::test]
    async fn a_message_over_the_size_limit_ends_that_connection_and_reaches_nobody() {
        let addr = start_with(Limits {
            message_bytes: 1024,
            ..Limits::default()
        })
        .await;
        let mut sender = join(addr, "room").await;
        let mut listener = join(addr, "room").await;
        settle().await;

        let _ = sender.send(Message::binary(vec![7u8; 4096])).await;

        let nothing = tokio::time::timeout(Duration::from_millis(300), listener.next()).await;
        assert!(
            nothing.is_err(),
            "an oversized message must not be forwarded: {nothing:?}"
        );
        // The sender's own connection is over.
        let ended = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(Ok(message)) = sender.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "the connection that sent it was not closed");
    }

    #[tokio::test]
    async fn a_peer_that_stops_reading_is_dropped_instead_of_queueing_for_ever() {
        let relay = Relay::with_limits(Limits {
            queued_messages: 2,
            ..Limits::default()
        });
        // Two peers by hand: one that is never read from, one that is.
        let (stalled_tx, _stalled_rx) = mpsc::channel(2);
        let (healthy_tx, mut healthy_rx) = mpsc::channel(2);
        relay.rooms.lock().await.insert(
            "room".to_string(),
            vec![
                Peer {
                    id: 1,
                    outbox: stalled_tx,
                },
                Peer {
                    id: 2,
                    outbox: healthy_tx,
                },
            ],
        );

        for n in 0..5u8 {
            relay.broadcast("room", 99, Message::binary(vec![n])).await;
            let _ = healthy_rx.try_recv(); // the healthy one keeps up
        }

        let rooms = relay.rooms.lock().await;
        let ids: Vec<u64> = rooms["room"].iter().map(|peer| peer.id).collect();
        assert_eq!(ids, vec![2], "only the peer that stopped reading was dropped");
    }

    #[tokio::test]
    async fn connections_past_the_cap_are_closed_before_anything_is_read() {
        let relay = Relay::with_limits(Limits {
            connections: 1,
            ..Limits::default()
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = relay.run(listener).await;
        });
        let _first = join(addr, "room").await; // takes the only place
        settle().await;

        let mut second = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut buffer = [0u8; 1];
        let read = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::io::AsyncReadExt::read(&mut second, &mut buffer),
        )
        .await
        .expect("the relay left the extra connection open");

        assert!(matches!(read, Ok(0) | Err(_)), "closed, not served: {read:?}");
    }
}
