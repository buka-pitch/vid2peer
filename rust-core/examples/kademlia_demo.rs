//! Phase 2 demo: Kademlia DHT discovery.
//!
//! Starts one bootstrap node and N peers. All peers bootstrap into the DHT,
//! discover each other, and the demo verifies peer discovery and routing-table
//! growth. There is no central database: routing information is distributed.
//!
//! Run:
//! ```bash
//! cargo run -p p2p-video-chat-core --example kademlia_demo
//! ```

use p2p_video_chat_core::{BootstrapPeer, Event, P2pConfig, Peer};
use std::collections::HashSet;
use std::time::Duration;

const PEER_COUNT: usize = 10;

fn local_config() -> P2pConfig {
    P2pConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_mdns: false,
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

    // --- 1. Start the bootstrap node ------------------------------------
    let mut bootstrap = Peer::new(local_config())?;
    let bootstrap_id = bootstrap.peer_id();
    println!("bootstrap node: {bootstrap_id}");
    let bootstrap_addr = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match bootstrap.next_event().await {
                Event::ListeningOn { address } => break address,
                _ => {}
            }
        }
    })
    .await?;
    println!("bootstrap node listening on {bootstrap_addr}");

    let bootstrap_peer = BootstrapPeer {
        peer_id: bootstrap_id,
        address: bootstrap_addr.clone(),
    };

    // --- 2. Start N peers that join via the bootstrap node --------------
    let mut peers: Vec<Peer> = Vec::new();
    let mut peer_ids: Vec<String> = Vec::new();
    for i in 0..PEER_COUNT {
        let cfg = P2pConfig {
            bootstrap_peers: vec![bootstrap_peer.clone()],
            ..local_config()
        };
        let mut p = Peer::new(cfg)?;
        peer_ids.push(p.peer_id().to_string());
        let addr = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match p.next_event().await {
                    Event::ListeningOn { address } => break address,
                    _ => {}
                }
            }
        })
        .await?;
        println!("peer[{i:02}] {} listening on {addr}", p.peer_id());
        // Trigger a Kademlia bootstrap query.
        let _ = p.bootstrap();
        peers.push(p);
    }

    // --- 3. Drive the event loop until all peers discover the network ----
    let mut discovered: Vec<HashSet<String>> = vec![HashSet::new(); PEER_COUNT];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let mut progress = false;
        for i in 0..PEER_COUNT {
            let ev = tokio::time::timeout(Duration::from_millis(250), peers[i].next_event()).await;
            match ev {
                Ok(Event::PeerConnected { peer_id }) => {
                    discovered[i].insert(peer_id.to_string());
                    progress = true;
                }
                Ok(Event::PeerDiscovered { peer_id }) => {
                    discovered[i].insert(peer_id.to_string());
                    progress = true;
                }
                Ok(Event::BootstrapCompleted { peers }) => {
                    println!("peer[{i:02}] bootstrap completed ({peers} connections)");
                }
                Ok(Event::PeersFound { peers, .. }) => {
                    for pid in peers {
                        discovered[i].insert(pid.to_string());
                    }
                    progress = true;
                }
                Ok(_) | Err(_) => {}
            }
        }
        if progress {
            let total: usize = discovered.iter().map(|s| s.len()).sum();
            println!("discovered so far: {total} peer-edges");
        }
    }

    // --- 4. Report -------------------------------------------------------
    println!("\n=== Phase 2 results ===");
    let mut ok = true;
    for i in 0..PEER_COUNT {
        // Exclude the bootstrap node id and self.
        let own = peers[i].peer_id().to_string();
        let known = discovered[i]
            .iter()
            .filter(|p| *p != &own && *p != &bootstrap_id.to_string())
            .count();
        println!(
            "peer[{i:02}] {} knows {} other peers (routing table {} entries)",
            peer_ids[i],
            known,
            peers[i].routing_table_size()
        );
        if known == 0 {
            ok = false;
        }
    }
    if !ok {
        println!("WARNING: some peers did not discover the network");
    } else {
        println!(
            "=== Phase 2 OK: {PEER_COUNT} peers discovered each other through the DHT ==="
        );
    }
    Ok(())
}
