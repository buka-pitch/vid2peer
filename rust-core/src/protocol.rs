//! Application protocol definitions.
//!
//! These are the strongly-typed message structures exchanged over libp2p.
//! Messages are serialized as CBOR (via the request-response `cbor` codec)
//! for compact binary transport.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Protocol id for the text-chat request-response protocol.
pub const CHAT_PROTOCOL: &str = "/p2p-video-chat/chat/1.0.0";
/// Protocol id for the call-signaling request-response protocol.
pub const CALL_PROTOCOL: &str = "/p2p-video-chat/call/1.0.0";
/// Protocol id used for the libp2p identify behaviour.
pub const IDENTIFY_PROTOCOL: &str = "/p2p-video-chat/identify/1.0.0";
/// Maximum allowed size of a chat message (bytes).
pub const MAX_CHAT_MESSAGE_BYTES: usize = 8192;
/// Maximum allowed size of a single signaling message (bytes).
pub const MAX_SIGNALING_MESSAGE_BYTES: usize = 256 * 1024;

/// A helper to produce a millisecond unix timestamp.
pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Text chat
// ---------------------------------------------------------------------------

/// A text chat message. The video path is kept entirely separate from this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Globally (probabilistically) unique message id.
    pub id: String,
    /// Peer ID of the sender.
    pub from: String,
    /// Peer ID of the intended recipient (for 1:1 chat).
    pub to: String,
    /// UTF-8 message body.
    pub text: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn new(from: &str, to: &str, text: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            text,
            timestamp: now_ts(),
        }
    }

    /// Simple input validation performed on the receiving side.
    pub fn is_valid(&self) -> bool {
        !self.from.is_empty()
            && !self.to.is_empty()
            && !self.text.is_empty()
            && self.text.len() <= MAX_CHAT_MESSAGE_BYTES
            && self.id.len() <= 128
    }
}

/// Acknowledgement for a delivered chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAck {
    pub ok: bool,
    pub message_id: Option<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Call signaling
// ---------------------------------------------------------------------------

/// Metadata about a call included in the initial request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallMetadata {
    /// Media kinds offered, e.g. `["audio", "video"]`.
    pub media: Vec<String>,
    /// Human readable display name.
    pub display_name: Option<String>,
}

/// Messages exchanged by the call-signaling protocol (`/call/1.0.0`).
///
/// Each variant is delivered as a request; the receiving side replies with a
/// [`CallAck`]. This matches the flow: SDP offer, SDP answer and ICE
/// candidates are routed through libp2p, but the actual media never is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallMessage {
    /// Alice wants to call Bob.
    CallRequest {
        call_id: String,
        from: String,
        to: String,
        timestamp: u64,
        metadata: CallMetadata,
    },
    /// Bob accepts the call.
    CallAccepted { call_id: String, from: String },
    /// Bob rejects the call.
    CallRejected {
        call_id: String,
        from: String,
        reason: Option<String>,
    },
    /// Either party hangs up.
    CallEnded { call_id: String, from: String },
    /// SDP offer (WebRTC).
    SdpOffer {
        call_id: String,
        from: String,
        sdp: String,
    },
    /// SDP answer (WebRTC).
    SdpAnswer {
        call_id: String,
        from: String,
        sdp: String,
    },
    /// An ICE candidate (WebRTC).
    IceCandidate {
        call_id: String,
        from: String,
        candidate: String,
        /// Optional SDP media stream identification.
        mid: Option<String>,
    },
}

impl CallMessage {
    /// The call this message belongs to.
    pub fn call_id(&self) -> &str {
        match self {
            Self::CallRequest { call_id, .. }
            | Self::CallAccepted { call_id, .. }
            | Self::CallRejected { call_id, .. }
            | Self::CallEnded { call_id, .. }
            | Self::SdpOffer { call_id, .. }
            | Self::SdpAnswer { call_id, .. }
            | Self::IceCandidate { call_id, .. } => call_id,
        }
    }

    /// The peer that created/sent this message.
    pub fn from(&self) -> &str {
        match self {
            Self::CallRequest { from, .. }
            | Self::CallAccepted { from, .. }
            | Self::CallRejected { from, .. }
            | Self::CallEnded { from, .. }
            | Self::SdpOffer { from, .. }
            | Self::SdpAnswer { from, .. }
            | Self::IceCandidate { from, .. } => from,
        }
    }

    /// Short human-readable type tag used for logging / observability.
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::CallRequest { .. } => "call_request",
            Self::CallAccepted { .. } => "call_accepted",
            Self::CallRejected { .. } => "call_rejected",
            Self::CallEnded { .. } => "call_ended",
            Self::SdpOffer { .. } => "sdp_offer",
            Self::SdpAnswer { .. } => "sdp_answer",
            Self::IceCandidate { .. } => "ice_candidate",
        }
    }

    /// Basic sanity validation on the receiving side.
    pub fn is_valid(&self) -> bool {
        let call_id_ok = !self.call_id().is_empty() && self.call_id().len() <= 128;
        let from_ok = !self.from().is_empty() && self.from().len() <= 256;
        call_id_ok && from_ok
    }
}

/// Acknowledgement for a signaling message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallAck {
    pub ok: bool,
    pub error: Option<String>,
}

impl CallAck {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_roundtrip_cbor() {
        let msg = ChatMessage::new("12D3KooWpeerA", "12D3KooWpeerB", "hello".into());
        let mut buf = Vec::new();
        ciborium::into_writer(&msg, &mut buf).unwrap();
        let decoded: ChatMessage = ciborium::from_reader(buf.as_slice()).unwrap();
        assert_eq!(decoded, msg);
        assert!(decoded.is_valid());
    }

    #[test]
    fn call_message_roundtrip_cbor() {
        let msg = CallMessage::SdpOffer {
            call_id: "call-1".into(),
            from: "12D3KooWpeerA".into(),
            sdp: "v=0\r\no=- 1 2 IN IP4 0.0.0.0".into(),
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&msg, &mut buf).unwrap();
        let decoded: CallMessage = ciborium::from_reader(buf.as_slice()).unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(decoded.call_id(), "call-1");
        assert_eq!(decoded.type_tag(), "sdp_offer");
        assert!(decoded.is_valid());
    }

    #[test]
    fn invalid_call_message_rejected() {
        let msg = CallMessage::CallEnded {
            call_id: String::new(),
            from: "peer".into(),
        };
        assert!(!msg.is_valid());
    }
}
