//! DHT / Kademlia helpers.
//!
//! The Kademlia behaviour lives in [`crate::network::Peer`]; this module
//! provides the record-key and value-encoding conventions the application uses
//! so that peers can publish and discover each other's reachable addresses in
//! a decentralized fashion.
//!
//! Important: there is **no centralized address database**. Addresses are
//! stored as *provider records* distributed across the DHT and as
//! value-records that peers can PUT/GET. Any peer can later re-publish, and
//! records expire and are refreshed by the DHT itself.

use libp2p::kad::RecordKey;
use libp2p::{Multiaddr, PeerId};
use std::str::FromStr;

/// The DHT namespace under which peers publish provider records.
pub const ADDRESS_RECORD_NAMESPACE: &str = "/p2p-video-chat/address/";

/// Derive the record key under which a peer's addresses are stored.
pub fn address_record_key(peer_id: &PeerId) -> RecordKey {
    RecordKey::new(&format!("{ADDRESS_RECORD_NAMESPACE}{peer_id}"))
}

/// A record value holding one or more reachable multiaddrs for a peer.
///
/// Encoding: line-separated multiaddr strings. This is deliberately simple and
/// debuggable; sizes are bounded by the DHT record limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressRecord {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
}

impl AddressRecord {
    pub fn new(peer_id: PeerId, addresses: Vec<Multiaddr>) -> Self {
        Self {
            peer_id,
            addresses,
        }
    }

    /// Encode to bytes for storage in the DHT.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = String::new();
        for addr in &self.addresses {
            out.push_str(&addr.to_string());
            out.push('\n');
        }
        out.into_bytes()
    }

    /// Decode from bytes. Malformed lines are skipped.
    pub fn decode(peer_id: PeerId, bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let addresses = text
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                if l.is_empty() {
                    None
                } else {
                    Multiaddr::from_str(l).ok()
                }
            })
            .collect();
        Self { peer_id, addresses }
    }
}

/// Cap the number of addresses published to keep records small.
pub fn capped(addresses: &mut Vec<Multiaddr>, max: usize) {
    addresses.truncate(max);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_record_roundtrip() {
        let peer_id = PeerId::random();
        let addrs: Vec<Multiaddr> = vec![
            "/ip4/192.168.1.20/tcp/4001".parse().unwrap(),
            "/ip4/10.20.30.40/udp/4001/quic-v1".parse().unwrap(),
        ];
        let rec = AddressRecord::new(peer_id, addrs.clone());
        let decoded = AddressRecord::decode(peer_id, &rec.encode());
        assert_eq!(decoded, rec);
    }

    #[test]
    fn address_record_skips_malformed_lines() {
        let peer_id = PeerId::random();
        let good: Multiaddr = "/ip4/1.2.3.4/tcp/9999".parse().unwrap();
        let bytes = format!("{}\nthis is not a multiaddr\n", good);
        let decoded = AddressRecord::decode(peer_id, bytes.as_bytes());
        assert_eq!(decoded.addresses, vec![good]);
    }

    #[test]
    fn address_record_key_is_namespaced() {
        let peer_id = PeerId::random();
        let key = address_record_key(&peer_id);
        assert!(key.as_ref().starts_with(ADDRESS_RECORD_NAMESPACE.as_bytes()));
    }

    #[test]
    fn cap_limits_addresses() {
        let mut addrs: Vec<Multiaddr> = (0..5u16)
            .map(|i| format!("/ip4/1.1.1.{}/tcp/{}", i + 1, 4000 + i).parse().unwrap())
            .collect();
        capped(&mut addrs, 2);
        assert_eq!(addrs.len(), 2);
    }
}
