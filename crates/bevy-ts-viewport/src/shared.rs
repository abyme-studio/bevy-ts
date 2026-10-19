//! State the main world, the render world, the channel's receive thread and the GPU thread share.

use crate::mach::{Incoming, Service};
use crate::surface::{SLOTS, Surface};
use bevy::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

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
    pub gpu_ns: AtomicU64,
}

impl Stats {
    pub fn skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
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

pub struct Shared {
    service: Mutex<Option<Service>>,
    visible: AtomicBool,
    resend: AtomicBool,
    slots: Mutex<Slots>,
    target: AtomicU8,
    pub stats: Stats,
    inbox_tx: Mutex<Sender<Incoming>>,
    inbox_rx: Mutex<Receiver<Incoming>>,
}

impl Default for Shared {
    fn default() -> Self {
        let (tx, rx) = channel();
        Shared {
            service: Mutex::new(None),
            visible: AtomicBool::new(false),
            resend: AtomicBool::new(false),
            slots: Mutex::new(Slots::default()),
            target: AtomicU8::new(0),
            stats: Stats::default(),
            inbox_tx: Mutex::new(tx),
            inbox_rx: Mutex::new(rx),
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
                    _ => {}
                }
            }
            let _ = tx.send(m);
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
    }

    pub fn should_render(&self) -> bool {
        self.visible.load(Ordering::Acquire) && self.attached()
    }

    /// Messages from the viewer since the last call, in order.
    pub fn drain(&self) -> Vec<Incoming> {
        self.inbox_rx.lock().unwrap().try_iter().collect()
    }

    /// Queues a message as if the viewer had sent it (HTTP input goes this way).
    pub fn push(&self, m: Incoming) {
        let _ = self.inbox_tx.lock().unwrap().send(m);
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

    /// The GPU finished `slot` of `ring`: tell the viewer.
    pub(crate) fn announce(&self, ring: u32, slot: usize, mut params: Value) {
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
    }
}
