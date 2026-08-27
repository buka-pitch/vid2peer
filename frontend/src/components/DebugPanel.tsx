import type { ActiveCall } from '../lib/controller'
import type { PeerInfo } from '../types'

export function DebugPanel({
  status,
  peers,
  call,
}: {
  status: string
  peers: PeerInfo[]
  call: ActiveCall | null
}) {
  return (
    <div className="card debug-panel">
      <h2>Debug</h2>
      <table className="debug-table">
        <tbody>
          <tr>
            <td>signaling status</td>
            <td>{status}</td>
          </tr>
          <tr>
            <td>known peers</td>
            <td>{peers.length}</td>
          </tr>
          <tr>
            <td>online peers</td>
            <td>{peers.filter((p) => p.status === 'online').length}</td>
          </tr>
          <tr>
            <td>call status</td>
            <td>{call?.status ?? 'idle'}</td>
          </tr>
          <tr>
            <td>ice state</td>
            <td>{call?.stats?.candidateType ?? '—'}</td>
          </tr>
          <tr>
            <td>rtt</td>
            <td>{call?.stats ? call.stats.rttMs.toFixed(1) + ' ms' : '—'}</td>
          </tr>
          <tr>
            <td>bytes sent</td>
            <td>{call?.stats ? formatBytes(call.stats.bytesSent) : '—'}</td>
          </tr>
          <tr>
            <td>bytes received</td>
            <td>{call?.stats ? formatBytes(call.stats.bytesReceived) : '—'}</td>
          </tr>
        </tbody>
      </table>
      <h3>Addresses</h3>
      <ul className="addr-list">
        {peers.slice(0, 5).map((p) => (
          <li key={p.peer_id}>
            <code>{p.peer_id.slice(0, 16)}…</code>
            {p.addresses.slice(0, 2).map((a, i) => (
              <div className="muted small" key={i}>
                {a}
              </div>
            ))}
          </li>
        ))}
      </ul>
    </div>
  )
}

function formatBytes(n: number): string {
  if (n > 1e6) return (n / 1e6).toFixed(1) + ' MB'
  if (n > 1e3) return (n / 1e3).toFixed(1) + ' KB'
  return String(n) + ' B'
}
