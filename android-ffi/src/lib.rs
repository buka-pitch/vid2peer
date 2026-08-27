//! C ABI bridge between the Android app (Kotlin via JNI) and the Rust
//! networking core.
//!
//! Design principles:
//!   * The Rust layer is fully independent from the UI.
//!   * The app obtains a peer handle (`p2pvc_init`), then:
//!       - calls actions (`p2pvc_send_chat`, `p2pvc_send_call_message`, ...)
//!       - drains events with `p2pvc_next_event` (JSON, one per call)
//!   * All string payloads are UTF-8 JSON; the app is never given the private
//!     key — identity stays inside the Rust core.
//!
//! Build for Android (from the workspace root):
//!   cargo build -p p2p-video-chat-android-ffi --release --target aarch64-linux-android

#![allow(clippy::missing_safety_doc)]

use p2p_video_chat_core::network::{P2pConfig, Peer};
use p2p_video_chat_core::protocol::CallMessage;
use p2p_video_chat_core::BootstrapPeer;
use serde_json::Value;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, Mutex};

#[cfg(feature = "jni-bridge")]
mod jni;

struct PeerHandle {
    rt: Runtime,
    peer: Arc<Mutex<Peer>>,
    rx: mpsc::UnboundedReceiver<p2p_video_chat_core::Event>,
}

fn cstr<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn to_c_string(s: String) -> *mut c_char {
    CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Parse the JSON config into a `P2pConfig`.
fn config_from_json(json: &str) -> Result<P2pConfig, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut cfg = P2pConfig::default();
    cfg.enable_mdns = v.get("enable_mdns").and_then(|x| x.as_bool()).unwrap_or(false);

    if let Some(addrs) = v.get("listen_addrs").and_then(|x| x.as_array()) {
        let mut parsed = Vec::new();
        for a in addrs {
            if let Some(s) = a.as_str() {
                parsed.push(s.parse().map_err(|e| format!("bad listen addr {s}: {e}"))?);
            }
        }
        cfg.listen_addrs = parsed;
    }
    if let Some(idf) = v.get("identity_file").and_then(|x| x.as_str()) {
        cfg.identity_file = Some(idf.into());
    }
    if let Some(peers) = v.get("bootstrap_peers").and_then(|x| x.as_array()) {
        let mut parsed = Vec::new();
        for p in peers {
            let peer_id = p.get("peer_id").and_then(|x| x.as_str()).unwrap_or("").parse()
                .map_err(|e| format!("bad peer id: {e}"))?;
            let address = p.get("address").and_then(|x| x.as_str()).unwrap_or("").parse()
                .map_err(|e| format!("bad address: {e}"))?;
            parsed.push(BootstrapPeer { peer_id, address });
        }
        cfg.bootstrap_peers = parsed;
    }
    if let Some(relays) = v.get("relay_servers").and_then(|x| x.as_array()) {
        let mut parsed = Vec::new();
        for p in relays {
            let peer_id = p.get("peer_id").and_then(|x| x.as_str()).unwrap_or("").parse()
                .map_err(|e| format!("bad relay peer id: {e}"))?;
            let address = p.get("address").and_then(|x| x.as_str()).unwrap_or("").parse()
                .map_err(|e| format!("bad relay address: {e}"))?;
            parsed.push(BootstrapPeer { peer_id, address });
        }
        cfg.relay_servers = parsed;
    }
    Ok(cfg)
}

/// JSON-serialize an event for the app layer.
fn event_to_json(ev: &p2p_video_chat_core::Event) -> String {
    use p2p_video_chat_core::Event as E;
    let obj = match ev {
        E::ListeningOn { address } => serde_json::json!({ "address": address.to_string() }),
        E::PeerConnected { peer_id } => serde_json::json!({ "peer_id": peer_id.to_string() }),
        E::PeerDisconnected { peer_id } => serde_json::json!({ "peer_id": peer_id.to_string() }),
        E::ConnectionError { error, .. } => serde_json::json!({ "error": error }),
        E::PeerIdentified(info) => serde_json::json!({
            "peer_id": info.peer_id.to_string(),
            "addresses": info.addresses.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        }),
        E::PingResult { peer_id, rtt } => serde_json::json!({ "peer_id": peer_id.to_string(), "rtt_ms": rtt.as_millis() }),
        E::BootstrapCompleted { peers } => serde_json::json!({ "peers": peers }),
        E::PeersFound { key, peers } => serde_json::json!({
            "key": key.to_string(),
            "peers": peers.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        }),
        E::PeerDiscovered { peer_id } => serde_json::json!({ "peer_id": peer_id.to_string() }),
        E::DhtPutOk { key } => serde_json::json!({ "key": String::from_utf8_lossy(key) }),
        E::DhtGetOk { key, value } => serde_json::json!({
            "key": String::from_utf8_lossy(key),
            "value": String::from_utf8_lossy(value),
        }),
        E::DhtGetNotFound { key } => serde_json::json!({ "key": String::from_utf8_lossy(key) }),
        E::DhtProviding { key } => serde_json::json!({ "key": String::from_utf8_lossy(key) }),
        E::RoutingTableChanged { known_peers } => serde_json::json!({ "known_peers": known_peers }),
        E::ChatMessageReceived { from, message } => serde_json::json!({
            "from": from.to_string(),
            "message": serde_json::to_value(message).unwrap_or_default(),
        }),
        E::ChatMessageDelivered { to, message_id } => serde_json::json!({ "to": to.to_string(), "message_id": message_id }),
        E::ChatError { to, error, .. } => serde_json::json!({ "to": to.to_string(), "error": error }),
        E::CallMessageReceived { from, message } => serde_json::json!({
            "from": from.to_string(),
            "message": serde_json::to_value(message).unwrap_or_default(),
        }),
        E::CallMessageSent { to, call_id, msg_type } => serde_json::json!({ "to": to.to_string(), "call_id": call_id, "type": msg_type }),
        E::CallError { to, error } => serde_json::json!({ "to": to.to_string(), "error": error }),
        E::RelayReservationAccepted { relay_peer_id } => serde_json::json!({ "relay_peer_id": relay_peer_id.to_string() }),
        E::RelayedConnectionEstablished { relay_peer_id } => serde_json::json!({ "relay_peer_id": relay_peer_id.to_string() }),
        E::DcutrEvent { peer_id } => serde_json::json!({ "peer_id": peer_id.to_string() }),
    };
    serde_json::json!({ "tag": ev.tag(), "data": obj }).to_string()
}

/// Borrow a handle without consuming it.
fn borrow_handle<'a>(ptr: *mut c_void) -> Option<&'a mut PeerHandle> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *(ptr as *mut PeerHandle) })
    }
}

/// Initialize a peer. `config_json` is a UTF-8 JSON string (see README).
/// Returns an opaque handle or NULL on error.
#[no_mangle]
pub extern "C" fn p2pvc_init(config_json: *const c_char) -> *mut c_void {
    let cfg = match config_from_json(cstr(config_json)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("p2pvc_init: bad config: {e}");
            return std::ptr::null_mut();
        }
    };
    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("p2pvc_init: runtime: {e}");
            return std::ptr::null_mut();
        }
    };
    let peer = match Peer::new(cfg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("p2pvc_init: peer: {e}");
            return std::ptr::null_mut();
        }
    };
    let peer = Arc::new(Mutex::new(peer));
    let (tx, rx) = mpsc::unbounded_channel();
    {
        let peer = peer.clone();
        rt.spawn(async move {
            loop {
                let ev = peer.lock().await.next_event().await;
                if tx.send(ev).is_err() {
                    break;
                }
            }
        });
    }
    Box::into_raw(Box::new(PeerHandle { rt, peer, rx })) as *mut c_void
}

/// Return the peer id (base58) as a C string; free with `p2pvc_free_string`.
#[no_mangle]
pub extern "C" fn p2pvc_peer_id(handle: *mut c_void) -> *mut c_char {
    let Some(h) = borrow_handle(handle) else { return std::ptr::null_mut() };
    let id = h.rt.block_on(async { h.peer.lock().await.peer_id().to_string() });
    to_c_string(id)
}

/// Block until the next networking event, returned as a JSON C string.
/// Free with `p2pvc_free_string`. Returns NULL when the peer is shutting down.
#[no_mangle]
pub extern "C" fn p2pvc_next_event(handle: *mut c_void) -> *mut c_char {
    let Some(h) = borrow_handle(handle) else { return std::ptr::null_mut() };
    match h.rx.blocking_recv() {
        Some(ev) => to_c_string(event_to_json(&ev)),
        None => std::ptr::null_mut(),
    }
}

/// Join the DHT through the configured bootstrap peers. 0 on success.
#[no_mangle]
pub extern "C" fn p2pvc_bootstrap(handle: *mut c_void) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    h.rt.block_on(async { h.peer.lock().await.bootstrap() }).is_ok() as i32 - 1
}

/// Send a text chat message. Returns 0 on success.
#[no_mangle]
pub extern "C" fn p2pvc_send_chat(handle: *mut c_void, peer_id: *const c_char, text: *const c_char) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    let Ok(to) = cstr(peer_id).parse::<libp2p::PeerId>() else { return -1 };
    let msg = cstr(text).to_string();
    h.rt.block_on(async { h.peer.lock().await.send_chat(&to, msg) }).is_ok() as i32 - 1
}

/// Send a call signaling message. `msg_json` is a serialized `CallMessage`.
#[no_mangle]
pub extern "C" fn p2pvc_send_call_message(handle: *mut c_void, peer_id: *const c_char, msg_json: *const c_char) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    let Ok(to) = cstr(peer_id).parse::<libp2p::PeerId>() else { return -1 };
    let Ok(msg) = serde_json::from_str::<CallMessage>(cstr(msg_json)) else { return -1 };
    h.rt.block_on(async { h.peer.lock().await.send_call_message(&to, msg) }).is_ok() as i32 - 1
}

/// Dial a multiaddr directly (e.g. a bootstrap/relay node).
#[no_mangle]
pub extern "C" fn p2pvc_dial(handle: *mut c_void, multiaddr: *const c_char) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    let Ok(addr) = cstr(multiaddr).parse::<libp2p::Multiaddr>() else { return -1 };
    h.rt.block_on(async { h.peer.lock().await.dial(addr) }).is_ok() as i32 - 1
}

/// Number of open connections. Returns -1 on invalid handle.
#[no_mangle]
pub extern "C" fn p2pvc_connection_count(handle: *mut c_void) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    h.rt.block_on(async { h.peer.lock().await.connection_count() as i32 })
}

/// Routing-table size (known DHT peers). Returns -1 on invalid handle.
#[no_mangle]
pub extern "C" fn p2pvc_routing_table_size(handle: *mut c_void) -> i32 {
    let Some(h) = borrow_handle(handle) else { return -1 };
    h.rt.block_on(async { h.peer.lock().await.routing_table_size() as i32 })
}

/// Free a string returned by this library.
#[no_mangle]
pub extern "C" fn p2pvc_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

/// Shut down and free the peer handle.
#[no_mangle]
pub extern "C" fn p2pvc_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // Consume and drop the handle (drops the runtime and peer).
    unsafe {
        drop(Box::from_raw(handle as *mut PeerHandle));
    }
}
