//! Phase 3 demo: 1:1 text chat over the `/chat/1.0.0` request-response
//! protocol. The video path is completely separate from the chat path.
//!
//! Run:
//! ```bash
//! cargo run -p p2p-video-chat-core --example chat_demo
//! ```

use p2p_video_chat_core::{Event, P2pConfig, Peer};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cfg = P2pConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_mdns: false,
        ..P2pConfig::default()
    };

    let mut a = Peer::new(cfg.clone())?;
    let mut b = Peer::new(cfg)?;

    println!("Alice: {}", a.peer_id());
    println!("Bob:   {}", b.peer_id());

    // Wait for both to be listening, then connect A -> B.
    let a_addr = wait_listen(&mut a).await;
    let _b_addr = wait_listen(&mut b).await;
    b.dial(a_addr.clone())?;
    println!("connecting Bob -> Alice...");

    // Wait until A sees B as connected.
    let bob_id = b.peer_id();
    wait_peer_connected(&mut a, &mut b, bob_id).await?;
    println!("connected!");

    // Alice sends a message to Bob.
    a.send_chat(&b.peer_id(), "hello from Alice!".into())?;
    println!("Alice -> Bob: hello from Alice!");

    let mut received = false;
    let mut delivered = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);

    while tokio::time::Instant::now() < deadline && !(received && delivered) {
        tokio::select! {
            ev = a.next_event() => match ev {
                Event::ChatMessageDelivered { to, message_id } => {
                    delivered = true;
                    println!("[A] message {message_id} delivered to {to}");
                }
                Event::ChatError { error, .. } => {
                    anyhow::bail!("[A] chat error: {error}");
                }
                _ => {}
            },
            ev = b.next_event() => match ev {
                Event::ChatMessageReceived { from, message } => {
                    received = true;
                    println!("[B] received from {from}: \"{}\"", message.text);
                }
                _ => {}
            },
        }
    }

    assert!(received, "Bob did not receive the message");
    assert!(delivered, "Alice did not get a delivery ack");
    println!("\n=== Phase 3 OK: chat message delivered and acknowledged ===");
    Ok(())
}

async fn wait_listen(peer: &mut Peer) -> libp2p::Multiaddr {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Event::ListeningOn { address } = peer.next_event().await {
                return address;
            }
        }
    })
    .await
    .expect("timed out waiting for listen address")
}

async fn wait_peer_connected(a: &mut Peer, b: &mut Peer, who: libp2p::PeerId) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            tokio::select! {
                ev = a.next_event() => {
                    if let Event::PeerConnected { peer_id } = ev {
                        if peer_id == who {
                            return;
                        }
                    }
                }
                ev = b.next_event() => {
                    if let Event::PeerConnected { peer_id } = ev {
                        if peer_id == who {
                            return;
                        }
                    }
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for connection to {who}"))
}
