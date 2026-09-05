import { useState } from 'react'
import { useController } from './hooks/useController'
import { PeerList, IdentityCard } from './components/PeerList'
import { CallPanel } from './components/CallPanel'
import { ChatPanel } from './components/ChatPanel'
import { DebugPanel } from './components/DebugPanel'
import type { SignalingMode } from './lib/controller'

export default function App() {
  const [mode, setMode] = useState<SignalingMode>('hub')
  const [relayAddr, setRelayAddr] = useState('')
  const [selectedPeer, setSelectedPeer] = useState<string | null>(null)
  const { controller, identity, peers, call, chats, status } = useController(mode, relayAddr || undefined)

  return (
    <div className="app">
      <header className="app-header">
        <h1>P2P Video Chat</h1>
        <div className="mode-switcher">
          <label>
            signaling:
            <select value={mode} onChange={(e) => setMode(e.target.value as SignalingMode)}>
              <option value="hub">Multi-device (default)</option>
              <option value="broadcast">Same-browser tabs</option>
              <option value="libp2p">libp2p circuit-relay</option>
            </select>
          </label>
          {mode === 'libp2p' && (
            <input
              className="relay-input"
              placeholder="/dns4/host/tcp/443/wss/p2p/... (relay addr)"
              value={relayAddr}
              onChange={(e) => setRelayAddr(e.target.value)}
            />
          )}
          <span className={`status-pill status-${status.includes('online') ? 'online' : 'starting'}`}>{status}</span>
        </div>
      </header>

      {controller && identity && (
        <IdentityCard
          peerId={identity.peer_id}
          displayName={identity.display_name}
          onRename={(name: string) => {
            controller.setDisplayName(name)
            window.location.reload()
          }}
        />
      )}

      <div className="main-grid">
        <PeerList
          peers={peers}
          selectedPeerId={selectedPeer}
          onSelect={setSelectedPeer}
          onCall={(id) => void controller?.callPeer(id)}
          onConnect={(id) => controller?.connectPeer(id)}
          busy={!!call && call.status !== 'ended'}
        />
        <div className="center-column">
          <CallPanel call={call} controller={controller} />
          {selectedPeer && (
            <ChatPanel
              peerId={selectedPeer}
              peerName={peers.find((p) => p.peer_id === selectedPeer)?.display_name ?? selectedPeer}
              chats={chats}
              onSend={(text) => void controller?.sendChat(selectedPeer, text)}
            />
          )}
        </div>
        <DebugPanel status={status} peers={peers} call={call} />
      </div>
    </div>
  )
}
