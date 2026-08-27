# Architecture

Decentralized P2P video/audio calling between browsers and Android devices.

## Design rule

The three questions below are answered by three independent layers (`.agent.md` §18):

| Question | Answer |
| --- | --- |
| "Where can I find this peer?" | Kademlia DHT (`rust-core/src/dht.rs`) |
| "How can I communicate with this peer?" | libp2p (`rust-core/src/network.rs`) |
| "How do we exchange real-time audio/video?" | WebRTC (browser/native, **not** libp2p) |

**Media never passes through libp2p.** libp2p only carries signaling and chat.
Relay nodes forward encrypted traffic only and never process media.

```
┌─────────────────────────────┐
│      React / Android UI      │
└──────────────┬──────────────┘
               │ events + commands (typed Rust `Event` / JSON)
┌──────────────▼──────────────┐
│       Application Layer      │  calls / chat / users
│   (signaling state machine)  │  rust-core: signaling.rs, protocol.rs
└──────────────┬──────────────┘
               │ libp2p
┌──────────────▼──────────────┐
│        libp2p Networking     │  identity, DHT, identify, ping,
│ Discovery / DHT / Signaling  │  chat/call request-response, relay client
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│           WebRTC             │  RTCPeerConnection, DTLS-SRTP,
│       Audio / Video          │  ICE (host → srflx → relay/TURN)
└─────────────────────────────┘
```

## Components

```
                 ┌──────────────────────┐
                 │  Bootstrap / Relay   │   bootstrap-node, relay-node,
                 │       Peers          │   frontend/scripts/relay-server.mjs
                 └──────────┬───────────┘
                            │
                     Discovery / Relay
                            │
             ┌──────────────┴──────────────┐
             │                             │
      ┌──────▼──────┐               ┌──────▼──────┐
      │   Browser   │◄────WebRTC────►│   Android   │
      │   Peer A    │               │   Peer B    │
      └──────┬──────┘               └──────┬──────┘
             │                             │
             └────────── libp2p ───────────┘
                    signaling / discovery
```

## Repository layout

| Path | Role |
| --- | --- |
| `rust-core/` | The Rust networking core: peer identity, swarm, DHT, chat/call protocols, signaling state machine, relay client. Platform-independent. |
| `bootstrap-node/` | Long-running DHT bootstrap peer. Helps new peers enter the network; not an application server. |
| `relay-node/` | libp2p circuit-relay v2 server. Forwards encrypted traffic; never sees media. |
| `android-ffi/` | C ABI + JNI bridge exposing `rust-core` to Android. |
| `frontend/` | Vite + React client: WebRTC media engine + JS libp2p signaling + BroadcastChannel demo mode. |
| `android/` | Android app scaffolding: JNI wrapper, build script. |
| `docs/` | This documentation. |

## Rust core module map

```
rust-core/src/
├── identity.rs    ed25519 keypair generation + secure persistence (Peer ID is
│                  derived from the public key; IP addresses are not identity)
├── network.rs     Peer: Swarm<Identify+Ping+Kademlia+Chat+Call+Relay+DCUtR+mDNS>
│                  typed API: dial, send_chat, send_call_message, bootstrap,
│                  put_record / get_record, start_providing / get_providers
├── events.rs      Event enum surfaced to UI / FFI (connectivity, ping, DHT,
│                  chat, call signaling, relay, dcutr)
├── discovery.rs   tracks identified/discovered peers and their addresses
├── dht.rs         AddressRecord encode/decode + `/p2p-video-chat/address/` key
├── signaling.rs   CallManager / CallSignaler: call state machine + message
│                  factories (request/accept/reject/end/SDP/ICE)
├── protocol.rs    ChatMessage / CallMessage, CBOR serialization, size limits
└── relay.rs       RelayConfig: parse relay multiaddr, circuit detection
```

## Cross-platform signaling

The same application protocol runs on every platform:

- **Browser ↔ Browser**: JS libp2p (`frontend/src/lib/signaling/libp2p-signaling.ts`)
  or, for the zero-infrastructure demo, BroadcastChannel
  (`frontend/src/lib/signaling/broadcast.ts`).
- **Browser ↔ Android**: browser dials via a public circuit-relay; the Android
  Rust core connects to the same relay/DHT network. WebRTC then flows directly.
- **Android ↔ Android**: Rust core on both sides via `android-ffi`.

## NAT traversal strategy

Direct WebRTC is preferred; relay is only a fallback. See
[nat-traversal.md](nat-traversal.md) for the full strategy (ICE candidates,
DCUtR, relay reservation, TURN fallback).

## Observability

Every peer logs structured events (`tracing`) and can report:

```
Peer ID · Connected peers · Known addresses · DHT status · routing-table size
WebRTC ICE state · connection state · RTT · packet loss · bytes sent/received
```

The frontend shows these on a debug panel (`frontend/src/components/DebugPanel.tsx`).
