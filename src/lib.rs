use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jfloat, jint};

mod text;
mod dom;
mod transport;
mod text_layout;
mod event_loop;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_photon_PhotonApi_run(env: JNIEnv,
                                                 class: JClass,
                                                 port: jint) {
    event_loop::run_event_loop(("localhost", port as u16));
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_photon_PhotonApi_applyUpdates(env: JNIEnv,
                                                          class: JClass,
                                                          updates: JString) {

}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_photon_PhotonApi_measureText(env: JNIEnv,
                                                         class: JClass) -> jfloat {
    0.0
}