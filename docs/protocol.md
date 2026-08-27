# Application Protocol

Small application-level protocols over libp2p. Payloads are **CBOR** (binary,
compact) in Rust; the browser uses JSON framing of the same message shapes.

## Protocol IDs

| Protocol | Use | Transport |
| --- | --- | --- |
| `/p2p-video-chat/identify/1.0.0` | Identify protocol version | libp2p Identify |
| `/p2p-video-chat/chat/1.0.0` | Text chat | libp2p request-response (CBOR) |
| `/p2p-video-chat/call/1.0.0` | WebRTC signaling | libp2p request-response (CBOR) |
| `/p2p-video-chat/signal/1.0.0` | Browser signaling (JS libp2p) | JSON over stream |

`rust-core/src/protocol.rs` defines `CHAT_PROTOCOL` and `CALL_PROTOCOL`.
The chat path is kept completely separate from the call path.

## Message types

```
Ping            PeerInfo        CallRequest     CallAccepted    CallRejected
CallEnded       SdpOffer        SdpAnswer       IceCandidate    ChatMessage
```

## Chat (`/p2p-video-chat/chat/1.0.0`)

`ChatMessage`:

```json
{
  "type": "chat",
  "id": "uuid",
  "from": "12D3KooW...",
  "to": "12D3KooW...",
  "timestamp": 1234567890,
  "text": "hello"
}
```

Each outbound message is answered by `ChatAck`; delivery is correlated with a
per-peer FIFO queue on the sending side (request-response responses do not
carry request ids). Oversized messages are rejected locally
(`MAX_CHAT_MESSAGE_BYTES`).

## Call signaling (`/p2p-video-chat/call/1.0.0`)

The call protocol is a state machine (`rust-core/src/signaling.rs`):

```
Pending → Negotiating → InProgress → Ended
   └────────── Rejected
```

Messages:

```json
{ "type": "call_request",   "call_id": "abc123", "from": "12D3KooW...",
  "to": "12D3KooW...",      "timestamp": 1234567890,
  "metadata": { "media": ["audio","video"], "display_name": "Alice" } }

{ "type": "call_accepted",  "call_id": "abc123", "from": "...", "to": "...", "timestamp": ... }
{ "type": "call_rejected",  "call_id": "abc123", "from": "...", "to": "...",
  "timestamp": ...,         "reason": "busy" }
{ "type": "call_ended",     "call_id": "abc123", "from": "...", "to": "...", "timestamp": ... }

{ "type": "sdp_offer",      "call_id": "abc123", "from": "...", "to": "...",
  "timestamp": ...,         "sdp": "v=0\r\no=- ..." }
{ "type": "sdp_answer",     "call_id": "abc123", "from": "...", "to": "...",
  "timestamp": ...,         "sdp": "v=0\r\no=- ..." }

{ "type": "ice_candidate",  "call_id": "abc123", "from": "...", "to": "...",
  "timestamp": ...,         "candidate": "candidate:1 1 UDP ...", "sdp_mid": "0" }
```

A full exchange:

```
Alice                       libp2p                        Bob
  │── call_request ────────────────────────────────────────►│
  │◄──────────────────────────── call_accepted ─────────────┤
  │── sdp_offer ───────────────────────────────────────────►│
  │◄───────────────────────────── sdp_answer ───────────────┤
  │── ice_candidate ───────────────────────────────────────►│
  │◄───────────────────────────── ice_candidate ────────────┤
  │                                                          │
  │◄────────────── WebRTC media (direct, NOT libp2p) ───────►│
```

## CBOR vs JSON

- Rust peers serialize `ChatMessage`/`CallMessage` with CBOR (via `ciborium`)
  for compactness and to match the "prefer CBOR/protobuf" requirement.
- Browsers use a JSON framing of the identical shapes because browser libp2p
  handling of binary is more verbose; the field names and semantics match 1:1.

## Security properties

- Every libp2p connection is encrypted with **Noise** and multiplexed with
  **Yamux**; handshake authenticates both Peer IDs.
- Peer IDs are derived from ed25519 public keys; private keys never leave the
  Rust core (`rust-core/src/identity.rs`, file mode `0600`).
- Signaling messages are validated (sizes, call-id matching, well-formed SDP
  before being passed to WebRTC). Call ids from ended calls are ignored
  (replay protection).
- DHT records are namespaced (`/p2p-video-chat/address/`) and size-capped.
- Media is additionally protected by WebRTC **DTLS-SRTP**; relay nodes only
  forward encrypted bytes and cannot decode media.
