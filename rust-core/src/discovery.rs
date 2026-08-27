//! Peer discovery bookkeeping.
//!
//! The actual discovery protocols (Kademlia DHT, mDNS) run inside the
//! [`crate::network::Peer`]. This module consumes the peer's event stream and
//! keeps an up-to-date, queriable view of discovered peers and their
//! addresses — the "peer directory" the UI renders.

use crate::events::{Event, PeerInfo};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;

/// Tracks peers discovered through any mechanism.
#[derive(Debug, Default)]
pub struct DiscoveryManager {
    /// peer_id -> peer info (from Identify).
    identified: HashMap<PeerId, PeerInfo>,
    /// peer_id -> addresses registered via Kademlia/mDNS (not yet identified).
    addresses: HashMap<PeerId, Vec<Multiaddr>>,
}

impl DiscoveryManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a network event; updates the discovered-peers view.
    pub fn on_event(&mut self, ev: &Event) {
        match ev {
            Event::PeerIdentified(info) => {
                self.addresses
                    .insert(info.peer_id, info.addresses.clone());
                self.identified.insert(info.peer_id, info.clone());
            }
            Event::PeerDiscovered { peer_id } | Event::PeerConnected { peer_id } => {
                // Ensure the peer appears even before Identify answers.
                self.addresses.entry(*peer_id).or_default();
            }
            _ => {}
        }
    }

    /// Manually register an address for a peer (e.g. pasted multiaddr).
    pub fn add_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
        let addrs = self.addresses.entry(peer_id).or_default();
        if !addrs.contains(&addr) {
            addrs.push(addr);
        }
    }

    /// All known peer IDs.
    pub fn known_peers(&self) -> Vec<PeerId> {
        let mut peers: Vec<PeerId> = self.addresses.keys().copied().collect();
        for id in self.identified.keys() {
            if !peers.contains(id) {
                peers.push(*id);
            }
        }
        peers
    }

    /// Full info for a peer if Identify has answered.
    pub fn get(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.identified.get(peer_id)
    }

    /// All known addresses for a peer (Identify + manual registration).
    pub fn addresses_for(&self, peer_id: &PeerId) -> Vec<Multiaddr> {
        let mut addrs = self.addresses.get(peer_id).cloned().unwrap_or_default();
        if let Some(info) = self.identified.get(peer_id) {
            for a in &info.addresses {
                if !addrs.contains(a) {
                    addrs.push(a.clone());
                }
            }
        }
        addrs
    }

    /// Number of distinct peers known.
    pub fn len(&self) -> usize {
        self.known_peers().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn tracks_identified_peers() {
        let mut mgr = DiscoveryManager::new();
        let pid = peer_id();
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().unwrap();
        mgr.on_event(&Event::PeerIdentified(PeerInfo {
            peer_id: pid,
            addresses: vec![addr.clone()],
            client_version: Some("test".into()),
            protocol_version: Some("test".into()),
        }));
        assert_eq!(mgr.known_peers(), vec![pid]);
        assert_eq!(mgr.addresses_for(&pid), vec![addr]);
        assert!(mgr.get(&pid).is_some());
    }

    #[test]
    fn manual_address_registration() {
        let mut mgr = DiscoveryManager::new();
        let pid = peer_id();
        let addr: Multiaddr = "/ip4/9.9.9.9/tcp/5555".parse().unwrap();
        mgr.add_address(pid, addr.clone());
        assert_eq!(mgr.addresses_for(&pid), vec![addr]);
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn no_duplicate_addresses() {
        let mut mgr = DiscoveryManager::new();
        let pid = peer_id();
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().unwrap();
        mgr.add_address(pid, addr.clone());
        mgr.add_address(pid, addr);
        assert_eq!(mgr.addresses_for(&pid).len(), 1);
    }
}
