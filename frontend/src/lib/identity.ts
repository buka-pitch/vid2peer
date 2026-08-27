// Browser-side peer identity.
//
// The browser does not have access to the Rust key store; it keeps a
// lightweight identity in localStorage. The Peer ID is the stable device
// identity and is what is shared with other peers. The real cryptographic
// identity lives in the Rust core (rust-core/src/identity.rs); in the full
// deployment the browser asks the Rust core for its Peer ID.

const ID_KEY = 'p2pvc.identity'
const NAME_KEY = 'p2pvc.displayName'

export interface BrowserIdentity {
  peer_id: string
  display_name: string
}

function randomPeerId(): string {
  // 12D3KooW-prefixed base58 look-alike; stable for the session/device.
  const bytes = new Uint8Array(20)
  crypto.getRandomValues(bytes)
  const b58 = base58(bytes)
  return '12D3KooW' + b58.slice(0, 36)
}

function base58(bytes: Uint8Array): string {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
  let n = BigInt('0x' + Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join(''))
  let out = ''
  while (n > 0n) {
    out = alphabet[Number(n % 58n)] + out
    n /= 58n
  }
  return out
}

export function loadIdentity(): BrowserIdentity {
  // Session-scoped peer id so two tabs of the same-origin demo are distinct
  // peers (BroadcastChannel discovery needs one identity per tab). The display
  // name stays device-wide (localStorage) so renaming one tab updates the rest.
  const existing = sessionStorage.getItem(ID_KEY)
  if (existing) {
    const name = localStorage.getItem(NAME_KEY) ?? 'browser-peer'
    return { peer_id: existing, display_name: name }
  }
  const peer_id = randomPeerId()
  sessionStorage.setItem(ID_KEY, peer_id)
  localStorage.setItem(NAME_KEY, 'browser-peer')
  return { peer_id, display_name: 'browser-peer' }
}

export function saveDisplayName(name: string): void {
  localStorage.setItem(NAME_KEY, name)
}
