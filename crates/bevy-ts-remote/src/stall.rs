//! The stall watchdog: the one owner of "Update stopped". A thread outside the main and render
//! threads watches each frame in two parts, from beats the frame leaves in shared atomics:
//!
//! - the main part, from the start of `First` until the frame's render extract (with no render
//!   world, until the end of the main schedule);
//! - the render part, from that extract until the render world's `Render` has finished.
//!
//! A part open for the limit, in wall time, is a stall. Time between frames is not: a test may
//! hold its app idle. On a stall it writes one `Update stopped:` line on stderr, answers every
//! protocol request still open with that line, then exits, warns or calls the test's function
//! ([`StallAction`]).

use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpMessage, BrpResult, BrpSender, RemotePlugin, error_codes};
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderSystems};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

/// How long a part of a frame may stay open: far past any frame seen, a synchronous pipeline
/// compile under load included.
pub const LIMIT: Duration = Duration::from_secs(30);
/// The exit code of a stalled headless run: `EX_TEMPFAIL`, the run may pass if run again.
pub const EXIT_CODE: i32 = 75;

/// What a stall does once it is reported and the open requests are answered.
#[derive(Clone)]
pub enum StallAction {
    /// Exit the process with this code, once the answers are read (at most 1 s).
    Exit(i32),
    /// Only report it, and say so again when frames come back.
    Warn,
    /// Call this, on the watchdog's thread (for tests).
    Call(Arc<dyn Fn(&Stall) + Send + Sync>),
}

/// The stall watchdog. Its settings go into [`StallSettings`], read at the first frame, so a test
/// may replace them after the app is built.
pub struct StallPlugin {
    pub limit: Duration,
    pub action: StallAction,
}

impl Default for StallPlugin {
    fn default() -> Self {
        StallPlugin { limit: LIMIT, action: StallAction::Exit(EXIT_CODE) }
    }
}

/// The watchdog's limit and action, read when its thread starts at the first frame. A zero limit
/// starts no thread.
#[derive(Resource, Clone)]
pub struct StallSettings {
    pub limit: Duration,
    pub action: StallAction,
}

/// Where a stalled frame is stuck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stuck {
    /// The main part, before the end of the main schedule: a system or a script is not returning.
    Systems,
    /// The main part, past the main schedule: the render world has not come back.
    RenderWorld,
    /// The render part: the render has not finished.
    Render,
}

/// A stall, as reported.
#[derive(Debug, Clone)]
pub struct Stall {
    /// The app's update the stuck part belongs to, from 1: the `FrameCount` its render extracts.
    pub frame: u64,
    /// How long that part has been open.
    pub open: Duration,
    pub limit: Duration,
    pub stuck: Stuck,
    /// The thread that built the app, when it has a name (a test's under `cargo test`).
    pub thread: Option<String>,
}

impl Stall {
    /// The report: one line, starting `Update stopped:`.
    pub fn line(&self) -> String {
        let place = match self.stuck {
            Stuck::Systems => "the frame's systems have not finished: a system or a script is not returning",
            Stuck::RenderWorld => "it is waiting for the render world: the render of the frame before has not finished",
            Stuck::Render => "its render has not finished: the GPU is not taking this engine's work",
        };
        let thread = self.thread.as_ref().map(|t| format!(" (thread {t})")).unwrap_or_default();
        format!(
            "Update stopped: frame {} started {:.1} s ago (limit {} s) and {place}{thread}",
            self.frame,
            self.open.as_secs_f64(),
            self.limit.as_secs_f64(),
        )
    }
}

/// The main schedule's first label: the main part opens.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct StallFirst;

/// The main schedule's last label: past it, the main part waits for the render world.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct StallLast;

/// A startup label between `PreStartup` (Bevy makes the protocol mailbox) and `Startup` (the HTTP
/// server clones its sender).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct StallStartup;

/// The beats, as nanoseconds since `epoch` plus one: 0 is a closed part.
struct Beats {
    epoch: Instant,
    frame: AtomicU64,
    main: AtomicU64,
    past_last: AtomicBool,
    render_frame: AtomicU64,
    render: AtomicU64,
}

impl Beats {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }

    fn stamp(&self) -> u64 {
        self.now().as_nanos() as u64 + 1
    }

    fn snapshot(&self) -> Snapshot {
        let at = |v: u64| (v != 0).then(|| Duration::from_nanos(v - 1));
        Snapshot {
            frame: self.frame.load(Ordering::Acquire),
            main: at(self.main.load(Ordering::Acquire)),
            past_last: self.past_last.load(Ordering::Acquire),
            render_frame: self.render_frame.load(Ordering::Acquire),
            render: at(self.render.load(Ordering::Acquire)),
        }
    }
}

/// The beats at one instant: when each part opened, if it is open.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Snapshot {
    frame: u64,
    main: Option<Duration>,
    past_last: bool,
    render_frame: u64,
    render: Option<Duration>,
}

/// A part open for the limit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Open {
    stuck: Stuck,
    frame: u64,
    since: Duration,
}

impl Open {
    /// Whether this part is still the one open in `s`.
    fn holds(&self, s: &Snapshot) -> bool {
        match self.stuck {
            Stuck::Systems | Stuck::RenderWorld => s.main == Some(self.since),
            Stuck::Render => s.render == Some(self.since),
        }
    }
}

/// The part stalled at `now`, if one has been open for `limit`. When both are, the older one: a
/// render that never returns also keeps the next frame's main part waiting.
fn stalled(s: &Snapshot, now: Duration, limit: Duration) -> Option<Open> {
    let main = s.main.map(|since| Open { stuck: if s.past_last { Stuck::RenderWorld } else { Stuck::Systems }, frame: s.frame, since });
    let render = s.render.map(|since| Open { stuck: Stuck::Render, frame: s.render_frame, since });
    [render, main].into_iter().flatten().filter(|o| now.saturating_sub(o.since) >= limit).min_by_key(|o| o.since)
}

/// The protocol requests the forwarder passed on, kept so a stall can answer them.
#[derive(Default)]
struct Mailbox {
    open: Mutex<Vec<async_channel::Sender<BrpResult>>>,
    /// The stall's line while one lasts: new requests are answered with it.
    stalled: Mutex<Option<String>>,
}

fn stall_error(line: &str) -> BrpError {
    BrpError { code: error_codes::INTERNAL_ERROR, message: line.to_string(), data: None }
}

impl Mailbox {
    /// Passes each request from `from` on to Bevy's mailbox `to`, keeping its result sender first,
    /// so a stall that starts meanwhile answers it.
    fn forward(&self, from: async_channel::Receiver<BrpMessage>, to: async_channel::Sender<BrpMessage>) {
        while let Ok(message) = from.recv_blocking() {
            {
                let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
                open.retain(|s| !s.is_closed());
                open.push(message.sender.clone());
            }
            let mut message = message;
            loop {
                if let Some(line) = self.stalled.lock().unwrap_or_else(PoisonError::into_inner).clone() {
                    let _ = message.sender.try_send(Err(stall_error(&line)));
                    break;
                }
                // Never block on a full mailbox: a stall must still reach this request.
                match to.try_send(message) {
                    Ok(()) => break,
                    Err(async_channel::TrySendError::Full(m)) => {
                        message = m;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(async_channel::TrySendError::Closed(_)) => return,
                }
            }
        }
    }

    /// Answers every open request with `line`, and every new one until [`Mailbox::resume`]. A
    /// request answered already has a full channel and keeps its answer.
    fn stall(&self, line: &str) {
        *self.stalled.lock().unwrap_or_else(PoisonError::into_inner) = Some(line.to_string());
        for s in self.open.lock().unwrap_or_else(PoisonError::into_inner).iter() {
            if !s.is_closed() {
                let _ = s.try_send(Err(stall_error(line)));
            }
        }
    }

    fn resume(&self) {
        *self.stalled.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Waits until every answer was read (its channel closed), at most `within`.
    fn wait_read(&self, within: Duration) {
        let start = Instant::now();
        while start.elapsed() < within && self.open.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|s| !s.is_closed()) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// What the main and render worlds share with the watchdog.
#[derive(Resource, Clone)]
struct Shared {
    beats: Arc<Beats>,
    mailbox: Arc<Mailbox>,
    /// The thread that built the app.
    thread: Option<String>,
    /// Whether the app has a render world: then the extract closes the main part.
    renders: bool,
    /// Whether the app serves the protocol: then its mailbox must be there to take.
    remote: bool,
}

impl Plugin for StallPlugin {
    fn build(&self, app: &mut App) {
        let thread = std::thread::current().name().filter(|n| *n != "main").map(String::from);
        let beats = Beats {
            epoch: Instant::now(),
            frame: AtomicU64::new(0),
            main: AtomicU64::new(0),
            past_last: AtomicBool::new(false),
            render_frame: AtomicU64::new(0),
            render: AtomicU64::new(0),
        };
        app.insert_resource(StallSettings { limit: self.limit, action: self.action.clone() })
            .insert_resource(Shared { beats: Arc::new(beats), mailbox: Arc::default(), thread, renders: false, remote: false });
        app.init_schedule(StallFirst).init_schedule(StallLast).init_schedule(StallStartup);
        let mut order = app.world_mut().resource_mut::<MainScheduleOrder>();
        order.insert_before(First, StallFirst);
        order.insert_startup_after(PreStartup, StallStartup);
        app.add_systems(StallFirst, open_main).add_systems(StallLast, close_main).add_systems(StallStartup, take_mailbox);
    }

    fn finish(&self, app: &mut App) {
        // Last of the main labels once every plugin is built: `RemoteLast` comes after `Last`.
        app.world_mut().resource_mut::<MainScheduleOrder>().labels.push(StallLast.intern());
        let remote = app.is_plugin_added::<RemotePlugin>();
        let renders = app.get_sub_app(RenderApp).is_some();
        let mut shared = app.world_mut().resource_mut::<Shared>();
        shared.remote = remote;
        shared.renders = renders;
        let shared = shared.clone();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(shared)
                .add_systems(ExtractSchedule, open_render)
                .add_systems(Render, close_render.after(RenderSystems::PostCleanup));
        }
    }
}

fn open_main(shared: Res<Shared>, settings: Res<StallSettings>, mut started: Local<bool>) {
    let b = &shared.beats;
    b.frame.fetch_add(1, Ordering::AcqRel);
    b.past_last.store(false, Ordering::Release);
    b.main.store(b.stamp(), Ordering::Release);
    if !*started {
        *started = true;
        if !settings.limit.is_zero() {
            let (beats, mailbox, thread, settings) = (Arc::downgrade(&shared.beats), shared.mailbox.clone(), shared.thread.clone(), settings.clone());
            std::thread::Builder::new().name("bevy-ts stall watchdog".into()).spawn(move || watch(beats, mailbox, thread, settings)).unwrap();
        }
    }
}

fn close_main(shared: Res<Shared>) {
    if shared.renders {
        shared.beats.past_last.store(true, Ordering::Release);
    } else {
        shared.beats.main.store(0, Ordering::Release);
    }
}

/// In the render world's extract, on the main thread once the render world came back.
fn open_render(shared: Res<Shared>) {
    let b = &shared.beats;
    b.render_frame.store(b.frame.load(Ordering::Acquire), Ordering::Release);
    b.render.store(b.stamp(), Ordering::Release);
    b.main.store(0, Ordering::Release);
}

fn close_render(shared: Res<Shared>) {
    shared.beats.render.store(0, Ordering::Release);
}

/// Puts the forwarder between the protocol's transport and Bevy's mailbox: requests reach the
/// app through it, and it answers them when Update stops.
fn take_mailbox(shared: Res<Shared>, sender: Option<ResMut<BrpSender>>) {
    let Some(mut sender) = sender else {
        assert!(!shared.remote, "the stall watchdog found no protocol mailbox after PreStartup: Bevy moved it");
        return;
    };
    let (tx, rx) = async_channel::bounded(64);
    let to = std::mem::replace(&mut **sender, tx);
    let mailbox = shared.mailbox.clone();
    std::thread::Builder::new().name("bevy-ts stall mailbox".into()).spawn(move || mailbox.forward(rx, to)).unwrap();
}

/// The watchdog's thread, until the app drops its beats.
fn watch(beats: Weak<Beats>, mailbox: Arc<Mailbox>, thread: Option<String>, settings: StallSettings) {
    let tick = (settings.limit / 10).max(Duration::from_millis(100));
    let mut current: Option<Open> = None;
    loop {
        std::thread::sleep(tick);
        let Some(b) = beats.upgrade() else { return };
        let (snapshot, now) = (b.snapshot(), b.now());
        drop(b);
        if let Some(open) = current {
            if open.holds(&snapshot) {
                continue;
            }
            current = None;
            mailbox.resume();
            if !matches!(settings.action, StallAction::Exit(_)) {
                eprintln!("Update resumed after {:.1} s", now.saturating_sub(open.since).as_secs_f64());
            }
        }
        let Some(open) = stalled(&snapshot, now, settings.limit) else { continue };
        current = Some(open);
        let stall = Stall { frame: open.frame, open: now.saturating_sub(open.since), limit: settings.limit, stuck: open.stuck, thread: thread.clone() };
        let line = stall.line();
        eprintln!("{line}");
        mailbox.stall(&line);
        match &settings.action {
            StallAction::Exit(code) => {
                mailbox.wait_read(Duration::from_secs(1));
                std::process::exit(*code);
            }
            StallAction::Warn => {}
            StallAction::Call(f) => f(&stall),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: Duration = Duration::from_secs(30);

    fn s(secs: u64) -> Option<Duration> {
        Some(Duration::from_secs(secs))
    }

    #[test]
    fn a_frame_stuck_in_its_systems() {
        let snap = Snapshot { frame: 40, main: s(10), ..Default::default() };
        assert_eq!(stalled(&snap, Duration::from_secs(39), LIMIT), None, "not yet the limit");
        let open = stalled(&snap, Duration::from_secs(40), LIMIT).unwrap();
        assert_eq!((open.stuck, open.frame), (Stuck::Systems, 40));
    }

    #[test]
    fn a_frame_waiting_for_the_render_world() {
        let snap = Snapshot { frame: 9, main: s(10), past_last: true, ..Default::default() };
        assert_eq!(stalled(&snap, Duration::from_secs(41), LIMIT).unwrap().stuck, Stuck::RenderWorld);
    }

    #[test]
    fn a_render_that_never_returns_is_named_before_the_frame_it_holds() {
        // Pipelined: render 812 opened, then frame 813's main part ran to the end and waits.
        let snap = Snapshot { frame: 813, main: s(11), past_last: true, render_frame: 812, render: s(10) };
        let open = stalled(&snap, Duration::from_secs(45), LIMIT).unwrap();
        assert_eq!((open.stuck, open.frame), (Stuck::Render, 812));
    }

    #[test]
    fn idle_time_between_frames_is_no_stall() {
        let snap = Snapshot { frame: 5, main: None, render_frame: 5, render: None, ..Default::default() };
        assert_eq!(stalled(&snap, Duration::from_secs(3600), LIMIT), None);
    }

    #[test]
    fn a_closed_part_ends_the_stall() {
        let open = Open { stuck: Stuck::Render, frame: 3, since: Duration::from_secs(10) };
        assert!(open.holds(&Snapshot { render: s(10), ..Default::default() }));
        assert!(!open.holds(&Snapshot { render: None, ..Default::default() }));
        assert!(!open.holds(&Snapshot { render: s(50), ..Default::default() }), "a later frame's render");
    }

    #[test]
    fn the_line_names_the_frame_and_the_place() {
        let stall = Stall { frame: 812, open: Duration::from_secs(30), limit: LIMIT, stuck: Stuck::Render, thread: Some("it::a_test".into()) };
        let line = stall.line();
        assert!(line.starts_with("Update stopped: frame 812 "), "{line}");
        assert!(line.contains("render has not finished") && line.contains("it::a_test"), "{line}");
    }
}
