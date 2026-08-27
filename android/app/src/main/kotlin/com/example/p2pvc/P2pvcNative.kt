package com.example.p2pvc

import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * JNI bridge to the Rust networking core (libp2pvc_ffi.so).
 *
 * The Rust core owns the peer identity, the libp2p swarm, the DHT, and the
 * call/chat signaling protocols. The Android UI only exchanges JSON messages
 * with it over this boundary. The private key never leaves the Rust core.
 *
 * Lifecycle:
 *   1. [init] with a JSON config (bootstrap peers, identity file, ...).
 *   2. Call [call] actions like sendChat / sendCallMessage / dial.
 *   3. Start [drainEvents] on a background thread; it dispatches [Listener.onEvent].
 *   4. [close] when done.
 */
class P2pvcNative(private val listener: Listener) {

    interface Listener {
        fun onEvent(eventJson: String)
        fun onStatus(status: String)
    }

    private val executor = Executors.newSingleThreadExecutor()
    private val mainHandler = Handler(Looper.getMainLooper())
    private val draining = AtomicBoolean(false)

    /** Opaque handle to the Rust Peer. */
    @Volatile
    private var handle: Long = 0L

    init {
        System.loadLibrary("p2pvc_ffi")
    }

    /**
     * Initialize the networking core.
     *
     * Example config:
     * {
     *   "listen_addrs": ["/ip4/0.0.0.0/tcp/0"],
     *   "bootstrap_peers": [
     *     { "peer_id": "12D3KooW...", "address": "/ip4/1.2.3.4/tcp/4001" }
     *   ],
     *   "identity_file": "/data/user/0/com.example.p2pvc/files/peer.key",
     *   "enable_mdns": false
     * }
     */
    fun init(configJson: String): Boolean {
        handle = nInit(configJson)
        if (handle == 0L) {
            listener.onStatus("failed to initialize networking core")
            return false
        }
        val id = nPeerId(handle)?.let { it }
        listener.onStatus("networking core online, peer id: $id")
        return true
    }

    fun peerId(): String? = handle.takeIf { it != 0L }?.let { nPeerId(it) }

    /** Start draining networking events on a background thread. */
    fun drainEvents() {
        if (handle == 0L || draining.getAndSet(true)) return
        executor.execute {
            while (draining.get()) {
                val ev = nNextEvent(handle) ?: break
                mainHandler.post { listener.onEvent(ev) }
            }
        }
    }

    fun bootstrap() = handle.takeIf { it != 0L }?.let { nBootstrap(it) } ?: -1

    fun sendChat(peerId: String, text: String) =
        handle.takeIf { it != 0L }?.let { nSendChat(it, peerId, text) } ?: -1

    /**
     * Send a call signaling message. `msgJson` is a serialized CallMessage,
     * e.g. {"type":"call_request","call_id":"...","from":"...","to":"...",
     *       "timestamp":...,"metadata":{"media":["audio","video"]}}
     */
    fun sendCallMessage(peerId: String, msgJson: String) =
        handle.takeIf { it != 0L }?.let { nSendCallMessage(it, peerId, msgJson) } ?: -1

    fun dial(multiaddr: String) =
        handle.takeIf { it != 0L }?.let { nDial(it, multiaddr) } ?: -1

    fun connectionCount(): Int = handle.takeIf { it != 0L }?.let { nConnectionCount(it) } ?: -1

    fun routingTableSize(): Int = handle.takeIf { it != 0L }?.let { nRoutingTableSize(it) } ?: -1

    fun close() {
        draining.set(false)
        val h = handle
        handle = 0L
        if (h != 0L) nFree(h)
        executor.shutdownNow()
    }

    private external fun nInit(configJson: String): Long
    private external fun nPeerId(handle: Long): String?
    private external fun nNextEvent(handle: Long): String?
    private external fun nBootstrap(handle: Long): Int
    private external fun nSendChat(handle: Long, peerId: String, text: String): Int
    private external fun nSendCallMessage(handle: Long, peerId: String, msgJson: String): Int
    private external fun nDial(handle: Long, multiaddr: String): Int
    private external fun nConnectionCount(handle: Long): Int
    private external fun nRoutingTableSize(handle: Long): Int
    private external fun nFree(handle: Long)

    companion object {
        init {
            System.loadLibrary("p2pvc_ffi")
        }
    }
}
