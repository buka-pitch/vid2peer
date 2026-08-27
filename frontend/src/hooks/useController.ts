import { useEffect, useRef, useState } from 'react'
import { Controller, type ActiveCall, type ChatItem, type ControllerEvent, type SignalingMode } from '../lib/controller'
import type { PeerInfo } from '../types'

export interface ControllerState {
  controller: Controller | null
  identity: { peer_id: string; display_name: string } | null
  peers: PeerInfo[]
  call: ActiveCall | null
  chats: ChatItem[]
  status: string
  localStream: MediaStream | null
}

export function useController(mode: SignalingMode, relayMultiaddr?: string): ControllerState {
  const [state, setState] = useState<ControllerState>({
    controller: null,
    identity: null,
    peers: [],
    call: null,
    chats: [],
    status: 'offline',
    localStream: null,
  })
  const controllerRef = useRef<Controller | null>(null)

  useEffect(() => {
    let cancelled = false
    let controller: Controller | null = null

    const create = async () => {
      const { loadIdentity } = await import('../lib/identity')
      const id = loadIdentity()
      controller = new Controller(id, mode, relayMultiaddr ? { relayMultiaddr } : {})
      controllerRef.current = controller
      controller.onEvent = (ev: ControllerEvent) => {
        if (cancelled) return
        switch (ev.type) {
          case 'peers':
            setState((s) => ({ ...s, peers: ev.peers }))
            break
          case 'call':
            setState((s) => ({ ...s, call: ev.call }))
            break
          case 'chat':
            setState((s) => ({ ...s, chats: ev.items }))
            break
          case 'status':
            setState((s) => ({ ...s, status: ev.status }))
            break
          case 'local-stream':
            setState((s) => ({ ...s, localStream: ev.stream }))
            break
        }
      }
      setState((s) => ({
        ...s,
        controller,
        identity: { peer_id: id.peer_id, display_name: id.display_name },
        status: 'starting',
      }))
      await controller.start()
      setState((s) => ({
        ...s,
        identity: { peer_id: controller!.identity.peer_id, display_name: controller!.identity.display_name },
      }))
    }

    void create()
    return () => {
      cancelled = true
      controllerRef.current?.stop()
    }
  }, [mode, relayMultiaddr])

  return state
}
