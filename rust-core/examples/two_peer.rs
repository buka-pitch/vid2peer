//! Phase 1 demo: two Rust peers connect over libp2p.
//!
//! Demonstrates: peer IDs, TCP transport, Noise encryption, Yamux
//! multiplexing, Identify and Ping.
//!
//! Run:
//! ```bash
//! cargo run -p p2p-video-chat-core --example two_peer
//! ```

use p2p_video_chat_core::{Event, P2pConfig, Peer};
use std::time::Duration;

fn config(name: &str) -> P2pConfig {
    P2pConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_mdns: false,
        agent_version: format!("p2p-video-chat-demo/{name}"),
        ..P2pConfig::default()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    // --- Peer A ---------------------------------------------------------
    let mut a = Peer::new(config("alice"))?;
    println!("Peer A (Alice):   {}", a.peer_id());

    // Wait for A's listen address.
    let a_addr = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match a.next_event().await {
                Event::ListeningOn { address } => break address,
                _ => {}
            }
        }
    })
    .await?;
    println!("Peer A listening on {a_addr}");

    // --- Peer B ---------------------------------------------------------
    let mut b = Peer::new(config("bob"))?;
    println!("Peer B (Bob):     {}", b.peer_id());

    let b_addr = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match b.next_event().await {
                Event::ListeningOn { address } => break address,
                _ => {}
            }
        }
    })
    .await?;
    println!("Peer B listening on {b_addr}");

    // --- Connect A -> B -------------------------------------------------
    b.dial(a_addr.clone())?;
    println!("Peer B dialing Peer A...");

    let mut a_connected = false;
    let mut b_connected = false;
    let mut a_identified = false;
    let mut b_identified = false;
    let mut ping_seen = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            ev = a.next_event() => match ev {
                Event::PeerConnected { peer_id } => { a_connected = true; println!("[A] connected to {peer_id}"); }
                Event::PeerIdentified(info) => { a_identified = true; println!("[A] identified {info:?}"); }
                Event::PingResult { peer_id, rtt } => { ping_seen = true; println!("[A] ping {peer_id} rtt={rtt:?}"); }
                Event::ConnectionError { error, .. } => println!("[A] connection error: {error}"),
                _ => {}
            },
            ev = b.next_event() => match ev {
                Event::PeerConnected { peer_id } => { b_connected = true; println!("[B] connected to {peer_id}"); }
                Event::PeerIdentified(info) => { b_identified = true; println!("[B] identified {info:?}"); }
                Event::PingResult { peer_id, rtt } => { ping_seen = true; println!("[B] ping {peer_id} rtt={rtt:?}"); }
                Event::ConnectionError { error, .. } => println!("[B] connection error: {error}"),
                _ => {}
            },
        }
        if a_connected && b_connected && a_identified && b_identified && ping_seen {
            break;
        }
    }

    assert!(a_connected && b_connected, "peers did not connect");
    assert!(a_identified && b_identified, "identify did not complete");
    assert!(ping_seen, "no ping result received");
    println!("\n=== Phase 1 OK: two peers connected, identified, pinged ===");
    Ok(())
}
