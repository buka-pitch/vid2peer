# P2P Video Chat

A production-oriented prototype of a **decentralized peer-to-peer video/audio
calling application** that works between web browsers and Android devices.

Video and audio flow **directly between peers over WebRTC** — there is **no
centralized media server**. libp2p is used only for discovery, signaling, and
text chat. Relay nodes forward encrypted signaling and are used only when a
direct connection is impossible; they never process media.

```
                 ┌──────────────────────┐
                 │  Bootstrap / Relay   │
                 │       Peers          │
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

## Key properties

- **True P2P media** — `RTCPeerConnection` between peers; audio/video never
  touches libp2p or a relay server.
- **Decentralized discovery** — Kademlia DHT (libp2p); bootstrap nodes are only
  the entrance, the network survives if they disappear.
- **Stable identity** — Peer IDs are derived from persisted ed25519 keys, never
  from IP addresses.
- **Security** — Noise-encrypted libp2p channels, DTLS-SRTP media, validated
  signaling, replay protection, message size limits.
- **Layered architecture** — DHT answers "where", libp2p answers "how to
  signal", WebRTC answers "how to stream".

## Repository layout

| Path | What it is |
| --- | --- |
| `rust-core/` | Rust networking core: identity, libp2p swarm (TCP / Noise / Yamux / Identify / Ping / Kademlia / mDNS / chat / call signaling / circuit-relay client / DCUtR), typed event stream. |
| `bootstrap-node/` | DHT bootstrap peer (entrance to the network; not an app server). |
| `relay-node/` | libp2p circuit-relay v2 server (encrypted signaling forwarding only). |
| `android-ffi/` | C ABI + JNI bridge exposing the Rust core to Android. |
| `android/` | Android app scaffolding: Kotlin JNI wrapper, FFI build script, connectivity notes. |
| `frontend/` | Vite + React client: WebRTC media engine, JS libp2p signaling, BroadcastChannel demo mode, chat, QR, debug panel. |
| `docs/` | `architecture.md`, `protocol.md`, `nat-traversal.md`, `run-guide.md`. |

## Prerequisites

- Rust stable ≥ 1.85 (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js ≥ 18 (`node --version`)
- A browser with WebRTC (Chrome, Edge, Firefox, Safari). Camera/mic require a
  secure context (HTTPS or `localhost`).

## Start it

### 1. Build and test the Rust core

```bash
cargo build --workspace
cargo test --workspace
```

Expected: 27 tests pass (identity, protocol CBOR roundtrips, discovery,
two-peer connect/identify/ping, chat delivery + ack, 10-peer DHT discovery,
full call-signaling sequence).

### 2. Run the phase demos (Rust)

```bash
cargo run -p p2p-video-chat-core --example two_peer          # Phase 1
cargo run -p p2p-video-chat-core --example kademlia_demo     # Phase 2 (10 peers)
cargo run -p p2p-video-chat-core --example chat_demo         # Phase 3
cargo run -p p2p-video-chat-core --example signaling_demo    # Phase 4
```

Each prints an "OK" line on success (details in `docs/run-guide.md`).

### 3. Start the network nodes (optional, for a live multi-machine network)

```bash
cargo build --release

# Terminal 1 — DHT bootstrap node
./target/release/bootstrap-node --listen /ip4/0.0.0.0/tcp/4001
#   => BOOTSTRAP_INFO /ip4/0.0.0.0/tcp/4001/p2p/12D3KooW...

# Terminal 2 — circuit-relay node
./target/release/relay-node --listen /ip4/0.0.0.0/tcp/4002
#   => RELAY_INFO /ip4/0.0.0.0/tcp/4002/p2p/12D3KooW...
```

Rust peers pass these addresses to `P2pConfig` as `bootstrap_peers` /
`relay_servers` (`<peer-id>/p2p/<multiaddr>`).

### 4. Run the web app

```bash
cd frontend
npm install
npm run dev          # Vite dev server, http://localhost:5173
```

Open the preview URL on **two devices** (phone + laptop, two phones, etc.).
Peers appear automatically. Press **call**, allow camera/mic.

Same-browser two-tab demo still works: switch signaling to **Same-browser tabs**.

### 5. Android

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/<version>
./android/build-ffi.sh
```

See [android/README.md](android/README.md) for the JNI wrapper and example app.

## Make a call (web app)

**Across devices (default):**

1. Open the same URL on two devices (or two browsers).
2. Both connect to the signaling hub at `/signal`. The left panel lists the other
   peer automatically. You can also paste a Peer ID and press **add**.
3. Press **call**, allow camera/mic, then **accept** on the other device.
4. Media flows **directly over WebRTC**. The hub only carries SDP/ICE/chat.
5. Controls: **mute / cam off / hang up**. Click the remote video to unmute.

**Same device — two tabs:**

Switch signaling to **Same-browser tabs**. Tabs discover each other via
BroadcastChannel with no extra server.

**libp2p circuit-relay (advanced):**

1. Start the browser relay server:

   ```bash
   cd frontend && npm run relay
   #   => RELAY_INFO /ip4/0.0.0.0/tcp/9090/ws/p2p/<peerid>
   ```

2. In the app, switch the signaling dropdown to **libp2p** and paste the relay
   multiaddr from `RELAY_INFO` into the relay input.
3. Open the app on another device, use the same relay, and share peer IDs via
   the QR code / copy button. Calls and chat now travel over libp2p (through
   the relay only for signaling); **media still flows directly via WebRTC**.

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| Peers never appear on another device | Stay on **Multi-device**. Both devices must use the same URL. Hard-reload if the page was open before this update. |
| Call stays on "connecting" | Usually network/firewall blocking WebRTC (STUN on UDP/3478 or host candidates). Confirm on a plain network, or use libp2p mode through a relay. A tab left open from before a code update can also cause this — hard reload (`Ctrl+Shift+R` / `Cmd+Shift+R`). |
| Black remote video | Browser autoplay policy blocks sound-on autoplay; the video starts muted by design. **Click the remote video to unmute.** |
| Camera/mic denied | `getUserMedia` requires a secure context (HTTPS or `localhost`). |
| Android cross-compile fails on `ring` | The libp2p crypto crate needs the Android NDK toolchain; set `ANDROID_NDK_HOME` (see `android/README.md`). |

## Development phases

Implemented incrementally per `.agent.md`:

1. Basic libp2p (identify/ping) — `two_peer` demo + integration test
2. Kademlia discovery — 10-peer demo + integration test
3. Messaging — `/chat/1.0.0` request-response + test
4. Signaling — `/call/1.0.0` state machine + test
5. Browser WebRTC — Vite + React client
6. Android — JNI bridge (`android-ffi`)
7. NAT traversal — ICE / relay / DCUtR strategy documented; relay nodes shipped

## Further reading

- [docs/architecture.md](docs/architecture.md) — layered design and component responsibilities
- [docs/protocol.md](docs/protocol.md) — libp2p application protocols and message shapes
- [docs/nat-traversal.md](docs/nat-traversal.md) — how peers connect through NATs
- [docs/run-guide.md](docs/run-guide.md) — expected output for every demo and node

## License

MIT
