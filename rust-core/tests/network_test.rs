//! Phase 1 integration test: two peers connect, identify each other and ping.

mod common;

use p2p_video_chat_core::Event;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_peers_connect_identify_and_ping() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();

    let (mut a, a_addr) = common::new_peer().await;
    let (mut b, _b_addr) = common::new_peer().await;
    let a_id = a.peer_id();
    let b_id = b.peer_id();

    common::connect(&mut a, a_id, &mut b, &a_addr).await;

    let mut identified_a = false;
    let mut identified_b = false;
    let mut pinged = false;

    common::drive_until(
        &mut a,
        &mut b,
        Duration::from_secs(20),
        |ev| match ev {
            Event::PeerIdentified(info) => {
                if info.peer_id == b_id {
                    identified_a = true;
                } else if info.peer_id == a_id {
                    identified_b = true;
                }
                identified_a && identified_b && pinged
            }
            Event::PingResult { .. } => {
                pinged = true;
                pinged && identified_a && identified_b
            }
            _ => false,
        },
    )
    .await;

    assert!(identified_a, "A should have identified B");
    assert!(identified_b, "B should have identified A");
    assert!(pinged, "a ping result should have been received");
    assert!(a.connection_count() >= 1);
    assert!(b.connection_count() >= 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_identity_is_stable_across_restarts() {
    // Load-or-create with a temp file: identity must persist.
    let dir = std::env::temp_dir().join(format!("p2pvc-nt-{}", uuid::Uuid::new_v4()));
    let file = dir.join("id.key");

    let cfg1 = p2p_video_chat_core::P2pConfig {
        identity_file: Some(file.clone()),
        ..common::local_config()
    };
    let a = p2p_video_chat_core::Peer::new(cfg1).unwrap();
    let first = a.peer_id();

    let cfg2 = p2p_video_chat_core::P2pConfig {
        identity_file: Some(file.clone()),
        ..common::local_config()
    };
    let b = p2p_video_chat_core::Peer::new(cfg2).unwrap();
    let second = b.peer_id();

    assert_eq!(first, second, "identity must survive restart");

    let _ = std::fs::remove_dir_all(&dir);
}
