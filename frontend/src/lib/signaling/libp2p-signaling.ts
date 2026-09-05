// libp2p signaling backend.
//
// Browser peers build a JS libp2p node (WebSocket transport) and connect to a
// public circuit-relay v2 server (scripts/relay-server.mjs). Call/chat
// signaling travels over `/p2p-video-chat/signal/1.0.0` through the relay;
// audio/video still flows over a direct WebRTC connection.

import { createLibp2p, type Libp2p } from 'libp2p'
import { webSockets } from '@libp2p/websockets'
import * as wsFilters from '@libp2p/websockets/filters'
import { noise } from '@chainsafe/libp2p-noise'
import { yamux } from '@chainsafe/libp2p-yamux'
import { circuitRelayTransport } from '@libp2p/circuit-relay-v2'
import { identify } from '@libp2p/identify'
import { ping } from '@libp2p/ping'
import { generateKeyPair, privateKeyFromProtobuf, privateKeyToProtobuf } from '@libp2p/crypto/keys'
import { pipe } from 'it-pipe'
import { fromString, toString } from 'uint8arrays'
import { multiaddr } from '@multiformats/multiaddr'
import type { Stream } from '@libp2p/interface'
import type { SignalEnvelope } from '../../types'
import type { BrowserIdentity } from '../identity'

export const SIGNAL_PROTOCOL = '/p2p-video-chat/signal/1.0.0'
const KEY_STORE = 'p2pvc.libp2p.privkey'

export interface Libp2pSignalingEvents {
  onPeer: (envelope: SignalEnvelope) => void
  onPeerPresence: (peer: { peer_id: string; display_name: string; online: boolean }) => void
  onStatus: (status: string) => void
}

async function loadPrivateKey() {
  const stored = sessionStorage.getItem(KEY_STORE)
  if (stored) {
    const bytes = Uint8Array.from(atob(stored), (c) => c.charCodeAt(0))
    return privateKeyFromProtobuf(bytes)
  }
  const key = await generateKeyPair('Ed25519')
  const bytes = privateKeyToProtobuf(key)
  let bin = ''
  bytes.forEach((b) => {
    bin += String.fromCharCode(b)
  })
  sessionStorage.setItem(KEY_STORE, btoa(bin))
  return key
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
    const privateKey = await loadPrivateKey()
    this.node = await createLibp2p({
      privateKey: privateKey as any,
      addresses: {
        listen: ['/p2p-circuit'],
      },
      transports: [
        webSockets({ filter: wsFilters.all }),
        circuitRelayTransport({ discoverRelays: 1 }),
      ],
      connectionEncrypters: [noise()],
      streamMuxers: [yamux()],
      connectionGater: {
        denyDialMultiaddr: async () => false,
      },
      services: {
        identify: identify({ runOnConnectionOpen: true }),
        ping: ping({ protocolPrefix: '/p2p-video-chat' }),
      },
      connectionManager: {
        maxConnections: 50,
      },
    })

    const onEnvelope = this.onEnvelope
    const events = this.events
    this.node.handle(SIGNAL_PROTOCOL, ({ stream, connection }) => {
      const remote = connection.remotePeer.toString()
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
                if (envelope.kind === 'peer-info') {
                  events.onPeerPresence({
                    peer_id: envelope.peer_id,
                    display_name: envelope.display_name,
                    online: true,
                  })
                }
                onEnvelope(envelope)
              } catch {
                // malformed signaling frame; ignore.
              }
            }
          }
        },
      )
      events.onPeerPresence({ peer_id: remote, display_name: remote.slice(-8), online: true })
    })

    this.node.addEventListener('peer:connect', (evt) => {
      const id = evt.detail.toString()
      if (id === this.relayPeerId) return
      events.onPeerPresence({ peer_id: id, display_name: id.slice(-8), online: true })
    })

    if (!this.relayMultiaddr) {
      this.events.onStatus('libp2p: paste a relay multiaddr to connect')
      return
    }

    try {
      const addr = multiaddr(this.relayMultiaddr)
      this.relayPeerId = addr.getPeerId() ?? null
      await this.node.dial(addr)
      this.events.onStatus('connected to relay, reserving…')
      await this.waitForReservation()
      this.events.onStatus('online (libp2p, reserved on relay)')
    } catch (err) {
      this.events.onStatus('relay dial failed: ' + String(err))
    }
  }

  private async waitForReservation(timeoutMs = 8000): Promise<void> {
    if (!this.node) return
    const start = Date.now()
    while (Date.now() - start < timeoutMs) {
      const addrs = this.node.getMultiaddrs().map((a) => a.toString())
      if (addrs.some((a) => a.includes('p2p-circuit'))) return
      await new Promise((r) => setTimeout(r, 250))
    }
  }

  get peerId(): string | null {
    return this.node?.peerId.toString() ?? null
  }

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
