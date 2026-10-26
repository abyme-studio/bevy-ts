//! The embedded engine: a thread of its own running the shipped game's app with a custom runner.
//! The host's calls become commands the runner drains at each frame boundary; it loops while
//! resumed with a window (or headless) and parks on the channel otherwise.

use crate::input::{HostWindow, PendingInput};
use crate::play::{Game, play_app};
use crate::window::NativeWindow;
use bevy::app::{AppExit, PluginsState};
use bevy::asset::io::AssetSourceBuilder;
use bevy::asset::io::file::FileAssetReader;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::view::{ExtractedWindow, SurfaceData};
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::window::{ExitCondition, PrimaryWindow, RawHandleWrapper, WindowCreated, WindowEvent, WindowResized, WindowResolution, WindowScaleFactorChanged};
use bevy_ts::engine::EngineIdentity;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_v8::V8Options;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long `surface` with no window waits for the engine to let go of the old one.
pub const SURFACE_DEADLINE: Duration = Duration::from_secs(2);
/// How long `destroy` waits for the engine thread.
pub const DESTROY_DEADLINE: Duration = Duration::from_secs(5);
/// A headless engine's frame rate.
const HEADLESS_HZ: f64 = 60.0;
/// The window size a headless engine made with no size gets.
const DEFAULT_SIZE: (u32, u32) = (960, 540);

/// `bevy_ts_create`'s `config_json`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {
    /// The folder holding `game.json`.
    pub game: PathBuf,
    /// What `process.env` and `${NAME}` in `net.allow` read, in place of the process's.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The one origin besides loopback a script may send headers to, over `wss:` only.
    #[serde(default)]
    pub headers_origin: Option<String>,
    #[serde(default)]
    pub headless: bool,
}

/// Where the engine's events go: one JSON object per call, from the engine's thread.
pub type Events = Arc<dyn Fn(&str) + Send + Sync>;

/// A Rust-only hook a test gets the app through before its first frame (a fake transport, a
/// frame counter). Never reachable from the C ABI.
pub type Setup = Box<dyn FnOnce(&mut App) + Send>;

/// A surface the host gives: a window (or none) and its size.
pub struct Surface {
    pub window: Option<NativeWindow>,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

enum Command {
    Surface(Surface, Option<SyncSender<()>>),
    Pause,
    Resume,
    Input(String),
    Stop,
}

/// One engine per process: V8's platform and Bevy's task pools are process-wide.
static LIVE: AtomicBool = AtomicBool::new(false);

/// A running engine. Dropping it destroys it.
pub struct Engine {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
    done: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

/// The identity an embedded engine plays as: the prebuilt engine of this version.
pub fn identity() -> EngineIdentity {
    EngineIdentity::prebuilt(crate::play::VERSION, TARGET)
}

/// The target this library was built for.
pub const TARGET: &str = env!("BEVY_TS_EMBED_TARGET");

impl Engine {
    /// Checks the game on this thread, then starts the engine's thread.
    pub fn create(config_json: &str, surface: Surface, events: Events, setup: Option<Setup>) -> Result<Engine, String> {
        let config: Config = serde_json::from_str(config_json).map_err(|e| format!("config: {e}"))?;
        if !config.game.is_absolute() {
            return Err(format!("config: game {} is not an absolute path", config.game.display()));
        }
        if let Some(origin) = &config.headers_origin {
            bevy_ts::net::HeadersOrigin::parse(origin).map_err(|e| format!("config: headersOrigin: {e}"))?;
        }
        if LIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err("an engine is already running".into());
        }
        let game = match Game::load(&config.game, &identity()) {
            Ok(g) => g,
            Err(e) => {
                LIVE.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        let (tx, rx) = channel();
        let done = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let finished = done.clone();
        let thread = std::thread::Builder::new()
            .name("bevy-ts engine".into())
            .spawn(move || {
                let exited = events.clone();
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || run(game, config, surface, rx, events, setup)));
                let message = match run {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(e),
                    Err(panic) => Some(panic_message(&panic)),
                };
                let code = if message.is_some() { 1 } else { 0 };
                let mut event = json!({ "event": "exited", "code": code });
                if let Some(m) = message {
                    event["message"] = m.into();
                }
                exited(&event.to_string());
                LIVE.store(false, Ordering::SeqCst);
                let (lock, cv) = &*finished;
                *lock.lock().unwrap() = true;
                cv.notify_all();
            })
            .map_err(|e| {
                LIVE.store(false, Ordering::SeqCst);
                format!("cannot start the engine thread: {e}")
            })?;
        Ok(Engine { tx, thread: Some(thread), done })
    }

    /// A new window, the same at a new size, or none. With none, returns once the engine has let
    /// go of the old window, or after [`SURFACE_DEADLINE`].
    pub fn surface(&self, surface: Surface) {
        if surface.window.is_some() {
            let _ = self.tx.send(Command::Surface(surface, None));
            return;
        }
        let (ack, wait) = sync_channel(1);
        if self.tx.send(Command::Surface(surface, Some(ack))).is_ok() && wait.recv_timeout(SURFACE_DEADLINE).is_err() {
            warn!("embed: the engine did not let go of its window within {SURFACE_DEADLINE:?}");
        }
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Command::Resume);
    }

    /// One SURFACE.md `ViewportInput` event, applied at the next frame.
    pub fn input(&self, json: String) {
        let _ = self.tx.send(Command::Input(json));
    }

    /// Stops the engine and waits for its thread, up to [`DESTROY_DEADLINE`].
    pub fn destroy(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        let _ = self.tx.send(Command::Stop);
        let (lock, cv) = &*self.done;
        let (done, _) = cv.wait_timeout_while(lock.lock().unwrap(), DESTROY_DEADLINE, |done| !*done).unwrap();
        if *done {
            let _ = thread.join();
        } else {
            warn!("embed: the engine thread did not stop within {DESTROY_DEADLINE:?}");
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    let what = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned());
    format!("the engine panicked: {}", what.unwrap_or_else(|| "(no message)".into()))
}

/// The host's events, as a resource systems send through.
#[derive(Resource, Clone)]
struct HostEvents(Events);

/// Bevy's plugins for an embedding: no winit (the host owns the window), no pipelined rendering
/// (a removed window's surface must drop inside the frame that removes it), assets read by path
/// from the game's folder, never a platform asset store, and unwatched.
fn base_app(assets: &std::path::Path) -> App {
    let mut app = App::new();
    let root = assets.to_path_buf();
    app.register_asset_source(
        bevy::asset::io::AssetSourceId::Default,
        AssetSourceBuilder::new(move || Box::new(FileAssetReader::new(root.clone()))),
    );
    let plugins = DefaultPlugins
        .build()
        .disable::<PipelinedRenderingPlugin>()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() });
    #[cfg(not(target_os = "android"))]
    let plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    #[cfg(target_os = "android")]
    let plugins = match crate::gpu::probe().and_then(|(name, limits)| crate::gpu::constrained_limits(&name, &limits)) {
        Some(limits) => {
            let settings = bevy::render::settings::WgpuSettings { constrained_limits: Some(limits), ..default() };
            plugins.set(bevy::render::RenderPlugin { render_creation: settings.into(), ..default() })
        }
        None => plugins,
    };
    app.add_plugins(plugins);
    app
}

fn run(game: Game, config: Config, surface: Surface, rx: Receiver<Command>, events: Events, setup: Option<Setup>) -> Result<(), String> {
    let env = config.env.clone();
    let lookup = move |name: &str| env.get(name).cloned();
    let mut app = play_app(game, &lookup, |assets, mesh_picking| {
        let mut app = base_app(assets);
        crate::shipped::add_shipped(&mut app, identity(), V8Options::default(), false);
        crate::shipped::finish_shipped(&mut app, mesh_picking, Some(assets));
        app
    });
    if let Some(origin) = &config.headers_origin {
        let origin = bevy_ts::net::HeadersOrigin::parse(origin)?;
        app.world_mut().resource_mut::<bevy_ts::net::NetPolicy>().headers_origin = Some(origin);
    }
    let (w, h) = if surface.width == 0 || surface.height == 0 { DEFAULT_SIZE } else { (surface.width, surface.height) };
    let mut resolution = WindowResolution::new(w, h);
    resolution.set_scale_factor_override(Some(surface.scale.max(0.1)));
    // Always there, so input and cameras have a window; it carries a `RawHandleWrapper` only while
    // the host gives a native one.
    let window = app.world_mut().spawn((Window { title: "bevy-ts".into(), resolution, ..default() }, PrimaryWindow, HostWindow)).id();
    app.insert_resource(HostEvents(events.clone()))
        .init_resource::<PendingInput>()
        .init_resource::<crate::input::Remote>()
        .init_resource::<Sent>()
        .add_systems(First, crate::input::apply)
        .add_systems(Last, send_events);
    if let Some(setup) = setup {
        setup(&mut app);
    }
    while app.plugins_state() == PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    app.world_mut().write_message(WindowCreated { window });
    let mut runner = Runner { app, window, native: None, released: None, resumed: true, headless: config.headless, started: false };
    if surface.window.is_some() {
        runner.set_surface(surface);
    }
    runner.run(rx)
}

struct Runner {
    app: App,
    window: Entity,
    /// The host's window pointer while one is given.
    native: Option<*mut std::ffi::c_void>,
    /// Set once the engine let go of the last window.
    released: Option<Arc<AtomicBool>>,
    resumed: bool,
    headless: bool,
    started: bool,
}

impl Runner {
    fn running(&self) -> bool {
        self.resumed && (self.native.is_some() || self.headless)
    }

    fn run(&mut self, rx: Receiver<Command>) -> Result<(), String> {
        let period = Duration::from_secs_f64(1.0 / HEADLESS_HZ);
        let mut next = Instant::now();
        loop {
            // Parked: wait for a command. Running: take what came, then a frame.
            let first = if self.running() { rx.try_recv().ok() } else { Some(rx.recv().unwrap_or(Command::Stop)) };
            for command in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                match command {
                    Command::Surface(surface, ack) => {
                        self.set_surface(surface);
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                    }
                    Command::Pause => self.resumed = false,
                    Command::Resume => self.resumed = true,
                    Command::Input(json) => {
                        let queued = serde_json::from_str(&json).is_ok_and(|value| crate::input::queue_at(self.app.world_mut(), Instant::now(), value));
                        if !queued {
                            // The host hears of it as a `log` event.
                            let mut log = self.app.world_mut().resource_mut::<ScriptLog>();
                            log.push(LogLevel::Warn, format!("input: malformed input event {json}"), None, 0);
                        }
                    }
                    Command::Stop => return self.shut_down(),
                }
            }
            if !self.running() {
                continue;
            }
            self.app.update();
            if !self.started {
                self.started = true;
                let inputs = crate::input::INPUTS;
                self.send(json!({ "event": "started", "inputs": inputs, "js": "v8" }));
            }
            if let Some(exit) = self.app.should_exit() {
                self.shut_down()?;
                return match exit {
                    AppExit::Success => Ok(()),
                    AppExit::Error(code) => Err(format!("the game exited with {code}")),
                };
            }
            // A window paces frames by its present mode; headless keeps 60 Hz.
            if self.native.is_none() {
                next += period;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now;
                }
            }
        }
    }

    fn send(&self, event: serde_json::Value) {
        (self.app.world().resource::<HostEvents>().0)(&event.to_string());
    }

    /// Closes every connection with 1001, drops the world and the JS engine and the window.
    fn shut_down(&mut self) -> Result<(), String> {
        if let Some(mut net) = self.app.world_mut().get_non_send_mut::<bevy_ts::net::NetRegistry>() {
            net.close_all();
        }
        self.drop_window();
        let app = std::mem::replace(&mut self.app, App::empty());
        drop(app);
        Ok(())
    }

    /// Takes the host's surface: a window replaces any before it (the old one dropped first, so
    /// the renderer makes a new surface), the same window resizes, none stops drawing.
    fn set_surface(&mut self, surface: Surface) {
        let Surface { window, width, height, scale } = surface;
        match window {
            None => self.drop_window(),
            Some(native) => {
                if self.native != Some(native.ptr()) {
                    self.drop_window();
                    self.native = Some(native.ptr());
                    self.released = Some(native.released());
                    match native.into_handle() {
                        Ok(handle) => {
                            self.app.world_mut().entity_mut(self.window).insert(handle);
                        }
                        Err(e) => warn!("embed: the host's window has no handle: {e}"),
                    }
                }
                // Dropped here when it was the same window: the engine keeps the first reference.
                self.resize(width, height, scale);
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) {
        if width == 0 || height == 0 {
            return;
        }
        let world = self.app.world_mut();
        let mut w = world.get_mut::<Window>(self.window).expect("the host window");
        let size_changed = w.resolution.physical_width() != width || w.resolution.physical_height() != height;
        let scale_changed = w.resolution.scale_factor_override() != Some(scale);
        w.resolution.set_physical_resolution(width, height);
        w.resolution.set_scale_factor_override(Some(scale));
        let (lw, lh) = (w.resolution.width(), w.resolution.height());
        let window = self.window;
        if scale_changed {
            let m = WindowScaleFactorChanged { window, scale_factor: scale as f64 };
            world.write_message(m.clone());
            world.write_message(WindowEvent::WindowScaleFactorChanged(m));
        }
        if size_changed || scale_changed {
            let m = WindowResized { window, width: lw, height: lh };
            world.write_message(m.clone());
            world.write_message(WindowEvent::WindowResized(m));
        }
    }

    /// Removes the window's handle from both worlds and drops the render world's surface, as
    /// Bevy's `extract_windows` does, then waits until the window is released or the deadline
    /// passes. No schedule runs: a render update with no main-world frame before it panics in
    /// Bevy's shadow queueing, and a main-world frame would run while paused.
    fn drop_window(&mut self) {
        if self.native.take().is_none() {
            return;
        }
        self.app.world_mut().entity_mut(self.window).remove::<RawHandleWrapper>();
        if let Some(render) = self.app.get_sub_app_mut(RenderApp) {
            let world = render.world_mut();
            let windows: Vec<Entity> = world.query_filtered::<Entity, With<RawHandleWrapper>>().iter(world).collect();
            for window in windows {
                world.entity_mut(window).remove::<(ExtractedWindow, RawHandleWrapper, SurfaceData)>();
            }
        }
        let released = self.released.take();
        let deadline = Instant::now() + SURFACE_DEADLINE;
        while !released.as_ref().is_none_or(|r| r.load(Ordering::SeqCst)) {
            if Instant::now() >= deadline {
                warn!("embed: the renderer still holds the host's window after {SURFACE_DEADLINE:?}");
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// What has been sent to the host: the next log entry, and whether text input was on.
#[derive(Resource, Default)]
struct Sent {
    log: u64,
    ime: bool,
}

/// Sends each new script log line as a `log` event and a change of the window's text input as
/// `ime`.
fn send_events(log: Option<Res<ScriptLog>>, windows: Query<&Window, With<HostWindow>>, events: Res<HostEvents>, mut sent: ResMut<Sent>) {
    if let Some(log) = log {
        for entry in log.since(sent.log) {
            let level = match entry.level {
                LogLevel::Debug => "debug",
                LogLevel::Info => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            };
            let mut event = json!({ "event": "log", "level": level, "message": entry.message });
            if let Some(at) = &entry.location {
                event["file"] = at.file.clone().into();
                event["line"] = at.line.into();
            }
            (events.0)(&event.to_string());
        }
        sent.log = log.next_seq();
    }
    if let Ok(w) = windows.single()
        && w.ime_enabled != sent.ime
    {
        sent.ime = w.ime_enabled;
        let kind = if w.ime_enabled { "enabled" } else { "disabled" };
        (events.0)(&json!({ "event": "ime", "kind": kind }).to_string());
    }
}
