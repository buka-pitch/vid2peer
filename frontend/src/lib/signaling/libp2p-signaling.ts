// libp2p signaling backend.
//
// Browser peers build a JS libp2p node (WebSocket transport) and connect to a
// public circuit-relay v2 server (scripts/relay-server.mjs). Call/chat
// signaling travels over a small JSON protocol (`/p2p-video-chat/signal/1.0.0`)
// multiplexed through the relay; the actual audio/video media flows over a
// direct WebRTC connection.
//
// This is the transport that lets browsers talk to Rust-based peers.

import { createLibp2p, type Libp2p } from 'libp2p'
import { webSockets } from '@libp2p/websockets'
import { noise } from '@chainsafe/libp2p-noise'
import { yamux } from '@chainsafe/libp2p-yamux'
import { circuitRelayTransport } from '@libp2p/circuit-relay-v2'
import { identify } from '@libp2p/identify'
import { ping } from '@libp2p/ping'
import { pipe } from 'it-pipe'
import { fromString, toString } from 'uint8arrays'
import { multiaddr } from '@multiformats/multiaddr'
import type { Stream } from '@libp2p/interface'
import type { SignalEnvelope } from '../../types'
import type { BrowserIdentity } from '../identity'

export const SIGNAL_PROTOCOL = '/p2p-video-chat/signal/1.0.0'

export interface Libp2pSignalingEvents {
  onPeer: (envelope: SignalEnvelope) => void
  onPeerPresence: (peer: { peer_id: string; display_name: string; online: boolean }) => void
  onStatus: (status: string) => void
}

export class Libp2pSignaling {
  private node: Libp2p | null = null
  private relayMultiaddr: string
  private relayPeerId: string | null = null
  private onEnvelope: (envelope: SignalEnvelope) => void
  private events: Libp2pSignalingEvents

  constructor(
    identity: BrowserIdentity,
    events: Libp2pSignalingEvents,
    relayMultiaddr: string,
    onEnvelope: (envelope: SignalEnvelope) => void,
  ) {
    void identity
    this.events = events
    this.relayMultiaddr = relayMultiaddr
    this.onEnvelope = onEnvelope
  }

  async start(): Promise<void> {
    this.node = await createLibp2p({
      transports: [webSockets(), circuitRelayTransport({ discoverRelays: 1 })],
      connectionEncrypters: [noise()],
      streamMuxers: [yamux()],
      services: {
        identify: identify({ runOnConnectionOpen: true }),
        ping: ping({ protocolPrefix: '/p2p-video-chat' }),
      },
      connectionManager: {
        maxConnections: 50,
      },
    })

    const onEnvelope = this.onEnvelope
    this.node.handle(SIGNAL_PROTOCOL, ({ stream }) => {
      pipe(
        stream,
        async function (source) {
          for await (const buf of source) {
            const chunk = buf.subarray()
            const text = toString(chunk)
            for (const line of text.split('\n')) {
              if (!line.trim()) continue
              try {
                const envelope = JSON.parse(line) as SignalEnvelope
                onEnvelope(envelope)
              } catch {
                // malformed signaling frame; ignore.
              }
            }
          }
        },
      )
    })

    try {
      const addr = multiaddr(this.relayMultiaddr)
      this.relayPeerId = addr.getPeerId() ?? null
      await this.node.dial(addr)
      this.events.onStatus('connected to relay ' + this.relayPeerId)
    } catch (err) {
      this.events.onStatus('relay dial failed: ' + String(err))
    }
  }

  get peerId(): string | null {
    return this.node?.peerId.toString() ?? null
  }

  /** Send a signaling envelope to a peer through the relay circuit. */
  async send(peerId: string, envelope: SignalEnvelope): Promise<void> {
    if (!this.node) throw new Error('node not started')
    if (!this.relayPeerId) throw new Error('no relay connection')
    const circuit = multiaddr(`/p2p/${this.relayPeerId}/p2p-circuit/p2p/${peerId}`)
    const conn = await this.node.dial(circuit)
    const stream: Stream = await conn.newStream([SIGNAL_PROTOCOL])
    await stream.sink([fromString(JSON.stringify(envelope) + '\n')])
    await stream.close()
  }

  async stop(): Promise<void> {
    if (this.node) {
      await this.node.stop()
      this.node = null
    }
  }
}
