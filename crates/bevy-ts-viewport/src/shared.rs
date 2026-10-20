//! State the main world, the render world, the channel's receive thread and the GPU thread share.

use crate::mach::{Incoming, Service};
use crate::surface::{SLOTS, Surface};
use bevy::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

/// Ticks older than this no longer pace frames.
pub(crate) const TICK_TIMEOUT: Duration = Duration::from_millis(100);

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
    /// The engine's update time (CPU) and this viewport's submit-to-completion time (GPU), the
    /// last `LATENCY_SAMPLES` of each, in ns.
    update: Mutex<VecDeque<u64>>,
    gpu: Mutex<VecDeque<u64>>,
}

fn push_sample(q: &Mutex<VecDeque<u64>>, ns: u64) {
    let mut l = q.lock().unwrap();
    if l.len() == LATENCY_SAMPLES {
        l.pop_front();
    }
    l.push_back(ns);
}

/// The `q` quantile of `samples` in ms.
fn quantile(samples: &Mutex<VecDeque<u64>>) -> impl Fn(f64) -> Option<f64> {
    let mut v: Vec<u64> = samples.lock().unwrap().iter().copied().collect();
    v.sort_unstable();
    move |q: f64| (!v.is_empty()).then(|| v[((v.len() - 1) as f64 * q).round() as usize] as f64 / 1e6)
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
        push_sample(&self.latency, ns);
    }

    /// One engine update took `ns`.
    pub(crate) fn add_update(&self, ns: u64) {
        push_sample(&self.update, ns);
    }

    /// `{ samples, medianMs, p95Ms }`.
    pub fn latency(&self) -> Value {
        let n = self.latency.lock().unwrap().len();
        let at = quantile(&self.latency);
        json!({ "samples": n, "medianMs": at(0.5), "p95Ms": at(0.95) })
    }

    /// `{ samples, cpuP50Ms, cpuP99Ms, gpuP50Ms, gpuP99Ms }` over the last 1000 updates and frames.
    pub fn frame_times(&self) -> Value {
        let n = self.update.lock().unwrap().len().max(self.gpu.lock().unwrap().len());
        let (cpu, gpu) = (quantile(&self.update), quantile(&self.gpu));
        json!({ "samples": n, "cpuP50Ms": cpu(0.5), "cpuP99Ms": cpu(0.99), "gpuP50Ms": gpu(0.5), "gpuP99Ms": gpu(0.99) })
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
    /// A tick arrived and no update has consumed it; one that arrives during an update is kept,
    /// so the next update starts as soon as that one ends.
    pub tick: bool,
    /// Something other than a tick arrived (input, resize, a channel event).
    pub other: bool,
    /// When the last tick arrived.
    pub last_tick: Option<std::time::Instant>,
    /// An update is running.
    pub updating: bool,
}

impl Wake {
    /// A tick arrived: kept, even during an update, unless one is already waiting. False when
    /// dropped.
    pub(crate) fn keep_tick(&mut self) -> bool {
        !std::mem::replace(&mut self.tick, true)
    }
}

/// What an engine's viewports share: the runner's wake, edit mode, and the viewports
/// themselves, the game's first, so the leading viewport's ticks pace the runner.
#[derive(Default)]
pub struct Hub {
    pub(crate) wake: Mutex<Wake>,
    pub(crate) wake_cv: Condvar,
    /// Edit mode: idle until something asks for a frame.
    edit: AtomicBool,
    viewports: Mutex<Vec<Weak<Shared>>>,
    /// The Metal device's allocated bytes, sampled by the render world.
    pub(crate) gpu_allocated: AtomicU64,
}

impl Hub {
    fn viewports(&self) -> Vec<Arc<Shared>> {
        let mut list = self.viewports.lock().unwrap();
        list.retain(|w| w.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    }
}

/// One viewport's state: its channel, ring, pacing, input queue and counters.
pub struct Shared {
    pub(crate) hub: Arc<Hub>,
    service: Mutex<Option<Service>>,
    /// Pacing before the first tick and when ticks stop, in Hz (bits of an f64).
    hz: AtomicU64,
    /// When this viewport's viewer last ticked.
    last_tick: Mutex<Option<Instant>>,
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
            hub: Arc::new(Hub::default()),
            service: Mutex::new(None),
            hz: AtomicU64::new(60f64.to_bits()),
            last_tick: Mutex::new(None),
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
    /// The game viewport, the first of a new hub.
    pub fn game() -> Arc<Shared> {
        let s = Arc::new(Shared::default());
        s.hub.viewports.lock().unwrap().push(Arc::downgrade(&s));
        s
    }

    /// Another viewport on this one's hub, paced by the same runner.
    pub fn sibling(&self) -> Arc<Shared> {
        let s = Arc::new(Shared { hub: self.hub.clone(), ..Shared::default() });
        self.hub.viewports.lock().unwrap().push(Arc::downgrade(&s));
        s
    }

    /// Whether this viewport renders and its viewer ticked lately.
    fn ticking(&self, now: Instant) -> bool {
        self.should_render() && self.last_tick.lock().unwrap().is_some_and(|t| now.duration_since(t) < TICK_TIMEOUT)
    }

    /// Whether this viewport's ticks pace the runner: the first ticking viewport leads, the
    /// game's while it renders, so a preview never adds updates to a visible game. A hidden or
    /// detached viewport's ticks pace nothing: its viewer's display link may keep running.
    fn leads(&self, now: Instant) -> bool {
        if !self.should_render() {
            return false;
        }
        match self.hub.viewports().into_iter().find(|s| s.ticking(now)) {
            Some(leader) => std::ptr::eq(Arc::as_ptr(&leader), self),
            None => true,
        }
    }

    /// What the GPU device has allocated, in bytes, when the render world has said.
    pub fn gpu_allocated(&self) -> Option<u64> {
        Some(self.hub.gpu_allocated.load(Ordering::Relaxed)).filter(|b| *b > 0)
    }

    /// Whether any viewport of the hub renders.
    pub fn any_rendering(&self) -> bool {
        self.should_render() || self.hub.viewports().iter().any(|s| s.should_render())
    }

    /// The pace while no ticks come: the first rendering viewport's attach `hz`.
    pub fn pace_hz(&self) -> f64 {
        let list = self.hub.viewports();
        list.iter().find(|s| s.should_render()).map_or_else(|| self.hz(), |s| s.hz())
    }

    /// Checks in `name` as the viewport's channel, replacing any earlier one; `inputs` are the
    /// `input` types its world reads.
    pub fn open(self: &Arc<Self>, name: &str, inputs: &'static [&'static str]) -> Result<(), String> {
        self.close();
        let tx = self.inbox_tx.lock().unwrap().clone();
        let me = Arc::downgrade(self);
        let service = Service::open(name, inputs, move |m| {
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
                crate::trace::mark("msg", 0, 0);
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
        let now = Instant::now();
        *self.last_tick.lock().unwrap() = Some(now);
        if !self.leads(now) {
            return;
        }
        let mut w = self.hub.wake.lock().unwrap();
        w.last_tick = Some(std::time::Instant::now());
        crate::trace::mark("tick", w.updating as u64, w.tick as u64);
        if !(crate::trace::earlier_pacing() && w.updating) && w.keep_tick() {
            self.hub.wake_cv.notify_all();
        } else {
            self.stats.dropped_ticks.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wakes the runner for something other than a tick.
    pub fn poke(&self) {
        self.hub.wake.lock().unwrap().other = true;
        self.hub.wake_cv.notify_all();
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
        self.hub.edit.store(on, Ordering::Release);
        self.poke();
    }

    pub fn edit(&self) -> bool {
        self.hub.edit.load(Ordering::Acquire)
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
        let gpu = submitted.elapsed().as_nanos() as u64;
        self.stats.gpu_ns.fetch_add(gpu, Ordering::Relaxed);
        push_sample(&self.stats.gpu, gpu);
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
        crate::trace::mark("done", params["seq"].as_u64().unwrap_or(0), update);
        self.stats.frames.fetch_add(1, Ordering::Relaxed);
        self.send("frame", params);
        self.input_shown(update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_viewports_ticks_pace_nothing() {
        let game = Shared::game();
        let preview = game.sibling();
        game.on_tick();
        preview.on_tick();
        assert!(!game.hub.wake.lock().unwrap().tick, "no viewport renders: their display links do not drive updates");
        assert!(!game.any_rendering());
    }

    #[test]
    fn a_tick_during_an_update_is_kept_and_a_second_dropped() {
        let mut w = Wake { updating: true, ..Wake::default() };
        assert!(w.keep_tick(), "the tick waits for the update to end");
        assert!(w.tick);
        assert!(!w.keep_tick(), "a second is dropped");
    }
}
