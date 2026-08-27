//! Bootstrap node.
//!
//! A bootstrap node's ONLY job is to help new peers enter the network: it
//! answers Kademlia lookups and acts as a well-known, stable peer that other
//! peers can connect to at startup. It is NOT an application server, does not
//! process media, and the network keeps operating even if it disappears.

use anyhow::Context;
use clap::Parser;
use p2p_video_chat_core::{BootstrapPeer, Event, P2pConfig, Peer};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "bootstrap-node", about = "P2P video chat bootstrap node")]
struct Args {
    /// Listen address(es). Repeatable.
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/4001")]
    listen: Vec<String>,

    /// Existing bootstrap node to join (`<peer-id>/p2p/<multiaddr>`).
    #[arg(long)]
    peer: Vec<String>,

    /// Where to persist this node's identity.
    #[arg(long, default_value = "bootstrap-node-data/identity.key")]
    identity_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let args = Args::parse();

    let bootstrap_peers = parse_bootstrap_peers(&args.peer)?;

    let config = P2pConfig {
        listen_addrs: args
            .listen
            .iter()
            .map(|s| s.parse().context("invalid listen address"))
            .collect::<anyhow::Result<_>>()?,
        bootstrap_peers,
        identity_file: Some(args.identity_file),
        ..P2pConfig::default()
    };

    let mut peer = Peer::new(config).context("failed to start bootstrap peer")?;
    println!("bootstrap-node peer id: {}", peer.peer_id());
    println!("protocol: {}", p2p_video_chat_core::protocol::IDENTIFY_PROTOCOL);

    loop {
        let event = peer.next_event().await;
        match event {
            Event::ListeningOn { address } => {
                println!("LISTENING {address}");
                println!(
                    "BOOTSTRAP_INFO {}",
                    peer.local_addresses()
                        .iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            Event::PeerConnected { peer_id } => {
                println!("connected {peer_id}");
            }
            Event::BootstrapCompleted { peers } => {
                println!("bootstrap completed with {peers} peers");
            }
            other => {
                println!("[{}] {other:?}", other.tag());
            }
        }
    }
}

fn parse_bootstrap_peers(inputs: &[String]) -> anyhow::Result<Vec<BootstrapPeer>> {
    let mut out = Vec::new();
    for s in inputs {
        let addr: libp2p::Multiaddr = s.parse()?;
        let mut found = None;
        for p in addr.iter() {
            if let libp2p::multiaddr::Protocol::P2p(id) = p {
                found = Some(id);
            }
        }
        let peer_id = found.with_context(|| format!("address {s} is missing a /p2p/<peer-id> suffix"))?;
        out.push(BootstrapPeer {
            peer_id,
            address: addr,
        });
    }
    Ok(out)
}
