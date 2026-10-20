//! The frame's threads (main, render, compute pool) run in the user-interactive QoS class while a
//! viewport is attached and visible, as an app's main and render threads do, so a busy machine's
//! background work (builds) does not delay each handoff of the frame. Hidden, detached or outside
//! viewport mode they keep their default class. A thread registers itself; the runner raises or
//! lowers every registered thread with a QoS override, which one thread can put on another.
//! `BEVY_TS_VIEWPORT_PACING=0` leaves them at default, for measuring.

use std::ffi::c_void;
use std::sync::Mutex;

unsafe extern "C" {
    fn pthread_self() -> *mut c_void;
    fn pthread_override_qos_class_start_np(thread: *mut c_void, qos: u32, relative_priority: i32) -> *mut c_void;
    fn pthread_override_qos_class_end_np(over: *mut c_void) -> i32;
}

const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

/// Registered threads, and the override on each while raised (pointers as `usize`).
struct Threads {
    list: Vec<(usize, Option<usize>)>,
    raised: bool,
}

static THREADS: Mutex<Threads> = Mutex::new(Threads { list: Vec::new(), raised: false });

fn start(thread: usize) -> Option<usize> {
    let o = unsafe { pthread_override_qos_class_start_np(thread as *mut c_void, QOS_CLASS_USER_INTERACTIVE, 0) };
    (!o.is_null()).then_some(o as usize)
}

fn end(over: usize) {
    unsafe { pthread_override_qos_class_end_np(over as *mut c_void) };
}

/// Adds the calling thread to the frame's threads, raised at once if they are raised.
pub fn register_this_thread() {
    let me = unsafe { pthread_self() } as usize;
    let mut t = THREADS.lock().unwrap();
    if t.list.iter().all(|(th, _)| *th != me) {
        let over = if t.raised { start(me) } else { None };
        t.list.push((me, over));
    }
}

/// Removes the calling thread, before it exits.
pub fn unregister_this_thread() {
    let me = unsafe { pthread_self() } as usize;
    let mut t = THREADS.lock().unwrap();
    t.list.retain(|(th, over)| {
        if *th == me && let Some(o) = over {
            end(*o);
        }
        *th != me
    });
}

/// Raises the frame's threads while `on` (a viewport renders), else lets them back down.
pub(crate) fn set(on: bool) {
    let on = on && !crate::trace::earlier_pacing();
    let mut t = THREADS.lock().unwrap();
    if t.raised == on {
        return;
    }
    t.raised = on;
    crate::trace::mark("qos", on as u64, t.list.len() as u64);
    for (th, over) in t.list.iter_mut() {
        match (on, over.take()) {
            (true, _) => *over = start(*th),
            (false, Some(o)) => end(o),
            (false, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn qos_class_self() -> u32;
    }

    #[test]
    fn a_registered_thread_is_raised_while_on_and_back_after() {
        let base = unsafe { qos_class_self() };
        register_this_thread();
        set(true);
        // An override raises the effective class; `qos_class_self` reports the thread's own class,
        // so this checks the bookkeeping: one override held, none after.
        assert!(THREADS.lock().unwrap().list.iter().any(|(_, o)| o.is_some()));
        set(false);
        assert!(THREADS.lock().unwrap().list.iter().all(|(_, o)| o.is_none()));
        unregister_this_thread();
        assert_eq!(unsafe { qos_class_self() }, base);
    }
}
