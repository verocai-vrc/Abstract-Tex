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
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("the WebSocket handshake failed: {0}")]
    Handshake(#[source] tokio_tungstenite::tungstenite::Error),
}

/// One connected peer: an id unique within this process (so a room can tell two connections from
/// the same room apart) and the channel its own task reads from to learn what to send it.
struct Peer {
    id: u64,
    outbox: mpsc::UnboundedSender<Message>,
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
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
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
        let socket = tokio_tungstenite::accept_hdr_async(stream, read_room_from_path)
            .await
            .map_err(RelayError::Handshake)?;
        self.serve(room, socket).await;
        Ok(())
    }

    async fn serve(&self, room: String, socket: tokio_tungstenite::WebSocketStream<TcpStream>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (mut write, mut read) = socket.split();
        let (outbox, mut inbox) = mpsc::unbounded_channel();

        self.rooms
            .lock()
            .await
            .entry(room.clone())
            .or_default()
            .push(Peer { id, outbox });

        // This connection's write half lives in its own task so that a slow reader in the same
        // room cannot block every other peer's broadcast — `broadcast` only ever has to put a
        // message on this unbounded channel, never wait for a socket write.
        let relay_to_client = tokio::spawn(async move {
            while let Some(message) = inbox.recv().await {
                if write.send(message).await.is_err() {
                    break;
                }
            }
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
        let rooms = self.rooms.lock().await;
        let Some(peers) = rooms.get(room) else { return };
        for peer in peers {
            if peer.id != from {
                // A send fails only once that peer's own task has already ended and dropped its
                // receiver — `serve`'s own cleanup will remove it from the room momentarily, so
                // there is nothing for this call to do about it.
                let _ = peer.outbox.send(message.clone());
            }
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
}
