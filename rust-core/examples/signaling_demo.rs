//! Phase 4 demo: WebRTC call signaling over the `/call/1.0.0` protocol.
//!
//! Two peers exchange a full signaling sequence (CallRequest -> CallAccepted
//! -> SdpOffer -> SdpAnswer -> ICE candidates). Only signaling travels through
//! libp2p — the actual media would flow over WebRTC afterwards.
//!
//! Run:
//! ```bash
//! cargo run -p p2p-video-chat-core --example signaling_demo
//! ```

use p2p_video_chat_core::protocol::CallMetadata;
use p2p_video_chat_core::signaling::{CallManager, CallSignaler};
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

    let mut alice = Peer::new(cfg.clone())?;
    let mut bob = Peer::new(cfg)?;
    println!("Alice: {}", alice.peer_id());
    println!("Bob:   {}", bob.peer_id());

    // Connect.
    let a_addr = wait_listen(&mut alice).await;
    let _ = wait_listen(&mut bob).await;
    bob.dial(a_addr.clone())?;
    let bob_id = bob.peer_id();
    wait_peer_connected(&mut alice, &mut bob, bob_id).await?;
    println!("connected. starting call...\n");

    let mut call_manager = CallManager::new();

    // Alice initiates a call with audio+video.
    let mut metadata = CallMetadata::default();
    metadata.media = vec!["audio".into(), "video".into()];
    metadata.display_name = Some("Alice".into());
    let request = CallSignaler::request(&alice.peer_id(), &bob.peer_id(), metadata.clone());
    let call_id = request.call_id().to_string();
    call_manager.on_call_requested(&call_id, bob.peer_id());
    alice.send_call_message(&bob.peer_id(), request)?;
    println!("[Alice] -> CallRequest ({call_id})");

    // Drive both event loops and print the signaling exchange.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut steps = 0usize;
    let target_steps = 5; // accepted, offer, answer, ice (from each side)
    while tokio::time::Instant::now() < deadline && steps < target_steps {
        tokio::select! {
            ev = alice.next_event() => match ev {
                Event::CallMessageReceived { from, message } => {
                    match message {
                        p2p_video_chat_core::protocol::CallMessage::CallAccepted { call_id, .. } => {
                            call_manager.on_call_accepted(&call_id);
                            println!("[Alice] <- CallAccepted ({call_id})");
                            // Alice sends SDP offer.
                            let offer = CallSignaler::sdp_offer(&alice.peer_id(), &call_id, "v=0 o=alice ...".into());
                            alice.send_call_message(&from, offer)?;
                            println!("[Alice] -> SdpOffer ({call_id})");
                            steps += 2;
                        }
                        p2p_video_chat_core::protocol::CallMessage::SdpAnswer { call_id, .. } => {
                            call_manager.on_sdp_exchange(&call_id);
                            println!("[Alice] <- SdpAnswer ({call_id})");
                            let cand = CallSignaler::ice_candidate(&alice.peer_id(), &call_id, "candidate:1 1 UDP ... typ host".into(), Some("0".into()));
                            alice.send_call_message(&from, cand)?;
                            println!("[Alice] -> IceCandidate ({call_id})");
                            steps += 1;
                        }
                        _ => {}
                    }
                }
                Event::CallMessageSent { call_id, msg_type, .. } => {
                    println!("[Alice] + ack {msg_type} ({call_id})");
                    steps += 1;
                }
                Event::CallError { error, .. } => anyhow::bail!("[Alice] call error: {error}"),
                _ => {}
            },
            ev = bob.next_event() => match ev {
                Event::CallMessageReceived { from, message } => {
                    call_manager.record_message(message.clone());
                    match message {
                        p2p_video_chat_core::protocol::CallMessage::CallRequest { call_id, metadata, .. } => {
                            call_manager.on_call_request(&call_id, from, metadata.clone());
                            println!("[Bob]   <- CallRequest ({call_id}) media={:?}", metadata.media);
                            let accept = CallSignaler::accepted(&bob.peer_id(), &call_id);
                            bob.send_call_message(&from, accept)?;
                            println!("[Bob]   -> CallAccepted ({call_id})");
                            steps += 2;
                        }
                        p2p_video_chat_core::protocol::CallMessage::SdpOffer { call_id, sdp, .. } => {
                            call_manager.on_sdp_exchange(&call_id);
                            println!("[Bob]   <- SdpOffer ({call_id}) len={}", sdp.len());
                            let answer = CallSignaler::sdp_answer(&bob.peer_id(), &call_id, "v=0 o=bob ...".into());
                            bob.send_call_message(&from, answer)?;
                            println!("[Bob]   -> SdpAnswer ({call_id})");
                            steps += 2;
                        }
                        p2p_video_chat_core::protocol::CallMessage::IceCandidate { call_id, candidate, .. } => {
                            println!("[Bob]   <- IceCandidate ({call_id}): {candidate}");
                            steps += 1;
                        }
                        _ => {}
                    }
                }
                Event::CallMessageSent { call_id, msg_type, .. } => {
                    println!("[Bob]   + ack {msg_type} ({call_id})");
                    steps += 1;
                }
                Event::CallError { error, .. } => anyhow::bail!("[Bob] call error: {error}"),
                _ => {}
            },
        }
    }

    println!("\nsession states: {:?}", call_manager.sessions().iter().map(|(k, s)| (k.clone(), s.state)).collect::<Vec<_>>());
    assert!(steps >= target_steps, "signaling sequence did not complete (steps={steps})");
    println!("\n=== Phase 4 OK: full signaling exchange over libp2p ===");
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
