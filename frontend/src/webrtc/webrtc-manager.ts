// WebRTC media engine for the browser.
//
// Responsibilities (kept separate from libp2p networking):
//   - getUserMedia() for local camera + microphone
//   - RTCPeerConnection, ICE, SDP
//   - mute / camera-toggle / hang-up
//   - RTT / packet-loss / bitrate stats for the debug screen
//
// The signaling transport (whoever sends our SDP/ICE) is injected through
// the `send` callback. Media never touches the libp2p application protocol.

export interface WebRtcEvents {
  onRemoteStream: (stream: MediaStream) => void
  onConnectionState: (state: RTCPeerConnectionState) => void
  onIceState: (state: RTCIceConnectionState) => void
  onStats: (stats: CallStats) => void
  onError: (message: string) => void
}

export interface CallStats {
  rttMs: number
  packetLossPct: number
  bitrateKbps: number
  bytesSent: number
  bytesReceived: number
  candidateType: string
  remoteCandidateType: string
}

export interface NegotiationRequest {
  /** The raw SDP answer/offer or ICE candidate to send to the remote peer. */
  sdp?: string
  candidate?: RTCIceCandidateInit
  type: 'offer' | 'answer' | 'ice'
}

export type SignalSender = (msg: NegotiationRequest) => Promise<void> | void

const PC_CONFIG: RTCConfiguration = {
  iceServers: [
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
  ],
}

export class WebRtcManager {
  private pc: RTCPeerConnection | null = null
  private localStream: MediaStream | null = null
  private sender: SignalSender
  private events: WebRtcEvents
  private statsTimer: number | null = null
  private pendingCandidates: RTCIceCandidateInit[] = []

  constructor(sender: SignalSender, events: WebRtcEvents) {
    this.sender = sender
    this.events = events
  }

  get localVideo(): MediaStream | null {
    return this.localStream
  }

  get connection(): RTCPeerConnection | null {
    return this.pc
  }

  async startLocalMedia(audio = true, video = true): Promise<MediaStream> {
    if (this.localStream) return this.localStream
    this.localStream = await navigator.mediaDevices.getUserMedia({ audio, video })
    return this.localStream
  }

  /** Initialize an outbound call: attach local tracks and create the offer. */
  async startOutbound(): Promise<void> {
    try {
      await this.ensurePeerConnection()
      if (this.pc && this.localStream) {
        for (const track of this.localStream.getTracks()) {
          this.pc.addTrack(track, this.localStream)
        }
        const offer = await this.pc.createOffer()
        await this.pc.setLocalDescription(offer)
        await this.sender({ type: 'offer', sdp: offer.sdp ?? '' })
      }
    } catch (err) {
      this.events.onError('failed to start outbound call: ' + String(err))
    }
  }

  /** Handle an inbound call: accept and answer with local media attached. */
  async acceptInbound(offerSdp: string): Promise<void> {
    try {
      await this.ensurePeerConnection()
      if (!this.pc) return
      for (const track of this.localStream?.getTracks() ?? []) {
        if (this.pc.getSenders().some((s) => s.track === track)) continue
        this.pc.addTrack(track, this.localStream as MediaStream)
      }
      const offer = { type: 'offer', sdp: offerSdp } as RTCSessionDescriptionInit
      await this.pc.setRemoteDescription(offer)
      const answer = await this.pc.createAnswer()
      await this.pc.setLocalDescription(answer)
      this.flushPendingCandidates()
      await this.sender({ type: 'answer', sdp: answer.sdp ?? '' })
    } catch (err) {
      this.events.onError('failed to answer: ' + String(err))
    }
  }

  async handleAnswer(sdp: string): Promise<void> {
    if (!this.pc) return
    try {
      await this.pc.setRemoteDescription({ type: 'answer', sdp })
      this.flushPendingCandidates()
    } catch (err) {
      this.events.onError('failed to apply answer: ' + String(err))
    }
  }

  async handleCandidate(candidate: string, sdpMid?: string | null): Promise<void> {
    if (!this.pc) return
    const init: RTCIceCandidateInit = { candidate, sdpMid: sdpMid ?? undefined }
    if (this.pc.remoteDescription) {
      await this.pc.addIceCandidate(init).catch(() => {
        // Candidates may be stale; ignore.
      })
    } else {
      // Remote description not set yet: queue until it is.
      this.pendingCandidates.push(init)
    }
  }

  private flushPendingCandidates(): void {
    if (!this.pc) return
    const queue = this.pendingCandidates
    this.pendingCandidates = []
    for (const c of queue) {
      void this.pc.addIceCandidate(c).catch(() => {})
    }
  }

  setAudioEnabled(enabled: boolean): void {
    this.localStream?.getAudioTracks().forEach((t) => (t.enabled = enabled))
  }

  setVideoEnabled(enabled: boolean): void {
    this.localStream?.getVideoTracks().forEach((t) => (t.enabled = enabled))
  }

  async toggleCamera(): Promise<boolean> {
    const track = this.localStream?.getVideoTracks()[0]
    if (!track) return false
    track.enabled = !track.enabled
    return track.enabled
  }

  async hangUp(): Promise<void> {
    this.stopStats()
    if (this.pc) {
      this.pc.ontrack = null
      this.pc.onconnectionstatechange = null
      this.pc.oniceconnectionstatechange = null
      this.pc.close()
      this.pc = null
    }
    this.localStream?.getTracks().forEach((t) => t.stop())
    this.localStream = null
  }

  private async ensurePeerConnection(): Promise<void> {
    if (this.pc) return
    this.pc = new RTCPeerConnection(PC_CONFIG)
    this.pc.onconnectionstatechange = () => {
      this.events.onConnectionState(this.pc?.connectionState ?? 'closed')
    }
    this.pc.oniceconnectionstatechange = () => {
      this.events.onIceState(this.pc?.iceConnectionState ?? 'closed')
    }
    this.pc.ontrack = (ev) => {
      this.events.onRemoteStream(ev.streams[0])
    }
    this.pc.onicecandidate = (ev) => {
      if (ev.candidate) {
        void this.sender({ type: 'ice', candidate: ev.candidate.toJSON() })
      }
    }
    this.startStats()
  }

  private startStats(): void {
    this.stopStats()
    this.statsTimer = window.setInterval(() => this.collectStats(), 2000)
  }

  private stopStats(): void {
    if (this.statsTimer !== null) {
      clearInterval(this.statsTimer)
      this.statsTimer = null
    }
  }

  private async collectStats(): Promise<void> {
    if (!this.pc) return
    const report = await this.pc.getStats()
    let rttMs = 0
    let bytesSent = 0
    let bytesReceived = 0
    let packetsSent = 0
    let packetsLost = 0
    let candidateType = 'unknown'
    let remoteCandidateType = 'unknown'
    report.forEach((s) => {
      const stat = s as any
      if (stat.type === 'candidate-pair' && stat.nominated) {
        if (stat.currentRoundTripTime) rttMs = stat.currentRoundTripTime * 1000
        if (stat.bytesSent) bytesSent = stat.bytesSent
        if (stat.bytesReceived) bytesReceived = stat.bytesReceived
        if (stat.packetsSent) packetsSent = stat.packetsSent
        if (stat.packetsLost) packetsLost = stat.packetsLost
      }
      if (stat.type === 'local-candidate' && stat.candidateType) candidateType = stat.candidateType
      if (stat.type === 'remote-candidate' && stat.candidateType) remoteCandidateType = stat.candidateType
    })
    const packetLossPct = packetsSent > 0 ? (packetsLost / packetsSent) * 100 : 0
    // Bitrate is computed over successive polls; keep last sample in closure.
    this.lastBytes = { sent: bytesSent, received: bytesReceived, at: Date.now() }
    const prev = this.prevSample
    let bitrateKbps = 0
    if (prev) {
      const dt = (this.lastBytes.at - prev.at) / 1000
      if (dt > 0) {
        const sentDelta = bytesSent - prev.sent
        bitrateKbps = (sentDelta * 8) / dt / 1000
      }
    }
    this.prevSample = this.lastBytes
    this.events.onStats({
      rttMs,
      packetLossPct,
      bitrateKbps,
      bytesSent,
      bytesReceived,
      candidateType,
      remoteCandidateType,
    })
  }

  private prevSample: { sent: number; received: number; at: number } | null = null
  private lastBytes: { sent: number; received: number; at: number } = { sent: 0, received: 0, at: Date.now() }
}
