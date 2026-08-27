//! JNI bridge for Android.
//!
//! The Kotlin app (com.example.p2pvc.P2pvcNative) calls these functions. Each
//! delegates to the plain C-ABI functions in `lib.rs`. Keeping two layers
//! means the core logic stays testable from plain C callers while the JNI
//! surface stays a thin, mechanical wrapper.

#![allow(clippy::missing_safety_doc)]

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

fn to_jstring(env: &mut JNIEnv, s: String) -> jstring {
    env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

macro_rules! jni_stub {
    ($name:ident, $inner:path, $ret:ty) => {
        #[no_mangle]
        pub unsafe extern "system" fn $name(_env: JNIEnv, _class: JClass, handle: jlong) -> $ret {
            $inner(handle as *mut std::ffi::c_void)
        }
    };
}

jni_stub!(Java_com_example_p2pvc_P2pvcNative_nBootstrap, crate::p2pvc_bootstrap, jint);
jni_stub!(Java_com_example_p2pvc_P2pvcNative_nConnectionCount, crate::p2pvc_connection_count, jint);
jni_stub!(Java_com_example_p2pvc_P2pvcNative_nRoutingTableSize, crate::p2pvc_routing_table_size, jint);

/// JNI wrapper for free (returns nothing).
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nFree(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    crate::p2pvc_free(handle as *mut std::ffi::c_void);
}

/// JNI wrapper for init: returns the opaque handle as a jlong.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nInit(
    mut env: JNIEnv,
    _class: JClass,
    config: JString,
) -> jlong {
    let cfg = match env.get_string(&config) {
        Ok(jstr) => jstr.to_string_lossy().into_owned(),
        Err(_) => return 0,
    };
    let mut buf = cfg.into_bytes();
    buf.push(0);
    let ptr = buf.as_mut_ptr() as *const std::ffi::c_char;
    crate::p2pvc_init(ptr) as jlong
}

/// JNI wrapper for peer id: returns a jstring or null.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nPeerId(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let ptr = crate::p2pvc_peer_id(handle as *mut std::ffi::c_void);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
    crate::p2pvc_free_string(ptr);
    to_jstring(&mut env, s)
}

/// JNI wrapper for next_event: returns a jstring (event JSON) or null on shutdown.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nNextEvent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let ptr = crate::p2pvc_next_event(handle as *mut std::ffi::c_void);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
    crate::p2pvc_free_string(ptr);
    to_jstring(&mut env, s)
}

/// JNI wrapper for send_chat.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nSendChat(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    peer_id: JString,
    text: JString,
) -> jint {
    let Ok(pid) = env.get_string(&peer_id) else { return -1 };
    let Ok(txt) = env.get_string(&text) else { return -1 };
    let mut pbuf = pid.to_string_lossy().into_owned().into_bytes();
    let mut tbuf = txt.to_string_lossy().into_owned().into_bytes();
    pbuf.push(0);
    tbuf.push(0);
    let p = pbuf.as_mut_ptr() as *const std::ffi::c_char;
    let t = tbuf.as_mut_ptr() as *const std::ffi::c_char;
    crate::p2pvc_send_chat(handle as *mut std::ffi::c_void, p, t)
}

/// JNI wrapper for send_call_message.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nSendCallMessage(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    peer_id: JString,
    msg_json: JString,
) -> jint {
    let Ok(pid) = env.get_string(&peer_id) else { return -1 };
    let Ok(msg) = env.get_string(&msg_json) else { return -1 };
    let mut pbuf = pid.to_string_lossy().into_owned().into_bytes();
    let mut mbuf = msg.to_string_lossy().into_owned().into_bytes();
    pbuf.push(0);
    mbuf.push(0);
    let p = pbuf.as_mut_ptr() as *const std::ffi::c_char;
    let m = mbuf.as_mut_ptr() as *const std::ffi::c_char;
    crate::p2pvc_send_call_message(handle as *mut std::ffi::c_void, p, m)
}

/// JNI wrapper for dial.
#[no_mangle]
pub unsafe extern "system" fn Java_com_example_p2pvc_P2pvcNative_nDial(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    multiaddr: JString,
) -> jint {
    let Ok(ma) = env.get_string(&multiaddr) else { return -1 };
    let mut buf = ma.to_string_lossy().into_owned().into_bytes();
    buf.push(0);
    let m = buf.as_mut_ptr() as *const std::ffi::c_char;
    crate::p2pvc_dial(handle as *mut std::ffi::c_void, m)
}
