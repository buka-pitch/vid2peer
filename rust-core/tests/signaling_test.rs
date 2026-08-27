//! Phase 4 integration test: full WebRTC signaling exchange over `/call/1.0.0`.

mod common;

use p2p_video_chat_core::protocol::{CallMessage, CallMetadata};
use p2p_video_chat_core::signaling::{CallManager, CallSignaler};
use p2p_video_chat_core::Event;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_call_signaling_sequence() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();

    let (mut alice, a_addr) = common::new_peer().await;
    let (mut bob, _b_addr) = common::new_peer().await;
    let alice_id = alice.peer_id();
    let bob_id = bob.peer_id();

    common::connect(&mut alice, alice_id, &mut bob, &a_addr).await;

    let mut alice_calls = CallManager::new();
    let mut bob_calls = CallManager::new();

    // Alice starts a call.
    let mut metadata = CallMetadata::default();
    metadata.media = vec!["audio".into(), "video".into()];
    let req = CallSignaler::request(&alice_id, &bob_id, metadata.clone());
    let call_id = req.call_id().to_string();
    alice_calls.on_call_requested(&call_id, bob_id);
    alice.send_call_message(&bob_id, req.clone()).unwrap();

    let mut steps = 0u32;
    // expect: bob gets CallRequest; bob sends accepted; alice gets accepted;
    // alice sends offer; bob gets offer; bob sends answer; alice gets answer;
    // alice sends ice; bob gets ice.
    const EXPECTED: u32 = 9;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline && steps < EXPECTED {
        let ev = tokio::select! {
            ev = alice.next_event() => ev,
            ev = bob.next_event() => ev,
        };
        match ev {
            Event::CallMessageReceived { from, message } => {
                if from == bob_id {
                    // Alice received a message from Bob.
                    match message {
                        CallMessage::CallAccepted { call_id, .. } => {
                            alice_calls.on_call_accepted(&call_id);
                            steps += 1;
                            alice
                                .send_call_message(
                                    &from,
                                    CallSignaler::sdp_offer(&alice_id, &call_id, "v=0 o=a ...".into()),
                                )
                                .unwrap();
                            steps += 1; // alice -> offer
                        }
                        CallMessage::SdpAnswer { call_id, sdp, .. } => {
                            assert!(!sdp.is_empty());
                            alice_calls.on_sdp_exchange(&call_id);
                            steps += 1;
                            alice
                                .send_call_message(
                                    &from,
                                    CallSignaler::ice_candidate(
                                        &alice_id,
                                        &call_id,
                                        "candidate:1 1 UDP 1 192.168.0.1 5000 typ host".into(),
                                        Some("0".into()),
                                    ),
                                )
                                .unwrap();
                            steps += 1; // alice -> ice
                        }
                        _ => {}
                    }
                } else {
                    // Bob received a message from Alice.
                    match message {
                        CallMessage::CallRequest { call_id, metadata, .. } => {
                            bob_calls.on_call_request(&call_id, from, metadata.clone());
                            steps += 1;
                            bob.send_call_message(&from, CallSignaler::accepted(&bob_id, &call_id))
                                .unwrap();
                            steps += 1; // bob -> accepted
                        }
                        CallMessage::SdpOffer { call_id, sdp, .. } => {
                            assert!(!sdp.is_empty());
                            bob_calls.on_sdp_exchange(&call_id);
                            steps += 1;
                            bob.send_call_message(
                                &from,
                                CallSignaler::sdp_answer(&bob_id, &call_id, "v=0 o=b ...".into()),
                            )
                            .unwrap();
                            steps += 1; // bob -> answer
                        }
                        CallMessage::IceCandidate { call_id, candidate, .. } => {
                            assert!(!candidate.is_empty());
                            bob_calls.on_sdp_exchange(&call_id);
                            let _ = call_id;
                            steps += 1;
                        }
                        _ => {}
                    }
                }
            }
            Event::CallError { error, .. } => panic!("call error: {error}"),
            _ => {}
        }
    }

    assert!(steps >= EXPECTED, "signaling sequence incomplete: {steps} steps");
    let caller_session = alice_calls
        .session(&call_id)
        .expect("call session should exist on caller side");
    assert_eq!(caller_session.role, p2p_video_chat_core::signaling::CallRole::Caller);
    let callee_session = bob_calls
        .session(&call_id)
        .expect("call session should exist on callee side");
    assert_eq!(callee_session.role, p2p_video_chat_core::signaling::CallRole::Callee);
}
