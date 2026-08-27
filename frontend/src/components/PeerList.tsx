import { useState } from 'react'
// @ts-expect-error qrcode ships without types
import QRCode from 'qrcode'
import type { PeerInfo } from '../types'

export function IdentityCard({ peerId, displayName, onRename }: { peerId: string; displayName: string; onRename: (name: string) => void }) {
  const [qr, setQr] = useState<string | null>(null)
  const [name, setName] = useState(displayName)

  const showQr = async () => {
    if (!qr) {
      const dataUrl = await QRCode.toDataURL(peerId, { width: 220, margin: 1 })
      setQr(dataUrl)
    } else {
      setQr(null)
    }
  }

  const copy = async () => {
    await navigator.clipboard.writeText(peerId)
  }

  return (
    <div className="card identity-card">
      <div className="identity-info">
        <span className="label">Peer ID</span>
        <code className="peer-id">{peerId}</code>
        <span className="label">Display name</span>
        <div className="name-row">
          <input value={name} onChange={(e) => setName(e.target.value)} />
          <button onClick={() => onRename(name)}>rename</button>
        </div>
      </div>
      <div className="identity-actions">
        <button onClick={copy}>copy</button>
        <button onClick={showQr}>QR</button>
      </div>
      {qr && <img className="qr" src={qr} alt="peer id qr code" />}
    </div>
  )
}

export function PeerList({
  peers,
  selectedPeerId,
  onSelect,
  onCall,
  busy,
}: {
  peers: PeerInfo[]
  selectedPeerId: string | null
  onSelect: (id: string) => void
  onCall: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="card">
      <h2>Peers</h2>
      {peers.length === 0 && <p className="muted">No peers discovered yet. Open this app in a second tab to demo.</p>}
      <ul className="peer-list">
        {peers.map((p) => (
          <li key={p.peer_id} className={p.peer_id === selectedPeerId ? 'selected' : ''}>
            <button className="peer-row" onClick={() => onSelect(p.peer_id)}>
              <span className={`dot ${p.status === 'online' ? 'online' : 'offline'}`} />
              <span className="peer-name">{p.display_name || p.peer_id.slice(-8)}</span>
              <code className="peer-short">{p.peer_id.slice(0, 12)}…</code>
            </button>
            <button className="call-btn" disabled={busy || p.status !== 'online'} onClick={() => onCall(p.peer_id)}>
              call
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
