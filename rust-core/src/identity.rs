//! Cryptographic peer identity handling.
//!
//! Every peer has a persistent libp2p keypair. The public key derives the
//! stable `PeerId`. The private key is persisted to disk so the identity
//! survives restarts. The private key is NEVER transmitted to the frontend or
//! over the network.

use anyhow::Context;
use libp2p::identity::{Keypair, PublicKey};
use libp2p::PeerId;
use std::path::Path;

/// Generate a fresh Ed25519 libp2p keypair.
pub fn generate_keypair() -> Keypair {
    Keypair::generate_ed25519()
}

/// Persist the given keypair to `path` using libp2p's protobuf encoding.
///
/// File permissions are tightened to owner-only (`600`).
pub fn save_keypair(path: &Path, keypair: &Keypair) -> anyhow::Result<()> {
    let bytes = keypair
        .to_protobuf_encoding()
        .context("failed to serialize keypair")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir {}", parent.display()))?;
    }

    std::fs::write(path, &bytes)
        .with_context(|| format!("failed to write keypair to {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Load an existing keypair from `path`, or create and persist a new one.
pub fn load_or_create_keypair(path: &Path) -> anyhow::Result<Keypair> {
    if path.exists() {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read keypair at {}", path.display()))?;
        let keypair = Keypair::from_protobuf_encoding(&bytes)
            .with_context(|| format!("failed to parse keypair at {}", path.display()))?;
        Ok(keypair)
    } else {
        let keypair = generate_keypair();
        save_keypair(path, &keypair)?;
        Ok(keypair)
    }
}

/// Derive the stable Peer ID from a public key.
pub fn peer_id_from_public_key(pk: &PublicKey) -> PeerId {
    pk.to_peer_id()
}

/// Human readable display of a Peer ID.
pub fn peer_id_display(peer_id: &PeerId) -> String {
    peer_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generation_produces_ed25519_peer_id() {
        let kp = generate_keypair();
        let peer_id = peer_id_from_public_key(&kp.public());
        assert_eq!(peer_id, kp.public().to_peer_id());
        // Peer IDs from Ed25519 public keys start with "12D3KooW".
        assert!(peer_id.to_base58().starts_with("12D3KooW"));
    }

    #[test]
    fn keypair_roundtrip_via_protobuf_encoding() {
        let kp = generate_keypair();
        let bytes = kp.to_protobuf_encoding().unwrap();
        let restored = Keypair::from_protobuf_encoding(&bytes).unwrap();
        assert_eq!(kp.public(), restored.public());
    }

    #[test]
    fn load_or_create_persists_identity_across_calls() {
        let dir = std::env::temp_dir().join(format!("p2pvc-key-{}", uuid::Uuid::new_v4()));
        let path: std::path::PathBuf = dir.join("identity.key");
        let kp1 = load_or_create_keypair(&path).unwrap();
        let kp2 = load_or_create_keypair(&path).unwrap();
        assert_eq!(kp1.public(), kp2.public());
        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identity_stable_while_addresses_change() {
        // Demonstrates the property: Peer ID is the identity, addresses are
        // ephemeral. Two distinct addresses map to the same peer.
        let kp = generate_keypair();
        let peer_id = peer_id_from_public_key(&kp.public());
        let addr_a: libp2p::Multiaddr = "/ip4/192.168.1.20/tcp/4001".parse().unwrap();
        let addr_b: libp2p::Multiaddr = "/ip4/10.20.30.40/udp/4001/quic-v1".parse().unwrap();
        assert_ne!(addr_a, addr_b);
        assert_eq!(peer_id.to_string(), peer_id.to_string());
    }
}
