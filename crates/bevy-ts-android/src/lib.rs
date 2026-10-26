//! `libbevy_ts_android.so`: the embedding API (`bevy-ts-embed`) as the native methods of
//! `dev.bevyts.Engine` (`java/dev/bevyts/Engine.java`). A `Surface` becomes an `ANativeWindow`
//! here; the engine takes its own reference and this layer drops its own after the call. The
//! listener is called on the engine's thread, attached to the VM once. Android only.

#![cfg(target_os = "android")]

use bevy_ts_embed::engine::{Engine, Events, Surface};
use bevy_ts_embed::window::NativeWindow;
use jni::JNIEnv;
use jni::JavaVM;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{jfloat, jint, jlong, jstring};
use std::ffi::c_void;
use std::sync::Arc;

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

/// The engine's surface from a Java `Surface`, or none for null.
fn surface(env: &mut JNIEnv, java: &JObject, width: jint, height: jint, scale: jfloat) -> Surface {
    let window = if java.is_null() {
        None
    } else {
        let ptr = unsafe { ANativeWindow_fromSurface(env.get_raw(), java.as_raw()) };
        if ptr.is_null() {
            None
        } else {
            // The engine acquires its own reference; this one goes once it has.
            let window = unsafe { NativeWindow::from_ptr(ptr) };
            unsafe { ANativeWindow_release(ptr) };
            window
        }
    };
    Surface { window, width: width.max(0) as u32, height: height.max(0) as u32, scale }
}

/// Calls `listener.onEvent(json)` from the engine's thread, attached once for good.
fn events(vm: JavaVM, listener: GlobalRef) -> Events {
    let vm = Arc::new(vm);
    Arc::new(move |json: &str| {
        let Ok(mut env) = vm.attach_current_thread_permanently() else { return };
        let Ok(text) = env.new_string(json) else { return };
        let _ = env.call_method(listener.as_obj(), "onEvent", "(Ljava/lang/String;)V", &[JValue::Object(&text)]);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        let _ = env.delete_local_ref(text);
    })
}

fn engine<'a>(handle: jlong) -> Option<&'a Engine> {
    (handle != 0).then(|| unsafe { &*(handle as *const Engine) })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeAbi(_: JNIEnv, _: JClass) -> jint {
    bevy_ts_embed::ffi::ABI as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeVersion(env: JNIEnv, _: JClass) -> jstring {
    env.new_string(bevy_ts_embed::play::VERSION).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeCreate(
    mut env: JNIEnv,
    _: JClass,
    config: JString,
    java_surface: JObject,
    width: jint,
    height: jint,
    scale: jfloat,
    listener: JObject,
) -> jlong {
    let created = (|| -> Result<Engine, String> {
        let config: String = env.get_string(&config).map_err(|e| format!("config: {e}"))?.into();
        let vm = env.get_java_vm().map_err(|e| e.to_string())?;
        let listener = env.new_global_ref(listener).map_err(|e| e.to_string())?;
        let surface = surface(&mut env, &java_surface, width, height, scale);
        Engine::create(&config, surface, events(vm, listener), None)
    })();
    match created {
        Ok(engine) => Box::into_raw(Box::new(engine)) as jlong,
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeSurface(mut env: JNIEnv, _: JClass, handle: jlong, java_surface: JObject, width: jint, height: jint, scale: jfloat) {
    if let Some(e) = engine(handle) {
        let s = surface(&mut env, &java_surface, width, height, scale);
        e.surface(s);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativePause(_: JNIEnv, _: JClass, handle: jlong) {
    if let Some(e) = engine(handle) {
        e.pause();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeResume(_: JNIEnv, _: JClass, handle: jlong) {
    if let Some(e) = engine(handle) {
        e.resume();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeInput(mut env: JNIEnv, _: JClass, handle: jlong, json: JString) {
    let Some(e) = engine(handle) else { return };
    if let Ok(text) = env.get_string(&json) {
        e.input(text.into());
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_bevyts_Engine_nativeDestroy(_: JNIEnv, _: JClass, handle: jlong) {
    if handle != 0 {
        let engine = unsafe { Box::from_raw(handle as *mut Engine) };
        engine.destroy();
    }
}

/// android-activity's native-activity backend (linked for bevy_asset, see Cargo.toml) refers to
/// the app's `android_main`, so the library does not load without one. A `NativeActivity` is never
/// pointed at this library: the host is a Java `Activity` calling `Engine`.
#[unsafe(no_mangle)]
fn android_main(_app: android_activity::AndroidApp) {}
