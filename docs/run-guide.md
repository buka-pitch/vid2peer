# Run Guide

This guide covers every component and demo. Each phase lists expected output.

## Prerequisites

```bash
# Rust (stable, ≥1.85)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Node ≥ 18 for the frontend
node --version
```

## Build everything

```bash
cargo build --workspace --release
cd frontend && npm install && npm run build
```

Run all Rust tests (unit + integration):

```bash
cargo test --workspace
```

Expected: all tests pass (identity, protocol CBOR roundtrips, discovery,
two-peer connect/identify/ping, chat delivery+ack, 10-peer DHT discovery,
full call-signaling sequence).

## Phase demos (Rust)

```bash
# Phase 1 — two peers connect, identify, ping
cargo run -p p2p-video-chat-core --example two_peer
#   => "=== Phase 1 OK: two peers connected, identified, pinged ==="

# Phase 2 — 10 peers discover each other through the DHT
cargo run -p p2p-video-chat-core --example kademlia_demo
#   => each peer reports routing-table size; "=== Phase 2 OK: 10 peers ... ==="

# Phase 3 — text chat over /chat/1.0.0
cargo run -p p2p-video-chat-core --example chat_demo
#   => "[B] received from <alice>: \"hello from Alice!\""
#   => "=== Phase 3 OK: chat message delivered and acknowledged ==="

# Phase 4 — full WebRTC signaling over /call/1.0.0
cargo run -p p2p-video-chat-core --example signaling_demo
#   => "=== Phase 4 OK: full signaling exchange over libp2p ==="
```

## Bootstrap and relay nodes

```bash
# Terminal 1 — bootstrap node (prints BOOTSTRAP_INFO line)
./target/release/bootstrap-node --listen /ip4/0.0.0.0/tcp/4001

# Terminal 2 — relay node (prints RELAY_INFO line)
./target/release/relay-node --listen /ip4/0.0.0.0/tcp/4002
```

Both print their reachable multiaddr:

```
BOOTSTRAP_INFO /ip4/0.0.0.0/tcp/4001/p2p/12D3KooW...
RELAY_INFO     /ip4/0.0.0.0/tcp/4002/p2p/12D3KooW...
```

A peer passes these to `P2pConfig` as `bootstrap_peers` / `relay_servers`
(peer id + address pairs). New peers join through the bootstrap node; the
network keeps working if a bootstrap node disappears.

## Frontend (Phase 5)

```bash
cd frontend
npm run dev            # Vite dev server on :5173
```

Open the app in **two tabs** (default "Broadcast" signaling mode). The tabs
discover each other via BroadcastChannel:

1. Tab A: press **call** next to the discovered peer.
2. Tab B: accept the incoming call. Camera/mic prompts appear (HTTPS or
   localhost required for `getUserMedia`).
3. Media flows **directly between the tabs over WebRTC**; signaling only
   negotiated the connection.

Features: peer ID + QR + copy, online/offline status, incoming/outgoing call
UI, video preview, mute/camera controls, hang up, connection status, quality
stats (RTT / loss / bitrate / candidate type), text chat, debug panel.

### libp2p signaling mode (across devices)

```bash
# Terminal 3 — JS circuit-relay server for browsers
cd frontend && npm run relay
#   => RELAY_INFO /ip4/0.0.0.0/tcp/9090/ws/p2p/<peerid>
```

In the app select **libp2p** mode and paste the relay multiaddr. Browsers then
signal through the relay; media still flows directly via WebRTC.

## Android (Phase 6)

See [android/README.md](../android/README.md). Build the native library:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/<version>
./android/build-ffi.sh
```

The Kotlin app drives the same networking core through the JNI bridge
(`com.example.p2pvc.P2pvcNative`).

## NAT traversal (Phase 7)

See [nat-traversal.md](nat-traversal.md). Direct WebRTC is preferred; relay is
the encrypted-only fallback.
