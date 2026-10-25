//! Helpers shared by the embedding's tests: a built game folder, the one-engine lock, and the
//! host's side of the C ABI.

use bevy_ts_embed::play::{GameFile, NetConfig, VERSION};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long a step may take on a machine at load 30.
pub const BOUND: Duration = Duration::from_secs(60);

/// One engine per process: tests that create one take this first.
pub fn one_engine() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A fresh directory under the temp dir, named for the test and the process.
pub fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-embed-tests").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, src) in files {
        let path = dir.join(f);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

/// A built game folder as `bevy-ts build` writes one: `game.json`, `scripts/game.js` and an
/// empty `assets/`.
pub fn game(name: &str, script: &str, file: impl FnOnce(&mut GameFile)) -> PathBuf {
    let dir = project(name, &[("scripts/game.js", script)]);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    let mut game = GameFile {
        name: name.into(),
        entry: "game.js".into(),
        bevy_ts: VERSION.into(),
        engine: "prebuilt".into(),
        mesh_picking: false,
        net: NetConfig::default(),
        env: vec![],
    };
    file(&mut game);
    std::fs::write(dir.join("game.json"), serde_json::to_string(&game).unwrap()).unwrap();
    dir
}

/// The config `bevy_ts_create` takes, for `game` headless, with `extra` keys.
pub fn config(game: &Path, extra: serde_json::Value) -> String {
    let mut c = serde_json::json!({ "game": game, "headless": true });
    if let serde_json::Value::Object(m) = extra {
        for (k, v) in m {
            c[k] = v;
        }
    }
    c.to_string()
}

/// The host's end of the event callback.
pub struct Host {
    tx: Mutex<Sender<String>>,
}

unsafe extern "C" fn on_event(user: *mut c_void, json: *const c_char, len: usize) {
    let host = unsafe { &*(user as *const Host) };
    let text = unsafe { CStr::from_ptr(json) }.to_str().unwrap().to_string();
    assert_eq!(text.len(), len);
    host.tx.lock().unwrap().send(text).unwrap();
}

/// An engine made through the C ABI, and its events.
pub struct CEngine {
    pub ptr: *mut bevy_ts_embed::ffi::BevyTsEngine,
    pub events: Receiver<String>,
    _host: Box<Host>,
}

/// `bevy_ts_create` headless, with no window: the engine or its error.
pub fn create(config: &str) -> Result<CEngine, String> {
    let (tx, events) = channel();
    let host = Box::new(Host { tx: Mutex::new(tx) });
    let config = CString::new(config).unwrap();
    let mut error: *mut c_char = std::ptr::null_mut();
    let user = &*host as *const Host as *mut c_void;
    let ptr = unsafe { bevy_ts_embed::ffi::bevy_ts_create(config.as_ptr(), std::ptr::null_mut(), 0, 0, 1.0, on_event, user, &mut error) };
    if ptr.is_null() {
        assert!(!error.is_null(), "a failed create sets its error");
        let message = unsafe { CStr::from_ptr(error) }.to_str().unwrap().to_string();
        unsafe { bevy_ts_embed::ffi::bevy_ts_free_string(error) };
        return Err(message);
    }
    assert!(error.is_null());
    Ok(CEngine { ptr, events, _host: host })
}

impl CEngine {
    pub fn input(&self, json: &str) {
        unsafe { bevy_ts_embed::ffi::bevy_ts_input(self.ptr, json.as_ptr().cast(), json.len()) };
    }

    /// Destroys it and answers the events sent before `bevy_ts_destroy` returned.
    pub fn destroy(self) -> Vec<String> {
        unsafe { bevy_ts_embed::ffi::bevy_ts_destroy(self.ptr) };
        self.events.try_iter().collect()
    }
}

/// Waits for an event containing every needle, answering every event read on the way.
pub fn wait_event(events: &Receiver<String>, needles: &[&str]) -> Vec<String> {
    let deadline = Instant::now() + BOUND;
    let mut seen = vec![];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(e) => {
                let hit = needles.iter().all(|n| e.contains(n));
                seen.push(e);
                if hit {
                    return seen;
                }
            }
            Err(_) => panic!("no event with {needles:?} within {BOUND:?}; saw {seen:#?}"),
        }
    }
}

/// Waits until `done` holds, with the deadline.
pub fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + BOUND;
    while !done() {
        assert!(Instant::now() < deadline, "{what}: not within {BOUND:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
