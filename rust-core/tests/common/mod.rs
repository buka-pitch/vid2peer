//! Shared helpers for integration tests.

#![allow(dead_code)]

use p2p_video_chat_core::{Event, P2pConfig, Peer};
use std::time::Duration;

pub fn local_config() -> P2pConfig {
    P2pConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_mdns: false,
        idle_connection_timeout: Duration::from_secs(3600),
        ..P2pConfig::default()
    }
}

pub async fn new_peer() -> (Peer, libp2p::Multiaddr) {
    let mut peer = Peer::new(local_config()).expect("peer should build");
    let addr = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match peer.next_event().await {
                Event::ListeningOn { address } => break address,
                _ => {}
            }
        }
    })
    .await
    .expect("peer should listen");
    (peer, addr)
}

pub async fn connect(a: &mut Peer, a_id: libp2p::PeerId, b: &mut Peer, a_addr: &libp2p::Multiaddr) {
    b.dial(a_addr.clone()).expect("dial should succeed");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut a_seen = false;
    let mut b_seen = false;
    while tokio::time::Instant::now() < deadline && !(a_seen && b_seen) {
        tokio::select! {
            ev = a.next_event() => {
                if let Event::PeerConnected { peer_id } = ev {
                    if peer_id == b.peer_id() { a_seen = true; }
                }
            }
            ev = b.next_event() => {
                if let Event::PeerConnected { peer_id } = ev {
                    if peer_id == a_id { b_seen = true; }
                }
            }
        }
    }
    assert!(a_seen, "peer A did not observe the connection");
    assert!(b_seen, "peer B did not observe the connection");
}

/// Collect events from both peers until `cond` returns true or timeout.
pub async fn drive_until<F>(
    a: &mut Peer,
    b: &mut Peer,
    timeout: Duration,
    mut cond: F,
) where
    F: FnMut(&Event) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let done = tokio::select! {
            ev = a.next_event() => cond(&ev),
            ev = b.next_event() => cond(&ev),
        };
        if done {
            return;
        }
    }
}
