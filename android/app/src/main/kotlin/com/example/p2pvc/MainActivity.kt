package com.example.p2pvc

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import org.json.JSONObject

/**
 * Minimal example wiring the Rust networking core into an Activity.
 * In a full app the events would drive a Compose/View-based call UI and a
 * WebRTC session (org.webrtc:google-webrtc).
 */
class MainActivity : AppCompatActivity(), P2pvcNative.Listener {

    private val native = P2pvcNative(this)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val identityFile = filesDir.resolve("peer.key").absolutePath
        val config = JSONObject().apply {
            put("listen_addrs", arrayOf("/ip4/0.0.0.0/tcp/0"))
            put("identity_file", identityFile)
            put("enable_mdns", false)
            put("bootstrap_peers", arrayOf(
                JSONObject().put("peer_id", "<bootstrap-peer-id>").put("address", "/ip4/<host>/tcp/4001")
            ))
        }

        if (native.init(config.toString())) {
            native.bootstrap()
            native.drainEvents()
        }
    }

    override fun onEvent(eventJson: String) {
        runOnUiThread {
            val ev = JSONObject(eventJson)
            when (ev.getString("tag")) {
                "peer_connected" -> { /* update peer list */ }
                "call_in" -> {
                    val msg = ev.getJSONObject("data").getJSONObject("message")
                    when (msg.getString("type")) {
                        "call_request" -> showIncomingCall(msg)
                        "sdp_offer" -> acceptOfferAndAnswer(msg.getString("sdp"))
                        "ice_candidate" -> addRemoteIce(msg.getString("candidate"))
                    }
                }
                "chat_in" -> { /* append message to chat UI */ }
            }
        }
    }

    override fun onStatus(status: String) {
        // e.g. "networking core online, peer id: 12D3KooW..."
    }

    private fun showIncomingCall(msg: JSONObject) { /* show accept/decline */ }

    private fun acceptOfferAndAnswer(sdp: String) {
        // 1. create RTCPeerConnection, setRemoteDescription(offer)
        // 2. createAnswer, setLocalDescription
        // 3. native.sendCallMessage(peerId, callMessageJson("sdp_answer", answer))
        // Media flows directly peer-to-peer over WebRTC.
    }

    private fun addRemoteIce(candidate: String) {
        // pc.addIceCandidate(...)
    }

    override fun onDestroy() {
        native.close()
        super.onDestroy()
    }
}
