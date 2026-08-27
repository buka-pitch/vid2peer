// Shared application-level protocol types for the browser client.
// These mirror the Rust `rust-core/src/protocol.rs` message shapes so that
// browser and Rust peers can interoperate.

export type CallMessage =
  | { type: 'call_request'; call_id: string; from: string; to: string; timestamp: number; metadata: CallMetadata }
  | { type: 'call_accepted'; call_id: string; from: string; to: string; timestamp: number }
  | { type: 'call_rejected'; call_id: string; from: string; to: string; timestamp: number; reason: string }
  | { type: 'call_ended'; call_id: string; from: string; to: string; timestamp: number }
  | { type: 'sdp_offer'; call_id: string; from: string; to: string; timestamp: number; sdp: string }
  | { type: 'sdp_answer'; call_id: string; from: string; to: string; timestamp: number; sdp: string }
  | { type: 'ice_candidate'; call_id: string; from: string; to: string; timestamp: number; candidate: string; sdp_mid: string | null }

export interface CallMetadata {
  media: string[]
  display_name: string
}

export interface ChatMessage {
  type: 'chat'
  id: string
  from: string
  to: string
  timestamp: number
  text: string
}

export type SignalEnvelope =
  | { kind: 'call'; message: CallMessage }
  | { kind: 'chat'; message: ChatMessage }
  | { kind: 'peer-info'; peer_id: string; display_name: string; media: string[] }

export interface PeerInfo {
  peer_id: string
  display_name: string
  status: 'online' | 'offline'
  addresses: string[]
  last_seen: number
}

export function nowTs(): number {
  return Math.floor(Date.now() / 1000)
}

export function newCallId(): string {
  return crypto.randomUUID()
}
