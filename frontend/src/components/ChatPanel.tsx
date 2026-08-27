import { useEffect, useRef, useState } from 'react'
import type { ChatItem } from '../lib/controller'

export function ChatPanel({
  peerId,
  peerName,
  chats,
  onSend,
}: {
  peerId: string
  peerName: string
  chats: ChatItem[]
  onSend: (text: string) => void
}) {
  const [text, setText] = useState('')
  const listRef = useRef<HTMLDivElement>(null)
  const peerChats = chats.filter((c) => c.peer_id === peerId)

  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = listRef.current.scrollHeight
  }, [peerChats.length])

  return (
    <div className="card chat-panel">
      <h2>Chat with {peerName}</h2>
      <div className="chat-list" ref={listRef}>
        {peerChats.length === 0 && <p className="muted">No messages yet.</p>}
        {peerChats.map((c) => (
          <div key={c.id} className={`chat-item ${c.incoming ? 'in' : 'out'}`}>
            <span className="chat-text">{c.text}</span>
            <span className="chat-time">{new Date(c.timestamp * 1000).toLocaleTimeString()}</span>
          </div>
        ))}
      </div>
      <form
        className="chat-form"
        onSubmit={(e) => {
          e.preventDefault()
          if (text.trim()) {
            onSend(text.trim())
            setText('')
          }
        }}
      >
        <input value={text} onChange={(e) => setText(e.target.value)} placeholder="type a message" />
        <button type="submit">send</button>
      </form>
    </div>
  )
}
