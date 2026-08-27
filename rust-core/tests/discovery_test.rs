//! Phase 2 integration test: Kademlia bootstrap and decentralized discovery
//! with 10+ local peers and no central database.

mod common;

use p2p_video_chat_core::{BootstrapPeer, Event, P2pConfig, Peer};
use std::collections::HashSet;
use std::time::Duration;

const PEER_COUNT: usize = 10;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kademlia_discovers_peers_decentralized() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();

    // Bootstrap node.
    let (bootstrap, bootstrap_addr) = common::new_peer().await;
    let bootstrap_id = bootstrap.peer_id();
    let bootstrap_peer = BootstrapPeer {
        peer_id: bootstrap_id,
        address: bootstrap_addr.clone(),
    };

    // N peers join through the bootstrap node.
    let mut peers: Vec<Peer> = Vec::new();
    for _ in 0..PEER_COUNT {
        let cfg = P2pConfig {
            bootstrap_peers: vec![bootstrap_peer.clone()],
            ..common::local_config()
        };
        let mut p = Peer::new(cfg).unwrap();
        p.bootstrap().ok();
        peers.push(p);
    }

    // Drive all peers until they discover each other.
    let mut discovered: Vec<HashSet<libp2p::PeerId>> = vec![HashSet::new(); PEER_COUNT];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let mut changed = false;
        for i in 0..PEER_COUNT {
            let ev = tokio::time::timeout(Duration::from_millis(200), peers[i].next_event()).await;
            match ev {
                Ok(Event::PeerConnected { peer_id }) => {
                    discovered[i].insert(peer_id);
                    changed = true;
                }
                Ok(Event::PeerDiscovered { peer_id }) => {
                    discovered[i].insert(peer_id);
                    changed = true;
                }
                Ok(Event::PeersFound { peers: found, .. }) => {
                    for pid in found {
                        discovered[i].insert(pid);
                    }
                    changed = true;
                }
                Ok(_) | Err(_) => {}
            }
        }
        // Stop early when every peer knows the whole network.
        if discovered.iter().all(|set| set.len() >= PEER_COUNT) {
            break;
        }
        if !changed {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    // Report.
    let mut all_ok = true;
    for i in 0..PEER_COUNT {
        let own = peers[i].peer_id();
        let knows_others = discovered[i]
            .iter()
            .filter(|p| **p != own && **p != bootstrap_id)
            .count();
        assert!(
            knows_others > 0,
            "peer {i} ({own}) discovered no other peers; routing table size {}",
            peers[i].routing_table_size()
        );
        if knows_others == 0 {
            all_ok = false;
        }
    }
    assert!(all_ok, "not all peers discovered the network");
}
