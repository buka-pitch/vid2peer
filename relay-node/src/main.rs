//! Circuit relay node.
//!
//! A relay is used ONLY when two peers cannot connect directly (NAT,
//! firewall, CGNAT). It forwards encrypted packets between peers and never
//! decodes or processes the media — it is not a video server.

use anyhow::Context;
use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{
    identify, kad, mdns, noise, ping, relay, tcp, yamux, PeerId,
};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "relay-node", about = "P2P video chat circuit relay node")]
struct Args {
    /// Listen address(es). Repeatable.
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/4002")]
    listen: Vec<String>,

    /// Where to persist this node's identity.
    #[arg(long, default_value = "relay-node-data/identity.key")]
    identity_file: PathBuf,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    mdns: mdns::tokio::Behaviour,
    relay: relay::Behaviour,
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

    let keypair = p2p_video_chat_core::identity::load_or_create_keypair(&args.identity_file)?;
    let peer_id: PeerId = keypair.public().to_peer_id();

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::new(),
            noise::Config::new,
            yamux::Config::default,
        )
        .with_context(|| "failed to build transport")?
        .with_behaviour(|key| {
            let pid = key.public().to_peer_id();
            Ok(Behaviour {
                identify: identify::Behaviour::new(identify::Config::new(
                    "/p2p-video-chat/identify/1.0.0".to_string(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
                kademlia: {
                    let mut k = kad::Behaviour::new(
                        pid,
                        kad::store::MemoryStore::new(pid),
                    );
                    k.set_mode(Some(kad::Mode::Server));
                    k
                },
                mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), pid)?,
                relay: relay::Behaviour::new(pid, relay::Config::default()),
            })
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(std::time::Duration::from_secs(3600)))
        .build();

    for l in &args.listen {
        let addr: libp2p::Multiaddr = l.parse().context("invalid listen address")?;
        swarm
            .listen_on(addr.clone())
            .with_context(|| format!("failed to listen on {addr}"))?;
    }

    println!("relay-node peer id: {peer_id}");
    println!("circuit relay v2 is now enabled on this node");

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("LISTENING {address}");
                println!(
                    "RELAY_INFO {}/p2p/{peer_id}",
                    address
                );
            }
            SwarmEvent::Behaviour(BehaviourEvent::Relay(ev)) => {
                tracing::info!(?ev, "relay event");
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                for addr in info.listen_addrs {
                    swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
            }
            other => {
                tracing::debug!(event = ?other, "swarm event");
            }
        }
    }
}
