//! Circuit relay support.
//!
//! A relay is used ONLY when a direct WebRTC/libp2p connection cannot be
//! established (NAT/firewall/CGNAT). The relay forwards encrypted packets; it
//! never decodes, processes or stores media.
//!
//! With libp2p circuit-relay v2:
//!
//! * a **relay server** (`relay-node`) accepts reservations and forwards
//!   traffic between peers that cannot reach each other directly;
//! * a **relay client** (every ordinary peer) reserves circuits and dials
//!   peers through the relay.
//!
//! This module only deals with address construction and configuration — the
//! actual relay behaviour runs inside the [`crate::network::Peer`] swarm.

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use std::str::FromStr;

/// Configuration for using a relay server.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Peer ID of the relay server.
    pub relay_peer: PeerId,
    /// Address of the relay server (must end with `/p2p/<relay-peer>`).
    pub address: Multiaddr,
}

impl RelayConfig {
    /// Build config from a full multiaddr that includes `/p2p/<relay-peer>`.
    pub fn from_multiaddr(addr: &str) -> Option<Self> {
        let addr = Multiaddr::from_str(addr).ok()?;
        let mut peer = None;
        for p in addr.iter() {
            if let Protocol::P2p(id) = p {
                peer = Some(id);
            }
        }
        Some(Self {
            relay_peer: peer?,
            address: addr,
        })
    }

    /// The address a peer should dial to reach *us* via this relay.
    ///
    /// `/p2p/<relay>/p2p-circuit`
    pub fn relay_listen_address(&self) -> Multiaddr {
        let mut a = Multiaddr::empty();
        a.push(Protocol::P2p(self.relay_peer));
        a.push(Protocol::P2pCircuit);
        a
    }

    /// Address to dial a specific target through this relay.
    ///
    /// `/p2p/<relay>/p2p-circuit/p2p/<target>`
    pub fn dial_through_relay(&self, target: &PeerId) -> Multiaddr {
        let mut a = Multiaddr::empty();
        a.push(Protocol::P2p(self.relay_peer));
        a.push(Protocol::P2pCircuit);
        a.push(Protocol::P2p(*target));
        a
    }
}

/// Returns true if the address is a relayed (`/p2p-circuit`) address.
pub fn is_circuit_addr(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::P2pCircuit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn relay_config_from_multiaddr() {
        let relay = peer_id();
        let cfg = RelayConfig::from_multiaddr(&format!(
            "/ip4/1.2.3.4/tcp/4002/p2p/{relay}"
        ))
        .unwrap();
        assert_eq!(cfg.relay_peer, relay);
    }

    #[test]
    fn relay_listen_and_dial_addresses() {
        let relay = peer_id();
        let target = peer_id();
        let cfg = RelayConfig {
            relay_peer: relay,
            address: format!("/ip4/1.2.3.4/tcp/4002/p2p/{relay}").parse().unwrap(),
        };
        let listen = cfg.relay_listen_address();
        let dial = cfg.dial_through_relay(&target);
        assert!(is_circuit_addr(&listen));
        assert!(is_circuit_addr(&dial));
        assert!(listen.to_string().contains("p2p-circuit"));
        assert!(dial.to_string().contains("p2p-circuit"));
        assert!(dial.to_string().contains(&target.to_string()));
    }

    #[test]
    fn malformed_multiaddr_rejected() {
        assert!(RelayConfig::from_multiaddr("not-an-address").is_none());
    }
}
