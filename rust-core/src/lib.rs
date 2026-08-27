//! # p2p-video-chat-core
//!
//! Rust networking core for a decentralized peer-to-peer video/audio calling
//! application.
//!
//! Layer separation (see `.agent.md` §18):
//!
//! ```text
//! React / Android UI
//!         │
//! Application layer  (calls / chat / users)
//!         │
//! libp2p networking   (this crate: discovery / DHT / signaling)
//!         │
//! WebRTC              (audio / video, browser or native, NOT this crate)
//! ```
//!
//! This crate:
//!
//! * generates and persists a cryptographic peer identity ([`identity`])
//! * runs a libp2p `Swarm` with Identify, Ping, Kademlia DHT, mDNS,
//!   chat and call request-response protocols, relay client and DCUtR
//!   ([`network`])
//! * exposes a typed event stream ([`events`])
//! * provides higher-level helpers for discovery ([`discovery`]), DHT record
//!   addressing ([`dht`]), call-signaling state ([`signaling`]) and relay
//!   handling ([`relay`])
//!
//! The actual audio/video media never passes through libp2p: after signaling
//! completes, media flows directly over WebRTC.

pub mod dht;
pub mod discovery;
pub mod events;
pub mod identity;
pub mod network;
pub mod protocol;
pub mod relay;
pub mod signaling;

pub use events::{Event, PeerInfo};
pub use libp2p::{Multiaddr, PeerId};
pub use network::{BootstrapPeer, P2pConfig, Peer};
