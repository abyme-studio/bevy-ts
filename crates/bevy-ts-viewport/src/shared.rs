//! State the main world, the render world, the channel's receive thread and the GPU thread share.

use crate::mach::{Incoming, Service};
use crate::surface::{SLOTS, Surface};
use bevy::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// `CLOCK_UPTIME_RAW` in nanoseconds, the clock `frame` and `tick` timestamps use.
pub fn now_ns() -> u64 {
    unsafe { libc_clock_gettime_nsec_np(8) }
}

unsafe extern "C" {
    #[link_name = "clock_gettime_nsec_np"]
    fn libc_clock_gettime_nsec_np(clock: u32) -> u64;
}

/// Frame counters for `bevy-ts/viewport_stats`.
#[derive(Default)]
pub struct Stats {
    pub frames: AtomicU64,
    pub skipped: AtomicU64,
    pub dropped_ticks: AtomicU64,
    /// Submit-to-completion time of announced frames, summed, and how many.
    pub gpu_ns: AtomicU64,
    pub gpu_frames: AtomicU64,
    /// Input arrival to the frame that first reflects it, the last `LATENCY_SAMPLES`, in ns.
    latency: Mutex<VecDeque<u64>>,
}

const LATENCY_SAMPLES: usize = 1000;

impl Stats {
    pub fn skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn gpu_ms(&self) -> Option<f64> {
        let n = self.gpu_frames.load(Ordering::Relaxed);
        (n > 0).then(|| self.gpu_ns.load(Ordering::Relaxed) as f64 / n as f64 / 1e6)
    }

    pub(crate) fn add_latency(&self, ns: u64) {
        let mut l = self.latency.lock().unwrap();
        if l.len() == LATENCY_SAMPLES {
            l.pop_front();
        }
        l.push_back(ns);
    }

    /// `{ samples, medianMs, p95Ms }`.
    pub fn latency(&self) -> Value {
        let mut v: Vec<u64> = self.latency.lock().unwrap().iter().copied().collect();
        v.sort_unstable();
        let at = |q: f64| (!v.is_empty()).then(|| v[((v.len() - 1) as f64 * q).round() as usize] as f64 / 1e6);
        json!({ "samples": v.len(), "medianMs": at(0.5), "p95Ms": at(0.95) })
    }
}

/// What the render world did with the virtual window this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Target {
    Idle = 0,
    Rendering = 1,
}

#[derive(Default)]
struct Slots {
    /// The slot last announced, as (ring, slot).
    last: Option<(u32, usize)>,
    /// Slots submitted and not yet announced, by the ring they belong to.
    pending: [Option<u32>; SLOTS],
    /// The newest ring a frame was announced from.
    announced_ring: Option<u32>,
    ring: u32,
}

/// What wakes the runner: a tick, input or another message, under one lock with a condvar.
#[derive(Default)]
pub(crate) struct Wake {
    /// A tick arrived and no update has consumed it.
    pub tick: bool,
    /// Something other than a tick arrived (input, resize, a channel event).
    pub other: bool,
    /// When the last tick arrived.
    pub last_tick: Option<std::time::Instant>,
    /// An update is running; a tick now is dropped.
    pub updating: bool,
}

pub struct Shared {
    service: Mutex<Option<Service>>,
    pub(crate) wake: Mutex<Wake>,
    pub(crate) wake_cv: std::sync::Condvar,
    /// Pacing before the first tick and when ticks stop, in Hz (bits of an f64).
    hz: AtomicU64,
    /// Edit mode: idle until something asks for a frame.
    edit: AtomicBool,
    /// The hole has keyboard focus.
    focused: AtomicBool,
    /// Gamepad input reaches the game without focus.
    gamepad_unfocused: AtomicBool,
    visible: AtomicBool,
    resend: AtomicBool,
    slots: Mutex<Slots>,
    target: AtomicU8,
    pub stats: Stats,
    inbox_tx: Mutex<Sender<(Instant, Incoming)>>,
    inbox_rx: Mutex<Receiver<(Instant, Incoming)>>,
    /// Arrival times of input applied by each update, waiting for the frame that shows it.
    input_marks: Mutex<VecDeque<(u64, Instant)>>,
    /// The ring in use and its allocation, for stats.
    ring_info: Mutex<Option<(u32, u32, u32)>>,
}

impl Default for Shared {
    fn default() -> Self {
        let (tx, rx) = channel();
        Shared {
            service: Mutex::new(None),
            wake: Mutex::new(Wake::default()),
            wake_cv: std::sync::Condvar::new(),
            hz: AtomicU64::new(60f64.to_bits()),
            edit: AtomicBool::new(false),
            focused: AtomicBool::new(false),
            gamepad_unfocused: AtomicBool::new(false),
            visible: AtomicBool::new(false),
            resend: AtomicBool::new(false),
            slots: Mutex::new(Slots::default()),
            target: AtomicU8::new(0),
            stats: Stats::default(),
            inbox_tx: Mutex::new(tx),
            inbox_rx: Mutex::new(rx),
            input_marks: Mutex::new(VecDeque::new()),
            ring_info: Mutex::new(None),
        }
    }
}

/// The shared state, in both worlds.
#[derive(Resource, Clone)]
pub struct SharedRes(pub Arc<Shared>);

impl Shared {
    /// Checks in `name` as the viewport's channel, replacing any earlier one.
    pub fn open(self: &Arc<Self>, name: &str) -> Result<(), String> {
        self.close();
        let tx = self.inbox_tx.lock().unwrap().clone();
        let me = Arc::downgrade(self);
        let service = Service::open(name, move |m| {
            if let Some(me) = me.upgrade() {
                match &m {
                    Incoming::Hello => {
                        me.visible.store(true, Ordering::Release);
                        me.resend.store(true, Ordering::Release);
                    }
                    Incoming::Gone => me.visible.store(false, Ordering::Release),
                    Incoming::Message { method, .. } if method == "tick" => {
                        me.on_tick();
                        return;
                    }
                    Incoming::Message { method, params, .. } if method == "visible" => {
                        me.visible.store(params["on"].as_bool().unwrap_or(true), Ordering::Release);
                    }
                    _ => {}
                }
                let _ = tx.send((Instant::now(), m));
                me.poke();
            }
        })?;
        *self.slots.lock().unwrap() = Slots::default();
        *self.service.lock().unwrap() = Some(service);
        Ok(())
    }

    /// Ends the channel; the ring goes with the next frame.
    pub fn close(&self) {
        self.visible.store(false, Ordering::Release);
        self.service.lock().unwrap().take();
    }

    pub fn service_name(&self) -> Option<String> {
        self.service.lock().unwrap().as_ref().map(|s| s.name.clone())
    }

    pub fn attached(&self) -> bool {
        self.service.lock().unwrap().as_ref().is_some_and(Service::attached)
    }

    pub fn set_visible(&self, on: bool) {
        self.visible.store(on, Ordering::Release);
        self.poke();
    }

    pub fn should_render(&self) -> bool {
        self.visible.load(Ordering::Acquire) && self.attached()
    }

    /// Messages from the viewer since the last call, in order, with when each arrived.
    pub fn drain(&self) -> Vec<(Instant, Incoming)> {
        self.inbox_rx.lock().unwrap().try_iter().collect()
    }

    /// Queues a message as if the viewer had sent it (HTTP input goes this way).
    pub fn push(&self, m: Incoming) {
        let _ = self.inbox_tx.lock().unwrap().send((Instant::now(), m));
        self.poke();
    }

    /// Update `update` applied input that arrived at `at`.
    pub(crate) fn mark_input(&self, update: u64, at: Instant) {
        self.input_marks.lock().unwrap().push_back((update, at));
    }

    /// A frame of update `update` was announced: input it reflects is measured.
    fn input_shown(&self, update: u64) {
        let mut marks = self.input_marks.lock().unwrap();
        while marks.front().is_some_and(|(u, _)| *u <= update) {
            let (_, at) = marks.pop_front().unwrap();
            self.stats.add_latency(at.elapsed().as_nanos() as u64);
        }
    }

    pub(crate) fn set_ring_info(&self, info: Option<(u32, u32, u32)>) {
        *self.ring_info.lock().unwrap() = info;
    }

    /// `{ ring, w, h }` of the ring in use.
    pub fn ring_info(&self) -> Value {
        match *self.ring_info.lock().unwrap() {
            Some((ring, w, h)) => json!({ "ring": ring, "w": w, "h": h }),
            None => Value::Null,
        }
    }

    fn on_tick(&self) {
        let mut w = self.wake.lock().unwrap();
        w.last_tick = Some(std::time::Instant::now());
        if w.updating || w.tick {
            self.stats.dropped_ticks.fetch_add(1, Ordering::Relaxed);
        } else {
            w.tick = true;
            self.wake_cv.notify_all();
        }
    }

    /// Wakes the runner for something other than a tick.
    pub fn poke(&self) {
        self.wake.lock().unwrap().other = true;
        self.wake_cv.notify_all();
    }

    pub fn set_hz(&self, hz: f64) {
        if hz.is_finite() && hz > 0.0 {
            self.hz.store(hz.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn hz(&self) -> f64 {
        f64::from_bits(self.hz.load(Ordering::Relaxed))
    }

    pub fn set_edit(&self, on: bool) {
        self.edit.store(on, Ordering::Release);
        self.poke();
    }

    pub fn edit(&self) -> bool {
        self.edit.load(Ordering::Acquire)
    }

    pub fn set_focused(&self, on: bool) {
        self.focused.store(on, Ordering::Release);
    }

    pub fn set_gamepad_unfocused(&self, on: bool) {
        self.gamepad_unfocused.store(on, Ordering::Release);
    }

    pub fn gamepad_unfocused(&self) -> bool {
        self.gamepad_unfocused.load(Ordering::Acquire)
    }

    pub fn focused(&self) -> bool {
        self.focused.load(Ordering::Acquire)
    }

    pub fn visible(&self) -> bool {
        self.visible.load(Ordering::Acquire)
    }

    pub fn send(&self, method: &str, params: Value) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            let _ = s.send(method, params);
        }
    }

    pub(crate) fn resend_surfaces(&self) {
        self.resend.store(true, Ordering::Release);
    }

    pub(crate) fn take_resend(&self) -> bool {
        self.resend.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn send_surface(&self, ring: u32, slot: usize, surface: &Surface) {
        if let Some(s) = self.service.lock().unwrap().as_ref() {
            let params = json!({ "ring": ring, "slot": slot, "w": surface.width(), "h": surface.height() });
            let _ = s.send_port("surface", params, surface.mach_port());
        }
    }

    pub(crate) fn set_ring(&self, ring: u32) {
        self.slots.lock().unwrap().ring = ring;
    }

    pub(crate) fn set_target(&self, t: Target) {
        self.target.store(t as u8, Ordering::Release);
    }

    pub fn rendering(&self) -> bool {
        self.target.load(Ordering::Acquire) == Target::Rendering as u8
    }

    /// A slot of `ring` that is not the last announced, not submitted and unannounced, and not
    /// in use by another process; the one after the last announced first.
    pub(crate) fn pick_slot(&self, ring: u32, in_use: impl Fn(usize) -> bool) -> Option<usize> {
        let s = self.slots.lock().unwrap();
        let after = match s.last {
            Some((r, i)) if r == ring => i,
            _ => SLOTS - 1,
        };
        (1..=SLOTS).map(|k| (after + k) % SLOTS).find(|&i| s.last != Some((ring, i)) && s.pending[i] != Some(ring) && !in_use(i))
    }

    pub(crate) fn mark_pending(&self, slot: usize) {
        let mut s = self.slots.lock().unwrap();
        s.pending[slot] = Some(s.ring);
    }

    pub(crate) fn announced_ring(&self) -> Option<u32> {
        self.slots.lock().unwrap().announced_ring
    }

    /// The GPU finished `slot` of `ring`, submitted at `submitted` for main-world update
    /// `update`: tell the viewer.
    pub(crate) fn announce(&self, ring: u32, slot: usize, submitted: Instant, update: u64, mut params: Value) {
        self.stats.gpu_ns.fetch_add(submitted.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.stats.gpu_frames.fetch_add(1, Ordering::Relaxed);
        {
            let mut s = self.slots.lock().unwrap();
            if s.pending[slot] == Some(ring) {
                s.pending[slot] = None;
            }
            s.last = Some((ring, slot));
            s.announced_ring = Some(s.announced_ring.map_or(ring, |r| r.max(ring)));
        }
        params["ts"] = json!(now_ns());
        self.stats.frames.fetch_add(1, Ordering::Relaxed);
        self.send("frame", params);
        self.input_shown(update);
    }
}
