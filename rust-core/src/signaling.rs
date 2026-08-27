//! Call signaling: message construction and session state machine.
//!
//! Signaling rides on the libp2p request-response protocol `/call/1.0.0`.
//! After signaling completes, media flows directly over WebRTC — never through
//! libp2p. This module is transport-agnostic: it turns `CallMessage`s and
//! session events into a small state machine that the UI / application layer
//! can drive.

use crate::protocol::{CallMessage, CallMetadata};
use crate::PeerId;
use std::collections::HashMap;

/// Role of the local peer in a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallRole {
    /// This peer initiated the call.
    Caller,
    /// This peer received the call.
    Callee,
}

/// Lifecycle state of a call session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    /// `CallRequest` sent or received; waiting for the callee's decision.
    Pending,
    /// Call accepted; negotiating SDP/ICE.
    Negotiating,
    /// SDP exchange complete; media should be flowing.
    InProgress,
    /// Call ended by either party.
    Ended,
    /// Call rejected by the callee.
    Rejected,
}

impl CallState {
    /// Whether the call is still active.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Negotiating | Self::InProgress)
    }
}

/// A single call session tracked by the local peer.
#[derive(Debug, Clone)]
pub struct CallSession {
    pub call_id: String,
    pub remote: PeerId,
    pub role: CallRole,
    pub state: CallState,
    pub metadata: CallMetadata,
}

impl CallSession {
    /// Construction of a call request message for the *caller* side.
    pub fn request_message(from: &PeerId, to: &PeerId, metadata: CallMetadata) -> CallMessage {
        CallMessage::CallRequest {
            call_id: uuid::Uuid::new_v4().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            timestamp: crate::protocol::now_ts(),
            metadata,
        }
    }
}

/// Tracks all calls the local peer is involved in.
///
/// Feed it the call-related events from the network layer and the UI. It does
/// NOT send messages itself; use [`CallSignaler`] for that.
#[derive(Debug, Default)]
pub struct CallManager {
    sessions: HashMap<String, CallSession>,
    /// call_id -> call messages received so far (for observability / tests).
    history: HashMap<String, Vec<CallMessage>>,
}

impl CallManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// An incoming `CallRequest` from `from`.
    pub fn on_call_request(&mut self, call_id: &str, from: PeerId, metadata: CallMetadata) {
        self.sessions.insert(
            call_id.to_string(),
            CallSession {
                call_id: call_id.to_string(),
                remote: from,
                role: CallRole::Callee,
                state: CallState::Pending,
                metadata,
            },
        );
    }

    /// The caller observes its own `CallRequest` being acknowledged.
    pub fn on_call_requested(&mut self, call_id: &str, to: PeerId) {
        self.sessions.entry(call_id.to_string()).or_insert(CallSession {
            call_id: call_id.to_string(),
            remote: to,
            role: CallRole::Caller,
            state: CallState::Pending,
            metadata: CallMetadata::default(),
        });
    }

    /// The callee accepted.
    pub fn on_call_accepted(&mut self, call_id: &str) {
        if let Some(s) = self.sessions.get_mut(call_id) {
            s.state = CallState::Negotiating;
        }
    }

    /// The callee rejected.
    pub fn on_call_rejected(&mut self, call_id: &str) {
        if let Some(s) = self.sessions.get_mut(call_id) {
            s.state = CallState::Rejected;
        }
    }

    /// Either party ended the call.
    pub fn on_call_ended(&mut self, call_id: &str) {
        if let Some(s) = self.sessions.get_mut(call_id) {
            s.state = CallState::Ended;
        }
    }

    /// SDP offer received / sent — move into negotiation.
    pub fn on_sdp_exchange(&mut self, call_id: &str) {
        if let Some(s) = self.sessions.get_mut(call_id) {
            if s.state == CallState::Pending {
                s.state = CallState::Negotiating;
            }
        }
    }

    /// Called by the application when the WebRTC connection is established.
    pub fn on_media_established(&mut self, call_id: &str) {
        if let Some(s) = self.sessions.get_mut(call_id) {
            s.state = CallState::InProgress;
        }
    }

    /// Record a received/sent call message for the session.
    pub fn record_message(&mut self, msg: CallMessage) {
        let call_id = msg.call_id().to_string();
        self.history.entry(call_id).or_default().push(msg);
    }

    pub fn session(&self, call_id: &str) -> Option<&CallSession> {
        self.sessions.get(call_id)
    }

    pub fn sessions(&self) -> &HashMap<String, CallSession> {
        &self.sessions
    }

    /// Active calls (pending/negotiating/in progress).
    pub fn active_calls(&self) -> Vec<&CallSession> {
        self.sessions.values().filter(|s| s.state.is_active()).collect()
    }
}

/// High-level helpers to construct outgoing signaling messages.
///
/// These are pure message factories; sending is done through
/// [`crate::network::Peer::send_call_message`].
pub struct CallSignaler;

impl CallSignaler {
    pub fn request(from: &PeerId, to: &PeerId, metadata: CallMetadata) -> CallMessage {
        CallSession::request_message(from, to, metadata)
    }

    pub fn accepted(peer: &PeerId, call_id: &str) -> CallMessage {
        CallMessage::CallAccepted {
            call_id: call_id.to_string(),
            from: peer.to_string(),
        }
    }

    pub fn rejected(peer: &PeerId, call_id: &str, reason: impl Into<String>) -> CallMessage {
        CallMessage::CallRejected {
            call_id: call_id.to_string(),
            from: peer.to_string(),
            reason: Some(reason.into()),
        }
    }

    pub fn hangup(peer: &PeerId, call_id: &str) -> CallMessage {
        CallMessage::CallEnded {
            call_id: call_id.to_string(),
            from: peer.to_string(),
        }
    }

    pub fn sdp_offer(peer: &PeerId, call_id: &str, sdp: String) -> CallMessage {
        CallMessage::SdpOffer {
            call_id: call_id.to_string(),
            from: peer.to_string(),
            sdp,
        }
    }

    pub fn sdp_answer(peer: &PeerId, call_id: &str, sdp: String) -> CallMessage {
        CallMessage::SdpAnswer {
            call_id: call_id.to_string(),
            from: peer.to_string(),
            sdp,
        }
    }

    pub fn ice_candidate(
        peer: &PeerId,
        call_id: &str,
        candidate: String,
        mid: Option<String>,
    ) -> CallMessage {
        CallMessage::IceCandidate {
            call_id: call_id.to_string(),
            from: peer.to_string(),
            candidate,
            mid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn peer_a() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }
    fn peer_b() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn full_call_lifecycle_as_callee() {
        let mut mgr = CallManager::new();
        let from = peer_a();
        let req = CallSignaler::request(&from, &peer_b(), CallMetadata::default());
        let call_id = req.call_id().to_string();
        mgr.on_call_request(&call_id, from, CallMetadata::default());
        mgr.on_call_accepted(&call_id);
        mgr.on_sdp_exchange(&call_id);
        mgr.on_media_established(&call_id);

        let session = mgr.session(&call_id).unwrap();
        assert_eq!(session.role, CallRole::Callee);
        assert_eq!(session.state, CallState::InProgress);
        assert!(session.state.is_active());
    }

    #[test]
    fn caller_sees_rejection() {
        let mut mgr = CallManager::new();
        let to = peer_b();
        let req = CallSignaler::request(&peer_a(), &to, CallMetadata::default());
        let call_id = req.call_id().to_string();
        mgr.on_call_requested(&call_id, to);
        mgr.on_call_rejected(&call_id);

        let session = mgr.session(&call_id).unwrap();
        assert_eq!(session.role, CallRole::Caller);
        assert_eq!(session.state, CallState::Rejected);
        assert!(!session.state.is_active());
    }

    #[test]
    fn active_calls_filters() {
        let mut mgr = CallManager::new();
        let req = CallSignaler::request(&peer_a(), &peer_b(), CallMetadata::default());
        let call_id = req.call_id().to_string();
        mgr.on_call_requested(&call_id, peer_b());
        mgr.on_call_accepted(&call_id);
        assert_eq!(mgr.active_calls().len(), 1);
        mgr.on_call_ended(&call_id);
        assert_eq!(mgr.active_calls().len(), 0);
    }

    #[test]
    fn message_factories_are_valid() {
        let offer = CallSignaler::sdp_offer(&peer_a(), "call-1", "v=0 ...".into());
        assert_eq!(offer.call_id(), "call-1");
        assert!(offer.is_valid());
        let candidate = CallSignaler::ice_candidate(
            &peer_b(),
            "call-1",
            "candidate:1 1 UDP 2122260223 192.168.0.10 40000 typ host".into(),
            Some("0".into()),
        );
        assert!(candidate.is_valid());
    }
}
