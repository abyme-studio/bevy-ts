//! The stall watchdog (change `update-stall`): a frame that stops is reported as `Update stopped:`
//! with its frame and where it is stuck, and every protocol request still open is answered. Each
//! stall is made on purpose, by a system that blocks until the watchdog's `Call` releases it: no
//! GPU fault, and no test exits the binary.

use crate::common;

use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::remote::BrpReceiver;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy_ts_cli::cli::{Display, base_app};
use bevy_ts_cli::headless;
use bevy_ts_remote::stall::{Stall, StallAction, StallSettings, Stuck};
use common::*;
use serde_json::Value;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(1);

/// Holds the system that reaches frame `at` until it is released.
#[derive(Default)]
struct Gate {
    at: AtomicU32,
    open: Mutex<bool>,
    opened: Condvar,
}

impl Gate {
    fn hold(&self, frame: u32) {
        if frame == 0 || frame != self.at.load(Ordering::SeqCst) {
            return;
        }
        let open = self.open.lock().unwrap();
        // Bounded, so a watchdog that never fires fails the test instead of holding it.
        let _ = self.opened.wait_timeout_while(open, BOUND, |open| !*open).unwrap();
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }

    /// Whether `stall` is the held frame's. On a loaded machine the first frames, which build
    /// pipelines and start the engine, can outlast the test's short limit too.
    fn holds(&self, stall: &Stall) -> bool {
        stall.frame == self.at.load(Ordering::SeqCst) as u64
    }
}

/// The watchdog's action: send the held frame's stall, then let the frame go on.
fn report(gate: &Arc<Gate>) -> (StallAction, std::sync::mpsc::Receiver<Stall>) {
    let (tx, rx) = channel();
    let gate = gate.clone();
    let action = StallAction::Call(Arc::new(move |s: &Stall| {
        if !gate.holds(s) {
            return;
        }
        tx.send(s.clone()).unwrap();
        gate.release();
    }));
    (action, rx)
}

#[test]
fn a_render_that_never_returns_is_reported_in_the_render() {
    let gate = Arc::new(Gate::default());
    let (action, rx) = report(&gate);
    let mut app = render_app();
    app.add_plugins(headless::default_plugins(true)).add_plugins(headless::HeadlessPlugin);
    app.insert_resource(StallSettings { limit: LIMIT, action });
    let g = gate.clone();
    app.sub_app_mut(RenderApp).add_systems(Render, (move |frame: Option<Res<FrameCount>>| g.hold(frame.map_or(0, |f| f.0))).after(RenderSystems::ExtractCommands).before(RenderSystems::Cleanup));
    app.finish();
    app.cleanup();
    updates(&mut app, 3);
    // A frame not extracted yet: the render world counts the frame it extracted.
    let at = app.world().resource::<FrameCount>().0 + 1;
    gate.at.store(at, Ordering::SeqCst);
    let start = Instant::now();
    let mut stall = None;
    until(&mut app, "the stall report", |_| {
        stall = rx.try_recv().ok();
        stall.is_some()
    });
    let stall = stall.unwrap();
    assert_eq!(stall.stuck, Stuck::Render, "{}", stall.line());
    assert_eq!(stall.frame, at as u64, "{}", stall.line());
    assert!(stall.open >= LIMIT && start.elapsed() >= LIMIT, "fired before the limit: {}", stall.line());
    let line = stall.line();
    assert!(line.starts_with(&format!("Update stopped: frame {at} ")), "{line}");
    assert!(line.contains("stall::a_render_that_never_returns_is_reported_in_the_render"), "names the test's thread: {line}");
}

#[test]
fn a_system_that_never_returns_is_reported_in_the_frames_systems() {
    let gate = Arc::new(Gate::default());
    let (action, rx) = report(&gate);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(bevy_ts_remote::stall::StallPlugin { limit: LIMIT, action });
    let g = gate.clone();
    // In Update, `FrameCount` is the frames before this one.
    app.add_systems(Update, move |frame: Res<FrameCount>| g.hold(frame.0 + 1));
    app.finish();
    app.cleanup();
    updates(&mut app, 3);
    gate.at.store(4, Ordering::SeqCst);
    app.update();
    let stall = rx.recv_timeout(BOUND).expect("a stall report");
    assert_eq!((stall.stuck, stall.frame), (Stuck::Systems, 4), "{}", stall.line());
    assert!(stall.open >= LIMIT, "{}", stall.line());
    assert!(stall.line().contains("the frame's systems have not finished"), "{}", stall.line());
}

/// Posts one JSON-RPC request and returns the response's JSON, waiting for the server to come up.
fn post(port: u16, method: &str, params: Value) -> Value {
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let start = Instant::now();
    loop {
        if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            // A request no frame answers fails the test instead of holding it.
            s.set_read_timeout(Some(BOUND)).unwrap();
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            let (from, to) = (out.find('{').expect(&out), out.rfind('}').expect(&out));
            return serde_json::from_str(&out[from..=to]).unwrap();
        }
        assert!(start.elapsed() < BOUND, "the server never came up within {BOUND:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[track_caller]
fn assert_stopped(response: &Value) {
    let error = &response["error"];
    assert_eq!(error["code"], -32603, "{response}");
    assert!(error["message"].as_str().is_some_and(|m| m.starts_with("Update stopped:")), "{response}");
}

#[test]
fn open_requests_are_answered_when_update_stops() {
    let _renderer = renderer();
    let port = free_port();
    let gate = Arc::new(Gate::default());
    // While stalled, a new request: answered by the watchdog's side, as no frame reads it.
    let (tx, rx) = channel();
    let g = gate.clone();
    let action = StallAction::Call(Arc::new(move |s: &Stall| {
        if !g.holds(s) {
            return;
        }
        let pause = post(port, "bevy-ts/pause", serde_json::json!({ "paused": true }));
        tx.send((s.clone(), pause)).unwrap();
        g.release();
    }));
    let engine = crate::ui_widgets::engine();
    let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, Default::default(), &[], false);
    app.add_plugins(bevy_ts_remote::BevyTsRemotePlugin { port });
    app.insert_resource(StallSettings { limit: LIMIT, action });
    let g = gate.clone();
    app.sub_app_mut(RenderApp).add_systems(Render, (move |frame: Option<Res<FrameCount>>| g.hold(frame.map_or(0, |f| f.0))).after(RenderSystems::ExtractCommands).before(RenderSystems::Cleanup));
    app.finish();
    app.cleanup();
    updates(&mut app, 3);

    // The shot reaches the app's mailbox first: were the render already held, a slow transport
    // could bring it after the stall, to frames the test no longer runs.
    let shots = crate::common::TestDir::new("stall-shot");
    let shot = shots.join("stall.png");
    let params = serde_json::json!({ "path": shot.to_str().unwrap() });
    let client = std::thread::spawn(move || post(port, "bevy-ts/screenshot", params));
    let start = Instant::now();
    while app.world().resource::<BrpReceiver>().is_empty() {
        assert!(!client.is_finished() && start.elapsed() < BOUND, "the shot never reached the app");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Frame `at` reads the shot, and its render blocks before it records its pipelines, so the
    // shot waits for that render: it is open when Update stops.
    let at = app.world().resource::<FrameCount>().0 + 1;
    gate.at.store(at, Ordering::SeqCst);
    app.update();

    let screenshot = client.join().unwrap();
    assert_stopped(&screenshot);
    let (stall, pause) = rx.recv_timeout(BOUND).expect("a stall report");
    assert_eq!((stall.stuck, stall.frame), (Stuck::Render, at as u64), "{}", stall.line());
    assert_eq!(screenshot["error"]["message"], stall.line(), "the answer is the report");
    assert_stopped(&pause);
    assert!(!shot.exists(), "a stalled shot writes no file");
}
