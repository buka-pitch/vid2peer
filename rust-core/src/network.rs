//! The peer networking core.
//!
//! Wraps a libp2p `Swarm` and exposes a simplified, strongly-typed API for
//! the application layer. It combines the following behaviours:
//!
//! * **Identify**  – exchange peer IDs and announced addresses
//! * **Ping**      – liveness + RTT observability
//! * **Kademlia**  – decentralized discovery / DHT
//! * **mDNS**      – local-network discovery
//! * **request-response (chat)** – `/chat/1.0.0` text messages (CBOR)
//! * **request-response (call)** – `/call/1.0.0` WebRTC signaling (CBOR)
//! * **Circuit relay client**   – fallback connectivity through relays
//! * **DCUtR**     – direct connection upgrade through relayed connections
//!
//! The layer never handles media. It only discovers peers and exchanges
//! signaling/chat messages.

use crate::events::{Event, PeerInfo};
use crate::identity;
use crate::protocol::{
    CallAck, CallMessage, ChatAck, ChatMessage, CALL_PROTOCOL, CHAT_PROTOCOL,
};
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use libp2p::request_response::{cbor, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::{
    dial_opts::DialOpts, NetworkBehaviour, Swarm, SwarmEvent, StreamProtocol,
};
use libp2p::{
    dcutr, identify, identity::Keypair, kad, mdns, multiaddr::Protocol as MProtocol, noise, ping,
    request_response, tcp, yamux, Multiaddr, PeerId,
};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;
/// Identity of a bootstrap / relay node as configured by the user.
#[derive(Debug, Clone)]
pub struct BootstrapPeer {
    pub peer_id: PeerId,
    pub address: Multiaddr,
}

/// Configuration for [`Peer`].
#[derive(Debug, Clone)]
pub struct P2pConfig {
    /// Multiaddrs to listen on, e.g. `/ip4/0.0.0.0/tcp/4001`.
    pub listen_addrs: Vec<Multiaddr>,
    /// Bootstrap peers used to enter the DHT.
    pub bootstrap_peers: Vec<BootstrapPeer>,
    /// Where to persist the identity keypair.
    pub identity_file: Option<PathBuf>,
    /// Relay servers to reserve a circuit on at startup.
    pub relay_servers: Vec<BootstrapPeer>,
    /// The protocol version reported to Identify.
    pub protocol_version: String,
    /// The agent version reported to Identify.
    pub agent_version: String,
    /// Enable mDNS local discovery (no-op on non-local networks).
    pub enable_mdns: bool,
    /// Connection idle timeout.
    pub idle_connection_timeout: Duration,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen_addrs: vec![
                "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
            ],
            bootstrap_peers: Vec::new(),
            identity_file: None,
            relay_servers: Vec::new(),
            protocol_version: crate::protocol::IDENTIFY_PROTOCOL.to_string(),
            agent_version: format!("p2p-video-chat/{}", env!("CARGO_PKG_VERSION")),
            enable_mdns: true,
            idle_connection_timeout: Duration::from_secs(120),
        }
    }
}

/// The combined network behaviour of a peer.
#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    chat: cbor::Behaviour<ChatMessage, ChatAck>,
    call: cbor::Behaviour<CallMessage, CallAck>,
    relay: libp2p::relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

/// Type alias for an outbound request id of the call behaviour.
pub type CallRequestId = libp2p::request_response::OutboundRequestId;

/// A single P2P peer.
pub struct Peer {
    config: P2pConfig,
    keypair: Keypair,
    peer_id: PeerId,
    swarm: Swarm<Behaviour>,
    /// Pending outbound chat messages per destination (FIFO).
    ///
    /// A request-response reply does not carry the original message id, so we
    /// correlate responses to sent messages by per-peer delivery order.
    chat_outbound: HashMap<PeerId, VecDeque<(OutboundRequestId, String)>>,
    /// Map outbound call request id -> (destination, call id, message tag).
    call_outbound: HashMap<OutboundRequestId, (PeerId, String, String)>,
}

impl Peer {
    /// Build a peer. A Tokio runtime must be active.
    pub fn new(config: P2pConfig) -> Result<Self> {
        let keypair = match &config.identity_file {
            Some(path) => identity::load_or_create_keypair(path)?,
            None => identity::generate_keypair(),
        };
        Self::new_with_keypair(config, keypair)
    }

    /// Build a peer from an explicit keypair.
    pub fn new_with_keypair(config: P2pConfig, keypair: Keypair) -> Result<Self> {
        let peer_id = keypair.public().to_peer_id();
        tracing::debug!(%peer_id, "building peer");

        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::new(),
                noise::Config::new,
                yamux::Config::default,
            )
            .with_context(|| "failed to build TCP transport")?
            .with_relay_client(noise::Config::new, yamux::Config::default)
            .with_context(|| "failed to build relay transport")?
            .with_behaviour(|key, relay_behaviour| {
                let peer_id = key.public().to_peer_id();

                let identify = identify::Behaviour::new(identify::Config::new(
                    config.protocol_version.clone(),
                    key.public(),
                ));

                let kademlia = {
                    let mut k = kad::Behaviour::new(
                        peer_id,
                        kad::store::MemoryStore::new(peer_id),
                    );
                    k.set_mode(Some(kad::Mode::Server));
                    k
                };

                let chat = cbor::Behaviour::new(
                    [(StreamProtocol::new(CHAT_PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default(),
                );
                let call = cbor::Behaviour::new(
                    [(StreamProtocol::new(CALL_PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default(),
                );
                let dcutr = dcutr::Behaviour::new(peer_id);

                let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

                Ok(Behaviour {
                    identify,
                    ping: ping::Behaviour::default(),
                    kademlia,
                    chat,
                    call,
                    relay: relay_behaviour,
                    dcutr,
                    mdns,
                })
            })
            .with_context(|| "failed to build behaviour")?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(config.idle_connection_timeout)
            })
            .build();

        // Apply configuration: listen addresses, bootstrap, relay reservations.
        let mut peer = Self {
            config,
            keypair,
            peer_id,
            swarm,
            chat_outbound: HashMap::new(),
            call_outbound: HashMap::new(),
        };        peer.apply_startup_config()?;
        Ok(peer)
    }

    fn apply_startup_config(&mut self) -> Result<()> {
        let cfg = self.config.clone();

        for addr in &cfg.listen_addrs {
            self.swarm
                .listen_on(addr.clone())
                .with_context(|| format!("failed to listen on {addr}"))?;
        }

        // Register bootstrap peers in Kademlia and dial them.
        let mut any_known = false;
        for bp in &cfg.bootstrap_peers {
            self.swarm
                .behaviour_mut()
                .kademlia
                .add_address(&bp.peer_id, bp.address.clone());
            any_known = true;
            let _ = self.swarm.dial(
                DialOpts::peer_id(bp.peer_id)
                    .addresses(vec![bp.address.clone()])
                    .build(),
            );
            tracing::info!(peer = %bp.peer_id, addr = %bp.address, "dialing bootstrap peer");
        }
        // Track bootstrap peers as known for kademlia.
        if any_known {
            let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
        }

        // Reserve a circuit on configured relay servers so other peers can
        // reach us through the relay.
        for r in &cfg.relay_servers {
            self.swarm
                .behaviour_mut()
                .kademlia
                .add_address(&r.peer_id, r.address.clone());
            let _ = self.swarm.dial(
                DialOpts::peer_id(r.peer_id)
                    .addresses(vec![r.address.clone()])
                    .build(),
            );
            tracing::info!(relay = %r.peer_id, addr = %r.address, "dialing relay server");
        }

        Ok(())
    }

    // -- Getters -------------------------------------------------------------

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Current local listen addresses, each suffixed with `/p2p/<peer-id>` so
    /// they can be shared directly with other peers.
    pub fn local_addresses(&self) -> Vec<Multiaddr> {
        self.swarm
            .listeners()
            .map(|a| a.clone().with(MProtocol::P2p(self.peer_id)))
            .collect()
    }

    /// Number of currently connected peers.
    pub fn connection_count(&self) -> usize {
        self.swarm.network_info().num_peers()
    }

    // -- Event loop ----------------------------------------------------------

    /// Poll the swarm and translate the next libp2p event into a domain event.
    ///
    /// This drives the entire networking stack and MUST be awaited
    /// continuously (e.g. in a loop) for the peer to make progress.
    pub async fn next_event(&mut self) -> Event {
        loop {
            let event = self.swarm.select_next_some().await;
            if let Some(translated) = self.translate_event(event) {
                return translated;
            }
        }
    }

    fn translate_event(&mut self, event: SwarmEvent<BehaviourEvent>) -> Option<Event> {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!(%address, "listening");
                Some(Event::ListeningOn { address })
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                tracing::info!(%address, "listener expired");
                None
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                tracing::debug!(%peer_id, "connection established");
                Some(Event::PeerConnected { peer_id })
            }
            SwarmEvent::ConnectionClosed {
                peer_id, num_established, ..
            } => {
                if num_established == 0 {
                    tracing::debug!(%peer_id, "connection closed");
                    Some(Event::PeerDisconnected { peer_id })
                } else {
                    None
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                tracing::debug!(error = %error, "outgoing connection error");
                Some(Event::ConnectionError {
                    peer_id,
                    address: None,
                    error: error.to_string(),
                })
            }
            SwarmEvent::IncomingConnectionError { error, .. } => {
                tracing::debug!(error = %error, "incoming connection error");
                Some(Event::ConnectionError {
                    peer_id: None,
                    address: None,
                    error: error.to_string(),
                })
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(ev)) => match ev {
                identify::Event::Received {
                    peer_id, info, ..
                } => {
                    let info = PeerInfo {
                        peer_id,
                        addresses: info.listen_addrs.clone(),
                        client_version: Some(info.agent_version.clone()),
                        protocol_version: Some(info.protocol_version.clone()),
                    };
                    // Add the identified addresses to Kademlia so they can be
                    // re-discovered later.
                    for addr in &info.addresses {
                        self.swarm
                            .behaviour_mut()
                            .kademlia
                            .add_address(&peer_id, addr.clone());
                    }
                    Some(Event::PeerIdentified(info))
                }
                identify::Event::Error { .. } => None,
                _ => None,
            },
            SwarmEvent::Behaviour(BehaviourEvent::Ping(ev)) => match ev {
                ping::Event {
                    peer: peer_id,
                    result: Ok(rtt),
                    ..
                } => Some(Event::PingResult { peer_id, rtt }),
                ping::Event {
                    peer: _,
                    result: Err(_),
                    ..
                } => None,
            },
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(ev)) => {
                self.translate_kademlia_event(ev)
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                let first = list.first().map(|(p, _)| *p).unwrap_or(self.peer_id);
                for (peer_id, addr) in list {
                    tracing::debug!(%peer_id, %addr, "mDNS discovered peer");
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                }
                Some(Event::PeerDiscovered { peer_id: first })
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, _) in list {
                    tracing::debug!(%peer_id, "mDNS peer expired");
                }
                None
            }
            SwarmEvent::Behaviour(BehaviourEvent::Chat(ev)) => self.translate_chat_event(ev),
            SwarmEvent::Behaviour(BehaviourEvent::Call(ev)) => self.translate_call_event(ev),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(ev)) => match ev {
                libp2p::relay::client::Event::ReservationReqAccepted {
                    relay_peer_id, ..
                } => Some(Event::RelayReservationAccepted { relay_peer_id }),
                libp2p::relay::client::Event::OutboundCircuitEstablished {
                    relay_peer_id, ..
                } => Some(Event::RelayedConnectionEstablished { relay_peer_id }),
                _ => None,
            },
            SwarmEvent::Behaviour(BehaviourEvent::Dcutr(ev)) => {
                if ev.result.is_ok() {
                    tracing::debug!(peer = %ev.remote_peer_id, "DCUtR direct connection established");
                } else {
                    tracing::debug!(peer = %ev.remote_peer_id, "DCUtR hole punch failed");
                }
                Some(Event::DcutrEvent {
                    peer_id: ev.remote_peer_id,
                })
            }
            SwarmEvent::Dialing { .. } => None,
            _ => None,
        }
    }

    fn translate_kademlia_event(&mut self, ev: kad::Event) -> Option<Event> {
        match ev {
            kad::Event::RoutingUpdated { peer, .. } => Some(Event::PeerDiscovered { peer_id: peer }),
            kad::Event::InboundRequest { .. } => None,
            kad::Event::RoutablePeer { peer, address } => {
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer, address.clone());
                Some(Event::PeerDiscovered { peer_id: peer })
            }
            kad::Event::PendingRoutablePeer { peer, .. }
            | kad::Event::UnroutablePeer { peer, .. } => Some(Event::PeerDiscovered { peer_id: peer }),
            kad::Event::ModeChanged { .. } => None,
            kad::Event::OutboundQueryProgressed { result, .. } => match result {
                kad::QueryResult::Bootstrap(Ok(_)) => {
                    let peers = self.swarm.network_info().num_peers();
                    Some(Event::BootstrapCompleted { peers })
                }
                kad::QueryResult::Bootstrap(Err(e)) => {
                    tracing::warn!(error = %e, "bootstrap query failed");
                    Some(Event::BootstrapCompleted { peers: 0 })
                }
                kad::QueryResult::GetClosestPeers(Ok(kad::GetClosestPeersOk { key, peers })) => {
                    let key = libp2p::PeerId::from_bytes(&key).unwrap_or(self.peer_id);
                    let peers = peers.into_iter().map(|p| p.peer_id).collect();
                    Some(Event::PeersFound { key, peers })
                }
                kad::QueryResult::GetClosestPeers(Err(_)) => None,
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(rec))) => {
                    Some(Event::DhtGetOk {
                        key: rec.record.key.to_vec(),
                        value: rec.record.value,
                    })
                }
                kad::QueryResult::GetRecord(Ok(
                    kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. },
                )) => None,
                kad::QueryResult::GetRecord(Err(e)) => {
                    Some(Event::DhtGetNotFound { key: e.key().to_vec() })
                }
                kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                    Some(Event::DhtPutOk { key: key.to_vec() })
                }
                kad::QueryResult::PutRecord(Err(_)) => None,
                kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                    Some(Event::DhtProviding { key: key.to_vec() })
                }
                kad::QueryResult::StartProviding(Err(_)) => None,
                kad::QueryResult::RepublishProvider(_) => None,
                kad::QueryResult::RepublishRecord(_) => None,
                kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                    providers,
                    ..
                })) => Some(Event::PeersFound {
                    key: self.peer_id,
                    peers: providers.into_iter().collect(),
                }),
                kad::QueryResult::GetProviders(Ok(
                    kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. },
                )) => None,
                kad::QueryResult::GetProviders(Err(_)) => None,
            },
        }
    }

    fn translate_chat_event(
        &mut self,
        ev: libp2p::request_response::Event<ChatMessage, ChatAck>,
    ) -> Option<Event> {
        use libp2p::request_response::{Event as RrEvent, Message as RrMessage};
        match ev {
            RrEvent::Message {
                peer,
                message: RrMessage::Request { request, channel, .. },
                ..
            } => {
                if request.is_valid() {
                    let ack = ChatAck {
                        ok: true,
                        message_id: Some(request.id.clone()),
                        error: None,
                    };
                    let _ = self
                        .swarm
                        .behaviour_mut()
                        .chat
                        .send_response(channel, ack);
                    Some(Event::ChatMessageReceived {
                        from: peer,
                        message: request,
                    })
                } else {
                    let _ = self.swarm.behaviour_mut().chat.send_response(
                        channel,
                        ChatAck {
                            ok: false,
                            message_id: None,
                            error: Some("invalid message rejected".into()),
                        },
                    );
                    None
                }
            }
            RrEvent::Message {
                peer,
                message: RrMessage::Response { response, .. },
                ..
            } => {
                if response.ok {
                    if let Some((_, mid)) = self.take_chat_outbound(&peer) {
                        Some(Event::ChatMessageDelivered {
                            to: peer,
                            message_id: response.message_id.unwrap_or(mid),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            RrEvent::OutboundFailure {
                peer, request_id, error, ..
            } => {
                let mid = self
                    .chat_outbound
                    .get_mut(&peer)
                    .and_then(|q| q.iter().position(|(id, _)| *id == request_id))
                    .and_then(|idx| self.chat_outbound.get_mut(&peer).and_then(|q| q.remove(idx)))
                    .map(|(_, mid)| mid)
                    .unwrap_or_default();
                Some(Event::ChatError {
                    to: peer,
                    message_id: mid,
                    error: error.to_string(),
                })
            }
            RrEvent::InboundFailure { .. } | RrEvent::ResponseSent { .. } => None,
        }
    }

    /// Pop the oldest pending chat message id for `peer` (FIFO).
    fn take_chat_outbound(&mut self, peer: &PeerId) -> Option<(OutboundRequestId, String)> {
        self.chat_outbound.get_mut(peer).and_then(|q| q.pop_front())
    }

    fn translate_call_event(
        &mut self,
        ev: libp2p::request_response::Event<CallMessage, CallAck>,
    ) -> Option<Event> {
        use libp2p::request_response::{Event as RrEvent, Message as RrMessage};
        match ev {
            RrEvent::Message {
                peer,
                message: RrMessage::Request { request, channel, .. },
                ..
            } => {
                if request.is_valid() {
                    let _ = self
                        .swarm
                        .behaviour_mut()
                        .call
                        .send_response(channel, CallAck::ok());
                    Some(Event::CallMessageReceived { from: peer, message: request })
                } else {
                    let _ = self.swarm.behaviour_mut().call.send_response(
                        channel,
                        CallAck::err("invalid signaling message rejected"),
                    );
                    None
                }
            }
            RrEvent::Message {
                peer,
                message: RrMessage::Response { response, .. },
                ..
            } => {
                if response.ok {
                    if let Some((to, call_id, tag)) = self.take_call_outbound(&peer) {
                        Some(Event::CallMessageSent { to, call_id, msg_type: tag })
                    } else {
                        None
                    }
                } else {
                    let msg = response.error.clone().unwrap_or_default();
                    Some(Event::CallError {
                        to: peer,
                        error: msg,
                    })
                }
            }
            RrEvent::OutboundFailure {
                peer, request_id, error, ..
            } => {
                let (_, _, tag) = self
                    .call_outbound
                    .remove(&request_id)
                    .unwrap_or((peer, String::new(), String::new()));
                let _ = tag;
                Some(Event::CallError {
                    to: peer,
                    error: error.to_string(),
                })
            }
            RrEvent::InboundFailure { .. } | RrEvent::ResponseSent { .. } => None,
        }
    }

    fn take_call_outbound(&mut self, peer: &PeerId) -> Option<(PeerId, String, String)> {
        let ids: Vec<libp2p::request_response::OutboundRequestId> = self
            .call_outbound
            .iter()
            .filter(|(_, (p, _, _))| p == peer)
            .map(|(k, _)| *k)
            .collect();
        let key = ids.first().copied()?;
        self.call_outbound.remove(&key)
    }

    // -- Actions -------------------------------------------------------------

    /// Dial an arbitrary multiaddr.
    pub fn dial(&mut self, addr: Multiaddr) -> Result<()> {
        self.swarm
            .dial(addr.clone())
            .with_context(|| format!("failed to dial {addr}"))?;
        Ok(())
    }

    /// Dial a known peer by Peer ID, optionally through a relay.
    ///
    /// `via_relay` accepts a relay Peer ID; the dial then targets
    /// `/p2p/<relay>/p2p-circuit/p2p/<peer>`.
    pub fn dial_peer(&mut self, peer: &PeerId, via_relay: Option<&PeerId>) -> Result<()> {
        let opts = match via_relay {
            Some(relay) => {
                let mut addr: Multiaddr = "/ipfs".parse().expect("valid prefix");
                addr = addr.with(MProtocol::P2p(*relay))
                    .with(MProtocol::P2pCircuit)
                    .with(MProtocol::P2p(*peer));
                DialOpts::peer_id(*peer).addresses(vec![addr]).build()
            }
            None => DialOpts::peer_id(*peer).condition(libp2p::swarm::dial_opts::PeerCondition::Always)
                .build(),
        };
        self.swarm
            .dial(opts)
            .with_context(|| format!("failed to dial peer {peer}"))?;
        Ok(())
    }

    // -- Chat ----------------------------------------------------------------

    /// Send a 1:1 chat message to `to`.
    pub fn send_chat(&mut self, to: &PeerId, text: String) -> Result<()> {
        if text.is_empty() || text.len() > crate::protocol::MAX_CHAT_MESSAGE_BYTES {
            bail!("chat message too large or empty");
        }
        let msg = ChatMessage::new(&self.peer_id.to_string(), &to.to_string(), text);
        let req_id = self
            .swarm
            .behaviour_mut()
            .chat
            .send_request(to, msg.clone());
        self.chat_outbound
            .entry(*to)
            .or_default()
            .push_back((req_id, msg.id.clone()));
        tracing::info!(to = %to, id = %msg.id, "chat message sent");
        Ok(())
    }

    // -- Call signaling ------------------------------------------------------

    /// Send a signaling message to `to` over `/call/1.0.0`.
    pub fn send_call_message(&mut self, to: &PeerId, msg: CallMessage) -> Result<()> {
        if !msg.is_valid() {
            bail!("invalid signaling message");
        }
        let call_id = msg.call_id().to_string();
        let tag = msg.type_tag().to_string();
        let req_id = self
            .swarm
            .behaviour_mut()
            .call
            .send_request(to, msg);
        self.call_outbound.insert(req_id, (*to, call_id, tag));
        Ok(())
    }

    // -- DHT -----------------------------------------------------------------

    /// Bootstrap the local Kademlia routing table (start discovery).
    pub fn bootstrap(&mut self) -> Result<()> {
        self.swarm
            .behaviour_mut()
            .kademlia
            .bootstrap()
            .map(|_| ())
            .with_context(|| "no known peers to bootstrap from")
    }

    /// Query the DHT for the closest peers to `key` (Peer ID).
    pub fn get_closest_peers(&mut self, key: PeerId) {
        self.swarm.behaviour_mut().kademlia.get_closest_peers(key);
    }

    /// Find the addresses of a peer through the DHT.
    pub fn find_peer(&mut self, peer: &PeerId) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .get_closest_peers(*peer);
    }

    /// Manually register an address for a peer in the routing table.
    pub fn add_peer_address(&mut self, peer: &PeerId, addr: Multiaddr) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .add_address(peer, addr);
    }

    /// Store a (key, value) record in the DHT.
    pub fn put_record(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let record = kad::Record {
            key: kad::RecordKey::new(&key),
            value,
            publisher: None,
            expires: None,
        };
        self.swarm
            .behaviour_mut()
            .kademlia
            .put_record(record, kad::Quorum::One)
            .map(|_| ())
            .map_err(anyhow::Error::msg)
    }

    /// Fetch a record from the DHT.
    pub fn get_record(&mut self, key: Vec<u8>) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .get_record(kad::RecordKey::new(&key));
    }

    /// Advertise that this node provides `key` (provider record).
    pub fn start_providing(&mut self, key: Vec<u8>) -> Result<()> {
        self.swarm
            .behaviour_mut()
            .kademlia
            .start_providing(kad::RecordKey::new(&key))
            .map(|_| ())
            .map_err(anyhow::Error::msg)
    }

    /// Find providers for `key`.
    pub fn get_providers(&mut self, key: Vec<u8>) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .get_providers(kad::RecordKey::new(&key));
    }

    /// Current number of peers in the Kademlia routing table.
    pub fn routing_table_size(&mut self) -> usize {
        self.swarm
            .behaviour_mut()
            .kademlia
            .kbuckets()
            .map(|k| k.num_entries())
            .sum()
    }
}
