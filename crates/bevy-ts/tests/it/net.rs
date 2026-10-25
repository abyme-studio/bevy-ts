//! Changes script-networking and script-networking-2: the registry, policy, limits, redirects,
//! generations and edit mode above the transport seam, for connections and fetches, against the
//! fake backend and the fake transport. No V8, no socket.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ts::backend::{ModuleSet, ScriptError};
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::log::ScriptLog;
use bevy_ts::net::{self, ConnId, FakeTransport, FetchId, Frame, HttpRequest, HttpResponse, NetEvent, NetPolicy, NetRequest, OpenRequest, Redirect};
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
    assert_eq!(conns.len(), 2, "wss: and the loopback open with an allowed header go out: {conns:?}");
    let events = delivered(&handle);
    for id in [1, 3] {
        let id = ConnId(id);
        let at = events.iter().position(|e| *e == NetEvent::Error { id }).unwrap_or_else(|| panic!("no error for {id:?}: {events:?}"));
        assert_eq!(events[at + 1], NetEvent::Close { id, code: 1006, reason: String::new(), clean: false });
    }
    let log = logs(&app).join("\n");
    assert!(log.contains("example.com:80 is not in net.allow; add it to net.allow in bevy-ts.json"), "{log}");
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

// ---------------------------------------------------------------------------------------------
// fetch

fn get(id: u64, generation: u64, url: &str) -> NetRequest {
    let request = HttpRequest { method: "GET".into(), url: url.into(), ..Default::default() };
    NetRequest::Fetch { id: FetchId(id), generation, request, redirect: Redirect::Follow }
}

/// Starts fetch `request` from a loaded generation; returns the wire id of its first hop, or
/// `None` when nothing reached the transport.
fn fetch(app: &mut App, handle: &FakeHandle, fake: &FakeTransport, request: NetRequest) -> Option<FetchId> {
    let before = fake.fetches().len();
    handle.request(request);
    app.update();
    (fake.fetches().len() > before).then(|| fake.last_fetch().unwrap().0)
}

fn loaded() -> (App, FakeHandle, FakeTransport) {
    let (mut app, handle, fake) = app();
    assert_eq!(load(&mut app, &handle, FakeProgram::default()).status, ReloadStatus::Applied);
    (app, handle, fake)
}

/// The fetch events delivered so far.
fn fetch_events(handle: &FakeHandle) -> Vec<NetEvent> {
    delivered(handle).into_iter().filter(|e| matches!(e, NetEvent::Response { .. } | NetEvent::FetchFailed { .. })).collect()
}

fn failed(handle: &FakeHandle, id: u64) -> String {
    match fetch_events(handle).into_iter().find(|e| matches!(e, NetEvent::FetchFailed { id: i, .. } | NetEvent::Response { id: i, .. } if i.0 == id)) {
        Some(NetEvent::FetchFailed { reason, .. }) => reason,
        other => panic!("fetch {id} did not fail: {other:?}"),
    }
}

fn response(handle: &FakeHandle, id: u64) -> HttpResponse {
    match fetch_events(handle).into_iter().find(|e| matches!(e, NetEvent::FetchFailed { id: i, .. } | NetEvent::Response { id: i, .. } if i.0 == id)) {
        Some(NetEvent::Response { response, .. }) => response,
        other => panic!("fetch {id} has no response: {other:?}"),
    }
}

#[test]
fn a_fetch_not_allowed() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    assert_eq!(fetch(&mut app, &handle, &fake, get(1, g, "https://example.com/")), None, "no connection is attempted");
    app.update();
    let reason = failed(&handle, 1);
    assert!(reason.contains("example.com:443 is not in net.allow"), "{reason}");
    assert!(logs(&app).iter().any(|l| l.starts_with("fetch https://example.com/: example.com:443 is not in net.allow")), "{:?}", logs(&app));
}

#[test]
fn a_headless_test_of_a_fetch() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/api/held")).unwrap();
    let seen = fake.fetch(wire).unwrap().request;
    assert_eq!((seen.method.as_str(), seen.url.as_str()), ("GET", "http://127.0.0.1:4477/api/held"));
    fake.respond(wire, 200, &[("Content-Type", "application/json")], br#"{"held":[]}"#);
    app.update();
    let r = response(&handle, 1);
    assert_eq!((r.status, r.body.as_slice(), r.url.as_str(), r.redirected), (200, &br#"{"held":[]}"#[..], "http://127.0.0.1:4477/api/held", false));
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert!(app.world().non_send::<net::NetRegistry>().is_empty(), "an answered fetch is forgotten");
}

#[test]
fn a_redirect_to_a_host_not_allowed() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/a")).unwrap();
    fake.respond(wire, 302, &[("location", "http://10.0.0.9/b")], b"");
    app.update();
    assert_eq!(fake.fetches().len(), 1, "nothing connects to 10.0.0.9");
    app.update();
    assert!(failed(&handle, 1).contains("10.0.0.9:80 is not in net.allow"));
    assert!(logs(&app).iter().any(|l| l.starts_with("fetch http://10.0.0.9/b: 10.0.0.9:80 is not in net.allow")), "{:?}", logs(&app));
}

#[test]
fn a_redirect_that_keeps_its_key() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let headers = vec![("authorization".to_string(), "Bearer k".to_string()), ("cookie".to_string(), "a=1".to_string()), ("x-trace".to_string(), "t".to_string())];
    let request = HttpRequest { method: "POST".into(), url: "http://127.0.0.1:4477/a".into(), headers, body: b"{}".to_vec() };
    let first = fetch(&mut app, &handle, &fake, NetRequest::Fetch { id: FetchId(1), generation: g, request, redirect: Redirect::Follow }).unwrap();
    fake.respond(first, 307, &[("location", "/b")], b"");
    app.update();
    let (second, hop) = fake.last_fetch().unwrap();
    assert_eq!((hop.request.url.as_str(), hop.request.method.as_str(), hop.request.body.as_slice()), ("http://127.0.0.1:4477/b", "POST", &b"{}"[..]), "307 resends the method and body");
    assert_eq!(hop.request.header("authorization"), Some("Bearer k"), "the same host keeps its key");
    fake.respond(second, 307, &[("location", "http://127.0.0.1:5000/c")], b"");
    app.update();
    let (third, hop) = fake.last_fetch().unwrap();
    assert_eq!(hop.request.url, "http://127.0.0.1:5000/c");
    assert_eq!((hop.request.header("authorization"), hop.request.header("cookie")), (None, None), "another port drops the key and cookie");
    assert_eq!(hop.request.header("x-trace"), Some("t"));
    fake.respond(third, 200, &[], b"done");
    app.update();
    let r = response(&handle, 1);
    assert_eq!((r.url.as_str(), r.redirected, r.body.as_slice()), ("http://127.0.0.1:5000/c", true, &b"done"[..]));
    assert_eq!(fetch_events(&handle).len(), 1, "no event reaches the script before the last hop");
}

#[test]
fn leaving_tls() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "https://127.0.0.1:4477/")).unwrap();
    fake.respond(wire, 301, &[("location", "http://127.0.0.1:4477/")], b"");
    app.update();
    app.update();
    assert_eq!(fake.fetches().len(), 1);
    assert!(failed(&handle, 1).contains("a redirect may not leave TLS"));
    assert!(logs(&app).iter().any(|l| l.contains("may not leave TLS")), "{:?}", logs(&app));
}

#[test]
fn redirects_rewrite_the_method_as_the_web_does() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    for (n, status, method, after) in [(1, 303, "POST", "GET"), (2, 301, "POST", "GET"), (3, 302, "POST", "GET"), (4, 302, "PUT", "PUT"), (5, 303, "HEAD", "HEAD"), (6, 308, "POST", "POST")] {
        let headers = vec![("content-type".to_string(), "text/plain".to_string())];
        let request = HttpRequest { method: method.into(), url: "http://127.0.0.1:4477/a".into(), headers, body: b"x".to_vec() };
        let wire = fetch(&mut app, &handle, &fake, NetRequest::Fetch { id: FetchId(n), generation: g, request, redirect: Redirect::Follow }).unwrap();
        fake.respond(wire, status, &[("location", "/b")], b"");
        app.update();
        let hop = fake.last_fetch().unwrap().1.request;
        assert_eq!(hop.method, after, "{status} {method}");
        assert_eq!(hop.body.is_empty(), after == "GET", "{status} {method}");
        assert_eq!(hop.header("content-type").is_none(), after == "GET", "{status} {method}: a GET drops the body's headers");
    }
}

#[test]
fn twenty_redirects_and_no_more() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let mut wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/0")).unwrap();
    for hop in 1..=20 {
        fake.respond(wire, 302, &[("location", &format!("/{hop}"))], b"");
        app.update();
        wire = fake.last_fetch().unwrap().0;
    }
    assert_eq!(fake.fetches().len(), 21);
    fake.respond(wire, 302, &[("location", "/21")], b"");
    app.update();
    app.update();
    assert_eq!(fake.fetches().len(), 21, "the twenty-first redirect is not followed");
    assert!(failed(&handle, 1).contains("more than 20 redirects"));
}

#[test]
fn redirect_error_and_manual() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let mode = |id, redirect| NetRequest::Fetch { id: FetchId(id), generation: g, request: HttpRequest { method: "GET".into(), url: "http://127.0.0.1:4477/a".into(), ..Default::default() }, redirect };
    let error = fetch(&mut app, &handle, &fake, mode(1, Redirect::Error)).unwrap();
    let manual = fetch(&mut app, &handle, &fake, mode(2, Redirect::Manual)).unwrap();
    fake.respond(error, 302, &[("location", "/b")], b"");
    fake.respond(manual, 302, &[("location", "/b")], b"");
    app.update();
    app.update();
    assert_eq!(fake.fetches().len(), 2, "neither follows");
    assert!(failed(&handle, 1).contains("redirect is \"error\""));
    let r = response(&handle, 2);
    assert_eq!((r.status, r.header("location"), r.redirected), (302, Some("/b"), false));
}

#[test]
fn a_huge_download() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/big")).unwrap();
    fake.respond(wire, 200, &[], &vec![0u8; 20 << 20]);
    app.update();
    assert!(failed(&handle, 1).contains("over the 16 MiB limit"));
    assert!(logs(&app).iter().any(|l| l.contains("over the 16 MiB limit")), "{:?}", logs(&app));
}

#[test]
fn saving_during_a_fetch() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/slow")).unwrap();
    assert_eq!(load(&mut app, &handle, FakeProgram::default()).status, ReloadStatus::Applied);
    assert!(fake.cancelled(wire), "the request is cancelled");
    fake.respond(wire, 200, &[], b"late");
    for _ in 0..3 {
        app.update();
    }
    assert!(fetch_events(&handle).is_empty(), "the old generation's promise never settles: {:?}", fetch_events(&handle));
}

#[test]
fn a_restart_and_an_abort_cancel_fetches() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let aborted = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/a")).unwrap();
    handle.request(NetRequest::Abort { id: FetchId(1), generation: g });
    app.update();
    assert!(fake.cancelled(aborted));
    let held = fetch(&mut app, &handle, &fake, get(2, g, "http://127.0.0.1:4477/b")).unwrap();
    handle.push(FakeProgram::default());
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, true);
    assert!(fake.cancelled(held));
    assert!(app.world().non_send::<net::NetRegistry>().is_empty());
    app.update();
    assert!(fetch_events(&handle).is_empty());
}

#[test]
fn a_fetch_across_edit() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/slow")).unwrap();
    set_mode(&mut app, EngineMode::Edit);
    assert!(fake.cancelled(wire));
    app.update();
    assert!(fetch_events(&handle).is_empty(), "nothing is delivered in edit mode");
    set_mode(&mut app, EngineMode::Play);
    assert_eq!(failed(&handle, 1), net::LEFT_PLAY, "the promise rejects at the first frame of play");
}

#[test]
fn a_fetch_in_edit_mode_is_refused() {
    let (mut app, handle, fake) = app();
    app.insert_state(EngineMode::Edit);
    app.update();
    handle.request(get(1, 0, "http://127.0.0.1:4477/api/held"));
    load(&mut app, &handle, FakeProgram::default());
    app.update();
    assert!(fake.fetches().is_empty());
    assert!(logs(&app).iter().any(|l| l.starts_with("fetch http://127.0.0.1:4477/api/held: networking runs only in play")), "{:?}", logs(&app));
    set_mode(&mut app, EngineMode::Play);
    assert!(failed(&handle, 1).contains("networking runs only in play"));
}

#[test]
fn a_transport_failure_names_the_origin() {
    let (mut app, handle, fake) = loaded();
    let g = live(&app);
    let wire = fetch(&mut app, &handle, &fake, get(1, g, "http://127.0.0.1:4477/x")).unwrap();
    fake.fail(wire, "the connection failed: refused");
    app.update();
    assert_eq!(failed(&handle, 1), "fetch 127.0.0.1:4477: the connection failed: refused");
    assert!(logs(&app).iter().any(|l| l == "fetch http://127.0.0.1:4477/x: the connection failed: refused"), "{:?}", logs(&app));
}
