// BroadcastChannel signaling backend.
//
// Zero-infrastructure mode for the same-device demo: two tabs of this app on
// the same origin exchange WebRTC signaling through the browser's
// BroadcastChannel API. Every peer broadcasts its presence so tabs discover
// each other without any server.
//
// In production this backend is replaced by the libp2p signaling backend
// (src/lib/signaling/libp2p-signaling.ts) so browsers talk to Rust peers.

import type { SignalEnvelope } from '../../types'
import type { BrowserIdentity } from '../identity'

export interface SignalingEvents {
  onPeer: (envelope: SignalEnvelope) => void
  onPeerPresence: (peer: { peer_id: string; display_name: string; online: boolean }) => void
}

const REGISTRY = 'p2pvc:registry'
const PRESENCE_INTERVAL_MS = 3000
const OFFLINE_TIMEOUT_MS = 8000

export class BroadcastSignaling {
  private registry: BroadcastChannel
  private inbox: BroadcastChannel
  private identity: BrowserIdentity
  private events: SignalingEvents
  private seen = new Map<string, number>()
  private timer: number | null = null

  constructor(identity: BrowserIdentity, events: SignalingEvents) {
    this.identity = identity
    this.events = events
    this.registry = new BroadcastChannel(REGISTRY)
    this.inbox = new BroadcastChannel(`p2pvc:${identity.peer_id}`)

    this.registry.onmessage = (ev: MessageEvent<{ peer_id: string; display_name: string }>) => {
      const data = ev.data
      if (!data || data.peer_id === this.identity.peer_id) return
      this.seen.set(data.peer_id, Date.now())
      this.events.onPeerPresence({ peer_id: data.peer_id, display_name: data.display_name, online: true })
    }

    this.inbox.onmessage = (ev: MessageEvent<SignalEnvelope>) => {
      if (!ev.data) return
      this.events.onPeer(ev.data)
    }
  }

  start(): void {
    this.announce()
    this.timer = window.setInterval(() => {
      this.announce()
      this.expire()
    }, PRESENCE_INTERVAL_MS)
  }

  /** Send a message to a specific peer through its private channel. */
  send(peerId: string, envelope: SignalEnvelope): void {
    const channel = new BroadcastChannel(`p2pvc:${peerId}`)
    channel.postMessage(envelope)
    // Give the channel time to flush before closing.
    setTimeout(() => channel.close(), 250)
  }

  announce(): void {
    this.registry.postMessage({ peer_id: this.identity.peer_id, display_name: this.identity.display_name })
  }

  listPeers(): { peer_id: string; display_name: string }[] {
    const now = Date.now()
    const out: { peer_id: string; display_name: string }[] = []
    this.seen.forEach((ts, peer_id) => {
      if (now - ts < OFFLINE_TIMEOUT_MS) out.push({ peer_id, display_name: peer_id })
    })
    return out
  }

  private expire(): void {
    const now = Date.now()
    this.seen.forEach((ts, peer_id) => {
      if (now - ts >= OFFLINE_TIMEOUT_MS) {
        this.seen.delete(peer_id)
        this.events.onPeerPresence({ peer_id, display_name: peer_id, online: false })
      }
    })
  }

  stop(): void {
    if (this.timer !== null) clearInterval(this.timer)
    this.registry.close()
    this.inbox.close()
  }
}
