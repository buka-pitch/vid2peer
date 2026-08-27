import { useRef } from 'react'
import type { ActiveCall, Controller } from '../lib/controller'

export function CallPanel({ call, controller }: { call: ActiveCall | null; controller: Controller | null }) {
  const remoteVideo = useRef<HTMLVideoElement>(null)
  if (remoteVideo.current && call?.remoteStream && remoteVideo.current.srcObject !== call.remoteStream) {
    remoteVideo.current.srcObject = call.remoteStream
    // Muted autoplay always works; unmute on click (autoplay with sound can
    // be blocked by the browser's autoplay policy, leaving the video black).
    remoteVideo.current.muted = true
    remoteVideo.current.play().catch(() => {})
  }

  if (!call || call.status === 'ended') {
    return (
      <div className="card call-panel idle">
        <h2>Call</h2>
        <p className="muted">
          {call?.status === 'ended' ? call.error : 'Select a peer and press call.'}
        </p>
      </div>
    )
  }

  return (
    <div className="card call-panel">
      <h2>
        Call with {call.remoteName}
        <span className={`call-status status-${call.status}`}>{call.status}</span>
      </h2>
      {call.error && <p className="error-text">{call.error}</p>}
      <div className="video-grid">
        {call.remoteStream ? (
          <video
            ref={remoteVideo}
            className="remote-video"
            autoPlay
            playsInline
            muted
            onClick={(e) => {
              const v = e.currentTarget
              v.muted = !v.muted
            }}
            title="click to toggle sound"
          />
        ) : (
          <div className="video-placeholder">connecting…</div>
        )}
        {call.localStream ? (
          <video className="local-video" autoPlay playsInline muted ref={(el) => el && (el.srcObject = call.localStream)} />
        ) : (
          <div className="video-placeholder small">no camera</div>
        )}
      </div>
      <div className="call-controls">
        {call.status === 'incoming' ? (
          <>
            <button className="accept" onClick={() => void controller?.acceptCall()}>
              accept
            </button>
            <button className="reject" onClick={() => controller?.rejectCall()}>
              decline
            </button>
          </>
        ) : (
          <>
            <button className={call.audioMuted ? 'muted' : ''} onClick={() => controller?.toggleAudio()}>
              {call.audioMuted ? 'unmute' : 'mute'}
            </button>
            <button className={call.videoMuted ? 'muted' : ''} onClick={() => controller?.toggleVideo()}>
              {call.videoMuted ? 'cam on' : 'cam off'}
            </button>
            <button className="hangup" onClick={() => controller?.hangUp()}>
              hang up
            </button>
          </>
        )}
      </div>
      {call.stats && (
        <div className="stats-line">
          rtt {call.stats.rttMs.toFixed(0)}ms · loss {call.stats.packetLossPct.toFixed(1)}% ·{' '}
          {call.stats.bitrateKbps.toFixed(0)} kbps · {call.stats.candidateType} →{' '}
          {call.stats.remoteCandidateType}
          {call.connState || call.iceState ? ` · pc ${call.connState} · ice ${call.iceState}` : ''}
        </div>
      )}
      {!call.stats && (call.connState || call.iceState) && (
        <div className="stats-line">pc {call.connState} · ice {call.iceState}</div>
      )}
    </div>
  )
}
