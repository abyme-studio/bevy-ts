//! Engine instances besides the live one: a spare booted ahead of a restart, which the restart
//! takes, and the instance the restart retired, dropped a frame later (stop-rebuild-split,
//! design decision 3).
//!
//! V8 enters an isolate on the thread when rusty_v8 creates it, keeps a per-thread stack of
//! entered isolates, and rusty_v8 drops an isolate only while it is the thread's current one
//! (`OwnedIsolate::drop` asserts it: isolates are dropped in the reverse order of creation). So
//! outside this type exactly one isolate is entered, the live one, and every held instance is
//! exited. Each method here switches the entered isolate with V8's own `Isolate::enter` and
//! `Isolate::exit`, on the main thread, and puts the live one back as the current isolate.

use crate::Fns;
use deno_core::{JsRuntime, v8};
use std::ffi::c_void;

/// A booted instance, exited. `fns` first: handles into the isolate drop before it.
pub(crate) struct Held {
    pub fns: Fns,
    pub runtime: JsRuntime,
}

/// The spare and the retired instance, each exited while held.
#[derive(Default)]
pub(crate) struct Standby {
    spare: Option<Held>,
    retired: Option<Held>,
}

unsafe extern "C" {
    // rusty_v8's binding of `v8::Isolate::GetCurrent`, which it does not export.
    fn v8__Isolate__GetCurrent() -> *mut c_void;
}

/// Whether `rt`'s isolate is the thread's current one.
fn is_current(rt: &mut JsRuntime) -> bool {
    let isolate: &v8::Isolate = rt.v8_isolate();
    // SAFETY: `v8::Isolate` is a `repr(transparent)` pointer to the C++ isolate.
    let ptr = unsafe { *(isolate as *const v8::Isolate as *const *mut c_void) };
    // SAFETY: reads the thread's current isolate, changing nothing.
    ptr == unsafe { v8__Isolate__GetCurrent() }
}

/// Exits `rt`'s isolate, which must be the current one.
fn exit(rt: &mut JsRuntime) {
    debug_assert!(is_current(rt), "exiting an isolate that is not the current one");
    // SAFETY: the isolate is the thread's current one (V8 requires it of `Exit`); the one entered
    // before it becomes current again.
    unsafe { rt.v8_isolate().exit() };
}

/// Enters `rt`'s isolate, held exited until now.
fn enter(rt: &mut JsRuntime) {
    // SAFETY: the isolate is alive and on this thread; V8 saves the current one to restore at
    // its `Exit`.
    unsafe { rt.v8_isolate().enter() };
    debug_assert!(is_current(rt), "entering an isolate did not make it current");
}

impl Standby {
    /// Drops the retired instance, then boots a spare unless one is held. `live` is the current
    /// isolate before and after.
    pub fn prepare(&mut self, live: &mut JsRuntime, boot: impl FnOnce() -> (JsRuntime, Fns)) {
        if let Some(old) = self.retired.take() {
            drop_held(old, live);
        }
        if self.spare.is_some() {
            return;
        }
        debug_assert!(is_current(live), "the live isolate is not the current one");
        // Made and entered over the live one.
        let (mut runtime, fns) = boot();
        exit(&mut runtime);
        debug_assert!(is_current(live), "the live isolate is current again");
        self.spare = Some(Held { fns, runtime });
    }

    /// Whether a spare is held.
    pub fn has_spare(&self) -> bool {
        self.spare.is_some()
    }

    /// Takes the spare in place of `live`, which is retired, exited, for [`Standby::prepare`] to
    /// drop. The spare comes back entered, the current isolate. `Err(live)` without a spare.
    pub fn take(&mut self, mut live: Held) -> Result<Held, Held> {
        let Some(mut spare) = self.spare.take() else { return Err(live) };
        exit(&mut live.runtime);
        enter(&mut spare.runtime);
        // Two restarts in one frame: the older retired one goes now.
        if let Some(old) = self.retired.replace(live) {
            drop_held(old, &mut spare.runtime);
        }
        Ok(spare)
    }

    /// Drops every held instance; `live` is the current isolate before and after.
    pub fn clear(&mut self, live: &mut JsRuntime) {
        for h in [self.retired.take(), self.spare.take()].into_iter().flatten() {
            drop_held(h, live);
        }
    }
}

/// Drops an exited instance: `live` is exited, `h` entered and dropped (which exits it), `live`
/// entered again.
fn drop_held(mut h: Held, live: &mut JsRuntime) {
    exit(live);
    enter(&mut h.runtime);
    drop(h);
    enter(live);
}

impl Drop for Standby {
    fn drop(&mut self) {
        // Its owner clears it first; an instance left would assert in rusty_v8's drop, so it leaks.
        debug_assert!(self.spare.is_none() && self.retired.is_none(), "Standby dropped holding an engine instance");
        std::mem::forget(self.spare.take());
        std::mem::forget(self.retired.take());
    }
}
