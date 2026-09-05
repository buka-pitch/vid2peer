// Signaling hub for multi-device discovery and WebRTC signaling.
//
// Peers connect over WebSocket. The hub only forwards encrypted signaling
// (SDP / ICE / chat / presence). Audio and video never pass through it.
//
// Run: node scripts/signal-hub.mjs

import { WebSocketServer } from 'ws'

const PORT = Number(process.env.SIGNAL_HUB_PORT ?? 9091)

/** @type {Map<string, { ws: import('ws').WebSocket, displayName: string, lastSeen: number }>} */
const peers = new Map()

const wss = new WebSocketServer({ port: PORT, path: '/signal' })

function send(ws, msg) {
  if (ws.readyState === 1) ws.send(JSON.stringify(msg))
}

function peerList(exceptId) {
  const out = []
  for (const [peer_id, p] of peers) {
    if (peer_id === exceptId) continue
    out.push({ peer_id, display_name: p.displayName })
  }
  return out
}

function broadcast(exceptId, msg) {
  const data = JSON.stringify(msg)
  for (const [id, p] of peers) {
    if (id === exceptId) continue
    if (p.ws.readyState === 1) p.ws.send(data)
  }
}

wss.on('connection', (ws) => {
  let peerId = null

  ws.on('message', (raw) => {
    let msg
    try {
      msg = JSON.parse(String(raw))
    } catch {
      return
    }

    if (msg.type === 'hello') {
      const id = typeof msg.peer_id === 'string' ? msg.peer_id.slice(0, 128) : ''
      const name = typeof msg.display_name === 'string' ? msg.display_name.slice(0, 64) : 'peer'
      if (!id) {
        send(ws, { type: 'error', message: 'missing peer_id' })
        return
      }
      const existing = peers.get(id)
      if (existing && existing.ws !== ws) {
        try {
          existing.ws.close()
        } catch {
          // ignore
        }
      }
      peerId = id
      peers.set(id, { ws, displayName: name, lastSeen: Date.now() })
      send(ws, { type: 'welcome', peers: peerList(id) })
      broadcast(id, { type: 'peer-join', peer_id: id, display_name: name })
      console.log('join', id, 'peers=', peers.size)
      return
    }

    if (!peerId) return
    const self = peers.get(peerId)
    if (self) self.lastSeen = Date.now()

    if (msg.type === 'ping') {
      send(ws, { type: 'pong' })
      return
    }

    if (msg.type === 'rename' && typeof msg.display_name === 'string') {
      if (self) self.displayName = msg.display_name.slice(0, 64)
      broadcast(peerId, { type: 'peer-join', peer_id: peerId, display_name: self?.displayName ?? 'peer' })
      return
    }

    if (msg.type === 'signal' && typeof msg.to === 'string' && msg.envelope) {
      const dest = peers.get(msg.to)
      if (!dest) {
        send(ws, { type: 'error', message: 'peer offline: ' + msg.to })
        return
      }
      send(dest.ws, { type: 'signal', from: peerId, envelope: msg.envelope })
    }
  })

  ws.on('close', () => {
    if (!peerId) return
    const current = peers.get(peerId)
    if (current && current.ws === ws) {
      peers.delete(peerId)
      broadcast(peerId, { type: 'peer-leave', peer_id: peerId })
      console.log('leave', peerId, 'peers=', peers.size)
    }
  })
})

console.log('SIGNAL_HUB listening on ws://0.0.0.0:' + PORT + '/signal')
