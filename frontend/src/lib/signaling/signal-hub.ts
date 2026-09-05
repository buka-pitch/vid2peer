// Same-origin WebSocket signaling backend.
//
// Used for multi-device discovery and WebRTC SDP/ICE exchange. The hub only
// forwards signaling envelopes; audio/video still flows peer-to-peer over
// WebRTC. This is the default mode so two phones / a phone and a laptop can
// find each other without a public libp2p relay.

import type { SignalEnvelope } from '../../types'
import type { BrowserIdentity } from '../identity'

export interface SignalHubEvents {
  onPeer: (envelope: SignalEnvelope) => void
  onPeerPresence: (peer: { peer_id: string; display_name: string; online: boolean }) => void
  onStatus: (status: string) => void
}

export class SignalHubSignaling {
  private identity: BrowserIdentity
  private events: SignalHubEvents
  private ws: WebSocket | null = null
  private closed = false
  private pingTimer: number | null = null
  private reconnectTimer: number | null = null
  private attempts = 0

  constructor(identity: BrowserIdentity, events: SignalHubEvents) {
    this.identity = identity
    this.events = events
  }

  start(): void {
    this.closed = false
    this.connect()
  }

  private url(): string {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
    return `${proto}//${location.host}/signal`
  }

  private connect(): void {
    if (this.closed) return
    this.events.onStatus('connecting to signaling hub')
    const ws = new WebSocket(this.url())
    this.ws = ws

    ws.onopen = () => {
      this.attempts = 0
      ws.send(
        JSON.stringify({
          type: 'hello',
          peer_id: this.identity.peer_id,
          display_name: this.identity.display_name,
        }),
      )
      this.stopPing()
      this.pingTimer = window.setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'ping' }))
      }, 15000)
    }

    ws.onmessage = (ev) => {
      let msg: any
      try {
        msg = JSON.parse(String(ev.data))
      } catch {
        return
      }
      if (msg.type === 'welcome') {
        this.events.onStatus('online (multi-device)')
        for (const p of msg.peers ?? []) {
          this.events.onPeerPresence({ peer_id: p.peer_id, display_name: p.display_name, online: true })
        }
        return
      }
      if (msg.type === 'peer-join') {
        this.events.onPeerPresence({ peer_id: msg.peer_id, display_name: msg.display_name, online: true })
        return
      }
      if (msg.type === 'peer-leave') {
        this.events.onPeerPresence({ peer_id: msg.peer_id, display_name: msg.peer_id, online: false })
        return
      }
      if (msg.type === 'signal' && msg.envelope) {
        this.events.onPeer(msg.envelope)
        return
      }
      if (msg.type === 'error') {
        this.events.onStatus('hub: ' + msg.message)
      }
    }

    ws.onclose = () => {
      this.stopPing()
      if (this.closed) return
      this.events.onStatus('signaling disconnected, retrying…')
      const delay = Math.min(8000, 500 * 2 ** this.attempts)
      this.attempts += 1
      this.reconnectTimer = window.setTimeout(() => this.connect(), delay)
    }

    ws.onerror = () => {
      ws.close()
    }
  }

  send(peerId: string, envelope: SignalEnvelope): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('not connected to signaling hub')
    }
    this.ws.send(JSON.stringify({ type: 'signal', to: peerId, envelope }))
  }

  rename(displayName: string): void {
    this.identity = { ...this.identity, display_name: displayName }
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'rename', display_name: displayName }))
    }
  }

  stop(): void {
    this.closed = true
    this.stopPing()
    if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer)
    this.ws?.close()
    this.ws = null
  }

  private stopPing(): void {
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer)
      this.pingTimer = null
    }
  }
}
