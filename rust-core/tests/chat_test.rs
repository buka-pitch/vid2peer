//! Phase 3 integration test: 1:1 chat message delivery over `/chat/1.0.0`.

mod common;

use p2p_video_chat_core::Event;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_message_is_delivered_and_acked() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();

    let (mut a, a_addr) = common::new_peer().await;
    let (mut b, _b_addr) = common::new_peer().await;
    let a_id = a.peer_id();
    let b_id = b.peer_id();

    common::connect(&mut a, a_id, &mut b, &a_addr).await;

    a.send_chat(&b_id, "hello bob".into()).unwrap();

    let mut received = false;
    let mut delivered = false;

    common::drive_until(
        &mut a,
        &mut b,
        Duration::from_secs(15),
        |ev| match ev {
            Event::ChatMessageReceived { from, message } => {
                assert_eq!(from, &a_id);
                assert_eq!(message.text, "hello bob");
                received = true;
                received && delivered
            }
            Event::ChatMessageDelivered { to, .. } => {
                assert_eq!(to, &b_id);
                delivered = true;
                received && delivered
            }
            _ => false,
        },
    )
    .await;

    assert!(received, "B should have received the message");
    assert!(delivered, "A should have received the delivery ack");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_rejects_oversized_messages_locally() {
    let (mut a, _) = common::new_peer().await;
    let (b, _) = common::new_peer().await;
    let b_id = b.peer_id();
    let too_long = "x".repeat(p2p_video_chat_core::protocol::MAX_CHAT_MESSAGE_BYTES + 1);
    assert!(a.send_chat(&b_id, too_long).is_err());
}
