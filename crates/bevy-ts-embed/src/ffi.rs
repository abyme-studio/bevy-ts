//! The C ABI (`include/bevy_ts_embed.h`): a host creates one engine on a native window and
//! drives it. Every call returns at once except `bevy_ts_surface` with no window and
//! `bevy_ts_destroy`, which wait (engine-embedding). A panic never crosses the boundary.

use crate::engine::{Engine, Events, Surface};
use crate::window::NativeWindow;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Arc;

/// The ABI number: raised whenever a function, its arguments, the config keys or the event
/// shapes change incompatibly, and kept across bevy-ts versions otherwise.
pub const ABI: u32 = 1;

/// The C ABI's opaque engine.
pub struct BevyTsEngine(Engine);

/// `void (*)(void *user, const char *json, size_t len)`: one event, NUL-terminated too.
pub type EventFn = unsafe extern "C" fn(user: *mut c_void, json: *const c_char, len: usize);

/// The host's `user` pointer, handed back on every event.
struct User(*mut c_void);
// SAFETY: the host promises its callback may be called from the engine's thread.
unsafe impl Send for User {}
unsafe impl Sync for User {}

impl User {
    fn ptr(&self) -> *mut c_void {
        self.0
    }
}

fn events(on_event: EventFn, user: *mut c_void) -> Events {
    let user = User(user);
    Arc::new(move |json: &str| {
        let len = json.len();
        let Ok(c) = CString::new(json) else { return };
        unsafe { on_event(user.ptr(), c.as_ptr(), len) };
    })
}

fn set_error(error: *mut *mut c_char, message: String) {
    if error.is_null() {
        return;
    }
    let c = CString::new(message.replace('\0', " ")).expect("no NUL");
    unsafe { *error = c.into_raw() };
}

/// # Safety
/// `window` is null or a live native window ([`NativeWindow::from_ptr`]).
unsafe fn surface(window: *mut c_void, width: u32, height: u32, scale: f32) -> Surface {
    let window = if window.is_null() { None } else { unsafe { NativeWindow::from_ptr(window) } };
    Surface { window, width, height, scale }
}

/// Creates the engine on `window` (or none) and returns it, or null with `*error` set.
///
/// # Safety
/// `config_json` is a NUL-terminated string; `error` is null or writable; `on_event` may be
/// called from another thread with `user` until `bevy_ts_destroy` returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_create(
    config_json: *const c_char,
    window: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
    on_event: EventFn,
    user: *mut c_void,
    error: *mut *mut c_char,
) -> *mut BevyTsEngine {
    let created = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if config_json.is_null() {
            return Err("config_json is null".to_string());
        }
        let config = unsafe { CStr::from_ptr(config_json) }.to_str().map_err(|e| format!("config: {e}"))?;
        let surface = unsafe { surface(window, width, height, scale) };
        Engine::create(config, surface, events(on_event, user), None)
    }));
    match created {
        Ok(Ok(engine)) => Box::into_raw(Box::new(BevyTsEngine(engine))),
        Ok(Err(e)) => {
            set_error(error, e);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(error, "bevy_ts_create panicked".into());
            std::ptr::null_mut()
        }
    }
}

fn with(engine: *mut BevyTsEngine, f: impl FnOnce(&Engine)) {
    if engine.is_null() {
        return;
    }
    let engine = unsafe { &*engine };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&engine.0)));
}

/// A new window, the same one at a new size, or null: then it returns once the engine stopped
/// using the old window. Never call it with null from inside the event callback.
///
/// # Safety
/// `engine` is from `bevy_ts_create`; `window` is null or a live native window.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_surface(engine: *mut BevyTsEngine, window: *mut c_void, width: u32, height: u32, scale: f32) {
    with(engine, |e| e.surface(unsafe { surface(window, width, height, scale) }));
}

/// # Safety
/// `engine` is from `bevy_ts_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_pause(engine: *mut BevyTsEngine) {
    with(engine, Engine::pause);
}

/// # Safety
/// `engine` is from `bevy_ts_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_resume(engine: *mut BevyTsEngine) {
    with(engine, Engine::resume);
}

/// One SURFACE.md `ViewportInput` event as JSON, `len` bytes.
///
/// # Safety
/// `engine` is from `bevy_ts_create`; `json` points at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_input(engine: *mut BevyTsEngine, json: *const c_char, len: usize) {
    if json.is_null() {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(json.cast::<u8>(), len) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    with(engine, |e| e.input(text));
}

/// Stops the engine, sends `exited`, joins its thread and frees it. Never call it from inside the
/// event callback.
///
/// # Safety
/// `engine` is from `bevy_ts_create` and not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_destroy(engine: *mut BevyTsEngine) {
    if engine.is_null() {
        return;
    }
    let engine = unsafe { Box::from_raw(engine) };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.0.destroy()));
}

/// Frees a string the ABI returned.
///
/// # Safety
/// `s` is null or from this library, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bevy_ts_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// The ABI number ([`ABI`]).
#[unsafe(no_mangle)]
pub extern "C" fn bevy_ts_abi() -> u32 {
    ABI
}

/// The bevy-ts version, a static NUL-terminated string.
#[unsafe(no_mangle)]
pub extern "C" fn bevy_ts_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}
