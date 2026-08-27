# NAT Traversal

Real deployments put peers behind home routers, cellular networks, different
Wi-Fi networks, VPNs, and CGNAT. This document describes how the application
establishes a direct WebRTC connection and when it falls back to a relay.

## Connectivity strategy

Preferred path:

```
Peer A ─────────────── Peer B
       Direct WebRTC (DTLS-SRTP)
```

Fallback:

```
Peer A ─── Relay ─── Peer B
```

The relay **only forwards encrypted traffic**; it never decodes, transcodes or
processes media.

## Layered ICE

The browser uses `RTCPeerConnection` with STUN servers to enumerate candidates:

1. **Host candidates** — LAN addresses. Work on the same network.
2. **Server-reflexive candidates** — public IP discovered via STUN.
   Works across most home routers (Cone NAT) without any relay.
3. **Relay / TURN candidates** — used only when direct paths fail
   (symmetric NAT, CGNAT, restrictive firewalls).

## libp2p side

The Rust core (`rust-core/src/relay.rs`, `network.rs`) implements the libp2p
side of NAT traversal:

- **Circuit relay v2 client** — every peer dials configured relay servers at
  startup and requests a reservation, so it is reachable even behind NAT
  (`with_relay_client` in `SwarmBuilder`).
- **DCUtR** — "direct connection upgrade through relay". Once two peers are
  connected through a relay, DCUtR attempts a hole-punch to establish a direct
  connection and then upgrades the path.
- **Identify** — peers advertise their externally observed addresses, which
  feeds the DHT and improves routability.
- **mDNS** — zero-config discovery on local networks (no-op on WAN).

The **relay-node** binary and the JS relay (`frontend/scripts/relay-server.mjs`)
are public circuit-relay v2 servers. Both only relay encrypted streams.

## Decision flow

```
Want to call peer
   │
   ▼
Resolve peer via DHT / manual dial
   │
   ▼
Try direct WebRTC (host + srflx candidates)
   │
   ├── connected ──────────────► media flows directly
   │
   ▼
ICE includes relay/TURN candidates
   │
   ├── relay/TURN connected ───► media flows through encrypted relay
   │
   ▼
Failure ───────────────────────► error state surfaced to user
```

## Mobile-specific handling

Android devices change networks frequently (Wi-Fi → cellular → VPN). Because
the **Peer ID, not the IP address, is the identity**, the app:

1. Detects connectivity changes (`ConnectivityManager`).
2. Re-inits the Rust core with the **same `identity_file`** (peer id stable).
3. Re-bootstraps into the DHT.
4. Re-dials important peers.
5. Re-establishes the WebRTC `RTCPeerConnection` with fresh ICE.

The Rust core never treats IP addresses as identity
(`rust-core/src/identity.rs`).

## Testing matrix (Phase 7)

| Scenario | Expectation |
| --- | --- |
| Same LAN | Direct host candidates, instant connect |
| Different Wi-Fi (home routers) | STUN srflx candidates, direct connect |
| Cellular networks | srflx or TURN/relay fallback |
| CGNAT | Relay/TURN fallback required |
| Browser ↔ Android | libp2p signaling via relay; WebRTC direct after ICE |

## Known limitations

- A fully public STUN/TURN service (e.g. Coturn) is not bundled; the demo uses
  Google's public STUN servers. For production, deploy your own TURN.
- DCUtR hole punching succeeds only on endpoint-independent mapping NATs;
  symmetric NATs require the relay/TURN fallback.
- The JS relay and the Rust relay-node are interchangeable, but the browser
  requires a WebSocket transport, so browsers connect to the JS relay.
