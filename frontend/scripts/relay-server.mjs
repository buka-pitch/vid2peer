// libp2p circuit-relay v2 server for browser peers.
//
// The relay node only forwards encrypted signaling traffic between browsers;
// it never sees or processes audio/video media (WebRTC flows directly between
// peers). This mirrors the Rust relay-node (relay-node/) but exposes a
// WebSocket transport that browsers can connect to.
//
// Run:  npm run relay
// It prints the relay multiaddr, e.g.:
//   RELAY_INFO /ip4/127.0.0.1/tcp/9090/ws/p2p/<peerid>

import { createLibp2p } from 'libp2p'
import { webSockets } from '@libp2p/websockets'
import { noise } from '@chainsafe/libp2p-noise'
import { yamux } from '@chainsafe/libp2p-yamux'
import { circuitRelayServer, circuitRelayTransport } from '@libp2p/circuit-relay-v2'
import { identify } from '@libp2p/identify'
import { ping } from '@libp2p/ping'

const PORT = Number(process.env.RELAY_PORT ?? 9090)

const node = await createLibp2p({
  addresses: {
    listen: [`/ip4/0.0.0.0/tcp/${PORT}/ws`],
  },
  transports: [webSockets(), circuitRelayTransport()],
  connectionEncrypters: [noise()],
  streamMuxers: [yamux()],
  connectionManager: {
    maxConnections: 500,
  },
  services: {
    identify: identify({ runOnConnectionOpen: true }),
    ping: ping({ protocolPrefix: '/p2p-video-chat' }),
    relay: circuitRelayServer({
      reservations: {
        maxReservations: 500,
        reservationTtl: 2 * 60 * 60 * 1000,
        defaultDurationLimit: 2 * 60 * 1000,
        applyDefaultLimit: false,
      },
    }),
  },
})

await node.start()

for (const addr of node.getMultiaddrs()) {
  console.log('RELAY_INFO ' + addr.toString())
}
console.log('relay listening on /ip4/0.0.0.0/tcp/' + PORT + '/ws')

process.on('SIGINT', async () => {
  await node.stop()
  process.exit(0)
})
