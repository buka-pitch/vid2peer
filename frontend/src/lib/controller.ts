// Application controller: ties identity, signaling, and WebRTC together.
//
// Mirrors the "Application Layer" from the architecture. The controller owns
// the peer list, call state, and chat history, and forwards WebRTC signaling
// messages to the active call. Media never passes through signaling.

import type { CallMessage, CallMetadata, ChatMessage, PeerInfo, SignalEnvelope } from '../types'
import { nowTs, newCallId } from '../types'
import type { BrowserIdentity } from './identity'
import { saveDisplayName } from './identity'
import { BroadcastSignaling } from './signaling/broadcast'
import { Libp2pSignaling } from './signaling/libp2p-signaling'
import { WebRtcManager, type CallStats, type NegotiationRequest } from '../webrtc/webrtc-manager'

export type SignalingMode = 'broadcast' | 'libp2p'

export type CallStatus =
  | 'idle'
  | 'outgoing'
  | 'incoming'
  | 'connecting'
  | 'active'
  | 'ended'
  | 'error'

export interface ActiveCall {
  call_id: string
  remote: string
  remoteName: string
  status: CallStatus
  outgoing: boolean
  error: string | null
  stats: CallStats | null
  localStream: MediaStream | null
  remoteStream: MediaStream | null
  audioMuted: boolean
  videoMuted: boolean
  media: string[]
  connState?: string
  iceState?: string
}

export interface ChatItem {
  id: string
  peer_id: string
  peer_name: string
  text: string
  timestamp: number
  incoming: boolean
}

export type ControllerEvent =
  | { type: 'peers'; peers: PeerInfo[] }
  | { type: 'call'; call: ActiveCall | null }
  | { type: 'chat'; items: ChatItem[] }
  | { type: 'status'; status: string }
  | { type: 'local-stream'; stream: MediaStream | null }

export class Controller {
  identity: BrowserIdentity
  mode: SignalingMode
  peers = new Map<string, PeerInfo>()
  chats: ChatItem[] = []
  activeCall: ActiveCall | null = null
  onEvent: ((event: ControllerEvent) => void) | null = null
  private lastSeen = new Map<string, number>()
  private broadcast: BroadcastSignaling | null = null
  private libp2p: Libp2pSignaling | null = null
  private webrtc: WebRtcManager | null = null
  private endedCallIds = new Set<string>()
  private pendingOffer: string | null = null
  private config: { relayMultiaddr?: string }

  constructor(identity: BrowserIdentity, mode: SignalingMode, config: { relayMultiaddr?: string } = {}) {
    this.identity = identity
    this.mode = mode
    this.config = config
  }

  async start(): Promise<void> {
    this.emit({ type: 'status', status: 'starting' })
    if (this.mode === 'broadcast') {
      this.broadcast = new BroadcastSignaling(this.identity, {
        onPeer: (env) => this.handleEnvelope(env),
        onPeerPresence: ({ peer_id, display_name, online }) => {
          this.updatePresence(peer_id, display_name, online)
        },
      })
      this.broadcast.start()
      this.emit({ type: 'status', status: 'online (broadcast mode)' })
    } else {
      this.libp2p = new Libp2pSignaling(
        this.identity,
        {
          onPeer: (env) => this.handleEnvelope(env),
          onPeerPresence: ({ peer_id, display_name, online }) => {
            this.updatePresence(peer_id, display_name, online)
          },
          onStatus: (s) => this.emit({ type: 'status', status: s }),
        },
        this.config.relayMultiaddr ?? '',
        (env) => this.handleEnvelope(env),
      )
      await this.libp2p.start()
      if (this.libp2p.peerId) {
        this.identity = { ...this.identity, peer_id: this.libp2p.peerId }
      }
      this.emit({ type: 'status', status: 'online (libp2p mode)' })
    }
    this.emit({ type: 'peers', peers: this.peerList() })
  }

  setDisplayName(name: string): void {
    saveDisplayName(name)
    this.identity = { ...this.identity, display_name: name }
  }

  // ---- Peers ----

  private updatePresence(peer_id: string, display_name: string, online: boolean): void {
    const now = Date.now()
    if (online) this.lastSeen.set(peer_id, now)
    const existing = this.peers.get(peer_id)
    if (online) {
      this.peers.set(peer_id, {
        peer_id,
        display_name: existing?.display_name ?? display_name,
        status: 'online',
        addresses: existing?.addresses ?? [],
        last_seen: now,
      })
    } else if (existing) {
      existing.status = 'offline'
    }
    this.emit({ type: 'peers', peers: this.peerList() })
  }

  private peerList(): PeerInfo[] {
    return Array.from(this.peers.values()).sort((a, b) => b.last_seen - a.last_seen)
  }

  // ---- Incoming envelope dispatch ----

  private handleEnvelope(env: SignalEnvelope): void {
    if (env.kind === 'chat') {
      const msg = env.message as ChatMessage
      if (msg.from === this.identity.peer_id) return
      this.pushChat(msg.from, msg.text, msg.id, msg.timestamp, true)
      return
    }
    if (env.kind === 'peer-info') {
      this.updatePresence(env.peer_id, env.display_name, true)
      return
    }
    const msg = env.message as CallMessage
    if (this.endedCallIds.has(msg.call_id)) return
    switch (msg.type) {
      case 'call_request':
        this.onCallRequest(msg)
        break
      case 'call_accepted':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          this.updateCall({ status: 'connecting' })
          // Callee accepted: now create and send the SDP offer.
          void this.webrtc?.startOutbound()
        }
        break
      case 'call_rejected':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          this.endCall('rejected by remote: ' + msg.reason)
        }
        break
      case 'call_ended':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          this.endCall('remote hung up')
        }
        break
      case 'sdp_offer':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          if (this.webrtc) {
            void this.webrtc.acceptInbound(msg.sdp)
          } else {
            // Offer arrived before the callee accepted: buffer it.
            this.pendingOffer = msg.sdp
          }
        }
        break
      case 'sdp_answer':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          void this.webrtc?.handleAnswer(msg.sdp)
          // Do not mark active here: the connection becomes active only when
          // RTCPeerConnection reports 'connected'.
        }
        break
      case 'ice_candidate':
        if (this.activeCall && this.activeCall.call_id === msg.call_id) {
          void this.webrtc?.handleCandidate(msg.candidate, msg.sdp_mid)
        }
        break
    }
  }

  // ---- Calls ----

  private onCallRequest(msg: Extract<CallMessage, { type: 'call_request' }>): void {
    const metadata = msg.metadata ?? { media: ['audio', 'video'], display_name: msg.from }
    const call: ActiveCall = {
      call_id: msg.call_id,
      remote: msg.from,
      remoteName: this.peers.get(msg.from)?.display_name ?? msg.from,
      status: 'incoming',
      outgoing: false,
      error: null,
      stats: null,
      localStream: null,
      remoteStream: null,
      audioMuted: false,
      videoMuted: false,
      media: metadata.media,
    }
    this.activeCall = call
    this.emit({ type: 'call', call })
  }

  async callPeer(peerId: string): Promise<void> {
    if (this.activeCall) return
    const call_id = newCallId()
    const call: ActiveCall = {
      call_id,
      remote: peerId,
      remoteName: this.peers.get(peerId)?.display_name ?? peerId,
      status: 'outgoing',
      outgoing: true,
      error: null,
      stats: null,
      localStream: null,
      remoteStream: null,
      audioMuted: false,
      videoMuted: false,
      media: ['audio', 'video'],
    }
    this.activeCall = call
    this.emit({ type: 'call', call })

    const webrtc = new WebRtcManager(
      (req: NegotiationRequest) => this.sendNegotiation(peerId, call_id, req),
      {
        onRemoteStream: (stream) => this.updateCall({ remoteStream: stream }),
        onConnectionState: (state) => {
          this.updateCall({ connState: state })
          if (state === 'connected') this.updateCall({ status: 'active' })
          if (state === 'failed' || state === 'closed') this.endCall('connection ' + state)
        },
        onIceState: (state) => this.updateCall({ iceState: state }),
        onStats: (stats) => this.updateCall({ stats }),
        onError: (msg) => this.updateCall({ error: msg, status: 'error' }),
      },
    )
    this.webrtc = webrtc
    try {
      const stream = await webrtc.startLocalMedia(true, true)
      this.updateCall({ localStream: stream, status: 'connecting' })
      await this.send(
        peerId,
        this.makeCallMessage('call_request', call_id, { media: ['audio', 'video'], display_name: this.identity.display_name }),
      )
    } catch (err) {
      this.updateCall({ status: 'error', error: 'media access denied: ' + String(err) })
    }
  }

  async acceptCall(): Promise<void> {
    if (!this.activeCall || this.activeCall.status !== 'incoming') return
    const call = this.activeCall
    const webrtc = new WebRtcManager(
      (req: NegotiationRequest) => this.sendNegotiation(call.remote, call.call_id, req),
      {
        onRemoteStream: (stream) => this.updateCall({ remoteStream: stream }),
        onConnectionState: (state) => {
          this.updateCall({ connState: state })
          if (state === 'connected') this.updateCall({ status: 'active' })
          if (state === 'failed' || state === 'closed') this.endCall('connection ' + state)
        },
        onIceState: (state) => this.updateCall({ iceState: state }),
        onStats: (stats) => this.updateCall({ stats }),
        onError: (msg) => this.updateCall({ error: msg, status: 'error' }),
      },
    )
    this.webrtc = webrtc
    try {
      const stream = await webrtc.startLocalMedia(true, true)
      this.updateCall({ localStream: stream, status: 'connecting' })
      if (this.pendingOffer) {
        const offer = this.pendingOffer
        this.pendingOffer = null
        await webrtc.acceptInbound(offer)
      }
      await this.send(
        call.remote,
        this.makeCallMessage('call_accepted', call.call_id, undefined),
      )
    } catch (err) {
      this.updateCall({ status: 'error', error: 'media access denied: ' + String(err) })
    }
  }

  rejectCall(): void {
    if (!this.activeCall || this.activeCall.status !== 'incoming') return
    const call = this.activeCall
    void this.send(call.remote, this.makeCallMessage('call_rejected', call.call_id, undefined, 'busy'))
    this.endCall('call rejected')
  }

  hangUp(): void {
    if (!this.activeCall) return
    const call = this.activeCall
    if (call.status === 'incoming') {
      void this.send(call.remote, this.makeCallMessage('call_rejected', call.call_id, undefined, 'declined'))
    } else if (call.status !== 'ended' && call.status !== 'error') {
      void this.send(call.remote, this.makeCallMessage('call_ended', call.call_id, undefined))
    }
    this.endCall('ended')
  }

  toggleAudio(): void {
    if (!this.webrtc || !this.activeCall) return
    const muted = !this.activeCall.audioMuted
    this.webrtc.setAudioEnabled(!muted)
    this.updateCall({ audioMuted: muted })
  }

  toggleVideo(): void {
    if (!this.webrtc || !this.activeCall) return
    const muted = !this.activeCall.videoMuted
    this.webrtc.setVideoEnabled(!muted)
    this.updateCall({ videoMuted: muted })
  }

  private endCall(reason: string): void {
    void this.webrtc?.hangUp()
    this.webrtc = null
    if (this.activeCall) {
      this.endedCallIds.add(this.activeCall.call_id)
      const last = this.activeCall
      const ended: ActiveCall = {
        ...last,
        status: 'ended',
        error: last.error ?? reason,
        localStream: null,
        remoteStream: null,
      }
      this.activeCall = ended
      this.emit({ type: 'call', call: ended })
      setTimeout(() => {
        this.activeCall = null
        this.emit({ type: 'call', call: null })
      }, 2500)
    }
  }

  private updateCall(patch: Partial<ActiveCall>): void {
    if (!this.activeCall) return
    this.activeCall = { ...this.activeCall, ...patch }
    this.emit({ type: 'call', call: this.activeCall })
  }

  private async sendNegotiation(peerId: string, call_id: string, req: NegotiationRequest): Promise<void> {
    if (req.type === 'offer') {
      await this.send(peerId, this.makeCallMessage('sdp_offer', call_id, undefined, undefined, req.sdp ?? ''))
    } else if (req.type === 'answer') {
      await this.send(peerId, this.makeCallMessage('sdp_answer', call_id, undefined, undefined, req.sdp ?? ''))
    } else if (req.candidate) {
      await this.send(
        peerId,
        this.makeCallMessage('ice_candidate', call_id, undefined, undefined, req.candidate.candidate ?? '', req.candidate.sdpMid ?? null),
      )
    }
  }

  private makeCallMessage(
    type: CallMessage['type'],
    call_id: string,
    metadata?: CallMetadata,
    reason?: string,
    payload?: string,
    sdpMid?: string | null,
  ): SignalEnvelope {
    const base = { call_id, from: this.identity.peer_id, to: '', timestamp: nowTs() }
    let message: CallMessage
    switch (type) {
      case 'call_request':
        message = { type, ...base, to: '', metadata: metadata ?? { media: ['audio', 'video'], display_name: this.identity.display_name } }
        break
      case 'call_rejected':
        message = { type, ...base, reason: reason ?? 'busy' }
        break
      case 'sdp_offer':
        message = { type, ...base, sdp: payload ?? '' }
        break
      case 'sdp_answer':
        message = { type, ...base, sdp: payload ?? '' }
        break
      case 'ice_candidate':
        message = { type, ...base, candidate: payload ?? '', sdp_mid: sdpMid ?? null }
        break
      default:
        message = { type, ...base }
    }
    return { kind: 'call', message }
  }

  // ---- Chat ----

  async sendChat(peerId: string, text: string): Promise<void> {
    const id = newCallId()
    const msg: ChatMessage = {
      type: 'chat',
      id,
      from: this.identity.peer_id,
      to: peerId,
      timestamp: nowTs(),
      text,
    }
    this.pushChat(peerId, text, id, msg.timestamp, false)
    await this.send(peerId, { kind: 'chat', message: msg })
  }

  private pushChat(peerId: string, text: string, id: string, timestamp: number, incoming: boolean): void {
    const name = this.peers.get(peerId)?.display_name ?? peerId
    this.chats = [...this.chats, { id, peer_id: peerId, peer_name: name, text, timestamp, incoming }]
    this.emit({ type: 'chat', items: this.chats })
  }

  // ---- Send ----

  private async send(peerId: string, envelope: SignalEnvelope): Promise<void> {
    if (this.broadcast) {
      this.broadcast.send(peerId, envelope)
    } else if (this.libp2p) {
      await this.libp2p.send(peerId, envelope)
    }
  }

  private emit(event: ControllerEvent): void {
    this.onEvent?.(event)
  }

  stop(): void {
    this.broadcast?.stop()
    void this.libp2p?.stop()
    void this.webrtc?.hangUp()
  }
}
