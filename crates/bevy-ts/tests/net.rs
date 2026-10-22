//! Change script-networking: the registry, policy, limits, generations and edit mode above the
//! transport seam, against the fake backend and the fake transport. No V8, no socket.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ts::backend::{ModuleSet, ScriptError};
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::log::ScriptLog;
use bevy_ts::net::{self, ConnId, FakeTransport, Frame, NetEvent, NetPolicy, NetRequest, OpenRequest};
use bevy_ts::{BevyTsPlugin, EngineMode};
use std::path::PathBuf;
use std::time::Duration;

fn app() -> (App, FakeHandle, FakeTransport) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
    let allow = vec!["127.0.0.1:4477".to_string(), "${ABYME_URL}".to_string()];
    let env = vec!["ABYME_URL".to_string(), "ABYME_KEY".to_string()];
    let vars = |n: &str| match n {
        "ABYME_URL" => Some("http://127.0.0.1:5000".to_string()),
        "HOME" => Some("/home/me".to_string()),
        _ => None,
    };
    app.insert_resource(NetPolicy::from_config(&allow, &env, &vars).0);
    let fake = net::install_fake(&mut app);
    (app, handle, fake)
}

fn load(app: &mut App, handle: &FakeHandle, program: FakeProgram) -> generation::ReloadReport {
    handle.push(program);
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false)
}

fn open(id: u64, url: &str) -> NetRequest {
    NetRequest::Open { id: ConnId(id), generation: 0, request: OpenRequest { url: url.into(), ..Default::default() } }
}

fn live(app: &App) -> u64 {
    bevy_ts::runtime::current_generation(app.world())
}

fn logs(app: &App) -> Vec<String> {
    app.world().resource::<ScriptLog>().since(0).map(|e| e.message.clone()).collect()
}

/// The events of every delivery so far, in order.
fn delivered(handle: &FakeHandle) -> Vec<NetEvent> {
    handle.deliveries().into_iter().flat_map(|(_, e)| e).collect()
}

/// Opens one connection from a module and accepts it; returns its script id and wire id.
fn connected(app: &mut App, handle: &FakeHandle, fake: &FakeTransport) -> (ConnId, ConnId) {
    handle.request(open(1, "ws://127.0.0.1:4477/api/stream"));
    assert_eq!(load(app, handle, FakeProgram::default()).status, ReloadStatus::Applied);
    let (wire, conn) = fake.last().expect("the open reached the transport");
    assert_eq!(conn.request.url, "ws://127.0.0.1:4477/api/stream");
    fake.accept(wire, "");
    app.update();
    (ConnId(1), wire)
}

#[test]
fn a_module_connects_and_the_handshake_is_delivered() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    assert_eq!(handle.envs()[0].get("ABYME_URL").map(String::as_str), Some("http://127.0.0.1:5000"));
    assert!(!handle.envs()[0].contains_key("ABYME_KEY") && !handle.envs()[0].contains_key("HOME"));
    fake.push_text(wire, "hello");
    app.update();
    let events = delivered(&handle);
    assert_eq!(events[0], NetEvent::Open { id, protocol: String::new(), extensions: String::new() });
    assert_eq!(events[1], NetEvent::Message { id, frame: Frame::Text("hello".into()) });
    // The script's first frame reaches the transport by the next delivery.
    handle.request(NetRequest::Send { id, generation: live(&app), frame: Frame::Text("hi".into()) });
    app.update();
    assert_eq!(fake.conn(wire).unwrap().sent, vec![Frame::Text("hi".into())]);
}

#[test]
fn refused_opens_never_reach_the_transport() {
    let (mut app, handle, fake) = app();
    load(&mut app, &handle, FakeProgram::default());
    let g = live(&app);
    let req = |id, url: &str, headers: &[(&str, &str)]| {
        let headers = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        handle.request(NetRequest::Open { id: ConnId(id), generation: g, request: OpenRequest { url: url.into(), protocols: vec![], headers } });
    };
    req(1, "ws://example.com/", &[]);
    req(2, "wss://127.0.0.1:4477/", &[]);
    req(3, "ws://127.0.0.1:4477/", &[("Origin", "http://evil")]);
    req(4, "ws://127.0.0.1:5000/", &[("authorization", "Bearer k")]);
    app.update(); // the requests reach core
    app.update(); // their events reach the script
    let conns = fake.conns();
    assert_eq!(conns.len(), 1, "only the loopback open with an allowed header goes out: {conns:?}");
    let events = delivered(&handle);
    for id in 1..=3 {
        let id = ConnId(id);
        let at = events.iter().position(|e| *e == NetEvent::Error { id }).unwrap_or_else(|| panic!("no error for {id:?}: {events:?}"));
        assert_eq!(events[at + 1], NetEvent::Close { id, code: 1006, reason: String::new(), clean: false });
    }
    let log = logs(&app).join("\n");
    assert!(log.contains("example.com:80 is not in net.allow; add it to net.allow in bevy-ts.json"), "{log}");
    assert!(log.contains("wss: is not supported yet"), "{log}");
    assert!(log.contains("the engine sends no Origin"), "{log}");
}

#[test]
fn idle_frames_call_nothing() {
    let (mut app, handle, _fake) = app();
    load(&mut app, &handle, FakeProgram::default());
    app.update();
    let before = handle.calls();
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(handle.calls(), before);
    // A due timer calls once.
    handle.set_timer(Some(0.0));
    app.update();
    assert_eq!(handle.calls(), before + 1);
}

#[test]
fn a_burst_is_delivered_in_one_call() {
    let (mut app, handle, fake) = app();
    let (_, wire) = connected(&mut app, &handle, &fake);
    let before = handle.deliveries().len();
    for i in 0..2000 {
        fake.push_text(wire, &format!("m{i}"));
    }
    app.update();
    let deliveries = handle.deliveries();
    assert_eq!(deliveries.len(), before + 1);
    let batch = &deliveries.last().unwrap().1;
    assert_eq!(batch.len(), 2000);
    assert!(matches!(&batch[1999], NetEvent::Message { frame: Frame::Text(t), .. } if t == "m1999"));
    assert!(fake.conn(wire).unwrap().closed.is_none());
}

#[test]
fn a_flood_closes_with_1009_after_what_fits() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    for i in 0..5000 {
        fake.push_text(wire, &format!("m{i}"));
    }
    app.update();
    let batch = handle.deliveries().last().unwrap().1.clone();
    assert_eq!(batch.len(), 4096 + 2);
    assert!(matches!(&batch[4095], NetEvent::Message { frame: Frame::Text(t), .. } if t == "m4095"));
    assert_eq!(batch[4096], NetEvent::Error { id });
    assert!(matches!(batch[4097], NetEvent::Close { code: 1009, clean: false, .. }));
    assert_eq!(fake.conn(wire).unwrap().closed.as_ref().map(|c| c.0), Some(Some(1009)));
    assert!(logs(&app).iter().any(|l| l.contains("1009") && l.contains("4096 messages")), "{:?}", logs(&app));
}

#[test]
fn a_huge_message_closes_with_1009() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    fake.push_text(wire, "before");
    fake.push_binary(wire, &vec![0u8; 20 << 20]);
    app.update();
    let batch = handle.deliveries().last().unwrap().1.clone();
    assert_eq!(batch.len(), 3);
    assert_eq!(batch[1], NetEvent::Error { id });
    assert!(matches!(batch[2], NetEvent::Close { code: 1009, clean: false, .. }));
    assert!(logs(&app).iter().any(|l| l.contains("over the 16 MiB limit")), "{:?}", logs(&app));
}

#[test]
fn bytes_over_64_mib_close_with_1009() {
    let (mut app, handle, fake) = app();
    let (_, wire) = connected(&mut app, &handle, &fake);
    for _ in 0..5 {
        fake.push_binary(wire, &vec![0u8; 15 << 20]);
    }
    app.update();
    let batch = handle.deliveries().last().unwrap().1.clone();
    assert_eq!(batch.len(), 4 + 2, "four 15 MiB messages fit in 64 MiB, the fifth does not");
    assert!(logs(&app).iter().any(|l| l.contains("64 MiB")), "{:?}", logs(&app));
}

#[test]
fn a_server_drop_reports_error_then_1006() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    fake.drop_conn(wire);
    app.update();
    let batch = handle.deliveries().last().unwrap().1.clone();
    assert_eq!(batch, vec![NetEvent::Error { id }, NetEvent::Close { id, code: 1006, reason: String::new(), clean: false }]);
    // A clean server close carries its code, with no error.
    let (mut app, handle, fake) = self::app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    fake.server_close(wire, None, "");
    app.update();
    let batch = handle.deliveries().last().unwrap().1.clone();
    assert_eq!(batch, vec![NetEvent::Close { id, code: 1005, reason: String::new(), clean: true }]);
}

#[test]
fn saving_closes_the_previous_generations_connections() {
    let (mut app, handle, fake) = app();
    let (_, old_wire) = connected(&mut app, &handle, &fake);
    let calls = handle.deliveries().len();
    // Generation 2's module opens its own.
    handle.request(open(2, "ws://127.0.0.1:4477/api/stream"));
    assert_eq!(load(&mut app, &handle, FakeProgram::default()).status, ReloadStatus::Applied);
    assert_eq!(fake.conn(old_wire).unwrap().closed, Some((Some(1001), "going away".into())));
    let (new_wire, conn) = fake.last().unwrap();
    assert_ne!(new_wire, old_wire);
    assert!(conn.closed.is_none());
    // The old connection's close is not delivered.
    app.update();
    assert_eq!(handle.deliveries().len(), calls, "{:?}", handle.deliveries());
}

#[test]
fn a_failed_reload_leaves_the_live_connection_alone() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    handle.request(open(2, "ws://127.0.0.1:4477/api/stream"));
    let program = FakeProgram { load_error: Some(vec![ScriptError::new("boom")]), ..Default::default() };
    assert_eq!(load(&mut app, &handle, program).status, ReloadStatus::Failed);
    assert_eq!(fake.conns().len(), 1, "the failed load's open never went out");
    assert!(fake.conn(wire).unwrap().closed.is_none());
    fake.push_text(wire, "still here");
    app.update();
    assert_eq!(handle.deliveries().last().unwrap().1, vec![NetEvent::Message { id, frame: Frame::Text("still here".into()) }]);
}

#[test]
fn a_restart_closes_everything() {
    let (mut app, handle, fake) = app();
    let (_, wire) = connected(&mut app, &handle, &fake);
    handle.push(FakeProgram::default());
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, true);
    assert_eq!(fake.conn(wire).unwrap().closed, Some((Some(1001), "going away".into())));
    assert!(app.world().non_send::<net::NetRegistry>().is_empty());
}

fn set_mode(app: &mut App, mode: EngineMode) {
    app.world_mut().resource_mut::<NextState<EngineMode>>().set(mode);
    app.update();
}

#[test]
fn edit_mode_closes_connections_and_play_delivers_the_close() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    let before = handle.deliveries().len();
    handle.set_timer(Some(1e12));
    set_mode(&mut app, EngineMode::Edit);
    assert_eq!(fake.conn(wire).unwrap().closed, Some((Some(1001), "going away".into())));
    // Nothing is delivered in edit mode, timers included.
    handle.set_timer(Some(0.0));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(handle.deliveries().len(), before);
    set_mode(&mut app, EngineMode::Play);
    let (_, batch) = handle.deliveries().last().unwrap().clone();
    assert_eq!(batch, vec![NetEvent::Close { id, code: 1001, reason: "going away".into(), clean: true }]);
    assert_eq!(handle.deliveries().len(), before + 1, "the close and the due timer in one call");
}

#[test]
fn a_load_in_edit_mode_opens_nothing() {
    let (mut app, handle, fake) = app();
    app.insert_state(EngineMode::Edit);
    app.update();
    handle.request(open(1, "ws://127.0.0.1:4477/api/stream"));
    load(&mut app, &handle, FakeProgram::default());
    app.update();
    assert!(fake.conns().is_empty());
    assert!(logs(&app).iter().any(|l| l.contains("networking runs only in play")), "{:?}", logs(&app));
    // The refusal's events wait for play.
    assert!(handle.deliveries().is_empty());
    set_mode(&mut app, EngineMode::Play);
    assert!(matches!(delivered(&handle).last(), Some(NetEvent::Close { code: 1006, .. })));
}

#[test]
fn a_paused_game_still_delivers() {
    let (mut app, handle, fake) = app();
    let (id, wire) = connected(&mut app, &handle, &fake);
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
    fake.push_text(wire, "while paused");
    app.update();
    assert_eq!(handle.deliveries().last().unwrap().1, vec![NetEvent::Message { id, frame: Frame::Text("while paused".into()) }]);
    // Real time keeps going for timers.
    let now = handle.deliveries().last().unwrap().0;
    handle.set_timer(Some(now + 50.0));
    for _ in 0..4 {
        app.update();
    }
    assert!(handle.deliveries().last().unwrap().0 >= now + 50.0);
}
