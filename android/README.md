# Android Integration

The Android app uses the Rust networking core through a **JNI bridge**. The
networking layer is fully independent from the UI: the Kotlin app exchanges
JSON messages with the Rust core and never touches the private key.

```
Android UI (Kotlin)
      │
      ▼
com.example.p2pvc.P2pvcNative   (JNI wrapper, app/src/main/kotlin)
      │
      ▼
libp2pvc_ffi.so                 (android-ffi crate, JNI functions)
      │
      ▼
p2p-video-chat-core             (libp2p / Kademlia / Noise / signaling)
```

## Layout

| Path | Purpose |
| --- | --- |
| `build-ffi.sh` | Cross-compiles `android-ffi` for all ABIs and copies `.so` files into `jniLibs`. |
| `app/src/main/kotlin/com/example/p2pvc/P2pvcNative.kt` | JNI wrapper: `init`, `drainEvents`, `sendChat`, `sendCallMessage`, `dial`, `close`, ... |
| `../android-ffi/` | Rust crate exposing both a plain C ABI (`p2pvc_*`) and the JNI functions (`Java_com_example_p2pvc_...`). |

## Build

```bash
# 1. Install the Rust Android targets (once)
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# 2. Cross-compile and install the .so into jniLibs
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/<version>
./android/build-ffi.sh
```

The script copies the shared library into
`android/app/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}`.

## Config JSON passed to `init`

```json
{
  "listen_addrs": ["/ip4/0.0.0.0/tcp/0"],
  "bootstrap_peers": [
    { "peer_id": "12D3KooW...", "address": "/ip4/1.2.3.4/tcp/4001" }
  ],
  "identity_file": "/data/user/0/com.example.p2pvc/files/peer.key",
  "enable_mdns": false
}
```

## Events

`drainEvents()` blocks on a background thread and delivers JSON events to the
UI, one per callback. Each event has the shape:

```json
{
  "tag": "peer_connected | peer_identified | call_in | chat_in | ...",
  "data": { }
}
```

The UI opens WebRTC after receiving a `call_in` with an `sdp_offer`/`ice_candidate`
and sends its answer via `sendCallMessage` — mirroring the browser flow in
`frontend/`.

## Event loop / connectivity changes

Because Android devices change networks frequently (Wi-Fi → cellular → VPN):

1. Observe `ConnectivityManager`; on change call `P2pvcNative.close()`.
2. Re-`init` with the same `identity_file` (peer id stays stable).
3. `bootstrap()` to rejoin the DHT, then re-dial important peers.
4. Re-establish the WebRTC `RTCPeerConnection` with new ICE.
