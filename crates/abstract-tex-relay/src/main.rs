//! The relay as a standalone process: the thing `DESIGN.md` §10 says ships as "the binary and a
//! Docker image" for a person to run on their own $5 VPS. It never ships inside the desktop app
//! and the app crate does not depend on this one — `abstract-tex-relay::Relay` is the only part
//! this binary adds to `main`.

use tokio::net::TcpListener;

use abstract_tex_relay::Relay;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = std::env::var("ABSTRACT_TEX_RELAY_ADDR").unwrap_or_else(|_| "0.0.0.0:4444".to_string());
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "abstract-tex-relay listening");

    let relay = Relay::new();
    loop {
        let (stream, peer) = listener.accept().await?;
        let relay = relay.clone();
        tokio::spawn(async move {
            if let Err(error) = relay.accept(stream).await {
                tracing::debug!(%peer, %error, "connection ended before it joined a room");
            }
        });
    }
}
