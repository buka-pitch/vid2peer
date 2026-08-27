//! Networking events exposed to upper layers (UI / application layer / FFI).
//!
//! The networking layer is decoupled from WebRTC and the UI. It reports what
//! happened through this event type; consumers decide what to do (e.g. open a
//! WebRTC `RTCPeerConnection` after receiving `SdpOffer`).

use crate::protocol::{CallMessage, ChatMessage};
use libp2p::{Multiaddr, PeerId};
use std::time::Duration;

/// A snapshot of a peer known to the local node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    /// Addresses the peer announced via Identify / DHT.
    pub addresses: Vec<Multiaddr>,
    /// Known client version string.
    pub client_version: Option<String>,
    /// Known protocol version string.
    pub protocol_version: Option<String>,
}

/// Events emitted by the [`crate::network::Peer`].
#[derive(Debug, Clone)]
pub enum Event {
    // -- Connectivity -------------------------------------------------------
    /// A new listen address is active.
    ListeningOn { address: Multiaddr },
    /// A direct connection to a peer was established.
    PeerConnected { peer_id: PeerId },
    /// A connection to a peer was closed.
    PeerDisconnected { peer_id: PeerId },
    /// A dial attempt failed.
    ConnectionError {
        peer_id: Option<PeerId>,
        address: Option<Multiaddr>,
        error: String,
    },
    /// Identify protocol answered: peer id and its announced addresses.
    PeerIdentified(PeerInfo),

    // -- Ping ---------------------------------------------------------------
    /// A ping succeeded, reporting the round-trip time.
    PingResult { peer_id: PeerId, rtt: Duration },

    // -- DHT / discovery ----------------------------------------------------
    /// The node successfully bootstrapped into the DHT.
    BootstrapCompleted { peers: usize },
    /// Kademlia answered a closest-peers query.
    PeersFound { key: PeerId, peers: Vec<PeerId> },
    /// A peer was discovered (via Kademlia/mDNS) and added to the routing table.
    PeerDiscovered { peer_id: PeerId },
    /// `PutRecord` succeeded.
    DhtPutOk { key: Vec<u8> },
    /// `GetRecord` returned a value.
    DhtGetOk { key: Vec<u8>, value: Vec<u8> },
    /// `GetRecord` returned nothing.
    DhtGetNotFound { key: Vec<u8> },
    /// `StartProviding` succeeded.
    DhtProviding { key: Vec<u8> },
    /// The Kademlia routing table changed (size reporting).
    RoutingTableChanged { known_peers: usize },

    // -- Chat ---------------------------------------------------------------
    /// An incoming chat message from `from`.
    ChatMessageReceived { from: PeerId, message: ChatMessage },
    /// The remote peer acknowledged our chat message.
    ChatMessageDelivered { to: PeerId, message_id: String },
    /// An outgoing chat message failed.
    ChatError { to: PeerId, message_id: String, error: String },

    // -- Call signaling -----------------------------------------------------
    /// An incoming signaling message (CallRequest / SdpOffer / ICE ...).
    CallMessageReceived { from: PeerId, message: CallMessage },
    /// A signaling message we sent was acknowledged.
    CallMessageSent {
        to: PeerId,
        call_id: String,
        msg_type: String,
    },
    /// Sending a signaling message failed.
    CallError { to: PeerId, error: String },

    // -- Relay / NAT --------------------------------------------------------
    /// Relay circuit reservation accepted.
    RelayReservationAccepted { relay_peer_id: PeerId },
    /// A relayed (via circuit) connection was established.
    RelayedConnectionEstablished { relay_peer_id: PeerId },
    /// DC-UTRS hole punch outcome.
    DcutrEvent { peer_id: PeerId },
}

impl Event {
    /// Short tag used for structured logging.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::ListeningOn { .. } => "listening",
            Self::PeerConnected { .. } => "peer_connected",
            Self::PeerDisconnected { .. } => "peer_disconnected",
            Self::ConnectionError { .. } => "connection_error",
            Self::PeerIdentified(_) => "peer_identified",
            Self::PingResult { .. } => "ping",
            Self::BootstrapCompleted { .. } => "bootstrap",
            Self::PeersFound { .. } => "peers_found",
            Self::PeerDiscovered { .. } => "peer_discovered",
            Self::DhtPutOk { .. } => "dht_put_ok",
            Self::DhtGetOk { .. } => "dht_get_ok",
            Self::DhtGetNotFound { .. } => "dht_get_not_found",
            Self::DhtProviding { .. } => "dht_providing",
            Self::RoutingTableChanged { .. } => "routing_table",
            Self::ChatMessageReceived { .. } => "chat_in",
            Self::ChatMessageDelivered { .. } => "chat_delivered",
            Self::ChatError { .. } => "chat_error",
            Self::CallMessageReceived { .. } => "call_in",
            Self::CallMessageSent { .. } => "call_out",
            Self::CallError { .. } => "call_error",
            Self::RelayReservationAccepted { .. } => "relay_reservation",
            Self::RelayedConnectionEstablished { .. } => "relay_connection",
            Self::DcutrEvent { .. } => "dcutr",
        }
    }
}
