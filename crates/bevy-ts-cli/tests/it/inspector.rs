//! A CDP client sets a breakpoint in a TS system the way an editor does (source-mapped line,
//! URL regex), sees the game stop there, resumes it, and sees it hit again after a soft reload.

use crate::common;

use bevy::prelude::*;
use bevy_ts::runtime::RuntimeInfo;
use common::*;
use serde_json::{Value, json};
use std::sync::mpsc;
use std::time::Instant;

const MARK: &str = "t.translation.y = 1.25;";

fn source(extra: &str) -> String {
    format!(
        "import {{ app, Transform, Update }} from \"bevy-ts\";\nimport type {{ Mut, Query }} from \"bevy-ts\";\n\nexport function bob(q: Query<[Mut<Transform>]>) {{\n  q.each((t) => {{\n      {MARK}\n  }});\n}}\n\napp.addSystems(Update, bob);\n{extra}"
    )
}

/// The 0-based line of the marked statement in the compiled JS.
fn generated_line(path: &std::path::Path, src: &str) -> u64 {
    let c = bevy_ts_cli::compile::compile_file(path, src, true).unwrap();
    c.code.lines().position(|l| l.contains("t.translation.y = 1.25")).unwrap() as u64
}

#[test]
fn a_breakpoint_hits_in_ts_and_again_after_a_soft_reload() {
    let src = source("");
    let dir = project("inspector", &[("game.ts", &src)]);
    let entry = dir.join("game.ts").canonicalize().unwrap();
    let line = generated_line(&entry, &src);
    let mut app = app_with_inspector(&entry);
    app.world_mut().spawn(Transform::default());
    let ws = app.world().resource::<RuntimeInfo>().inspector.clone().expect("inspector address");

    let (events, seen) = mpsc::channel::<Value>();
    let (go, wait) = mpsc::channel::<&'static str>();
    let (attached, ready) = mpsc::channel::<()>();
    let client = std::thread::spawn(move || {
        let (mut sock, _) = tungstenite::connect(ws.as_str()).unwrap();
        let mut id = 0;
        let mut send = |sock: &mut tungstenite::WebSocket<_>, method: &str, params: Value| {
            id += 1;
            sock.send(tungstenite::Message::text(json!({ "id": id, "method": method, "params": params }).to_string())).unwrap();
        };
        send(&mut sock, "Runtime.enable", json!({}));
        send(&mut sock, "Debugger.enable", json!({}));
        send(&mut sock, "Debugger.setBreakpointByUrl", json!({ "urlRegex": "game\\.ts(\\?gen=\\d+)?$", "lineNumber": line }));
        let mut hits = 0;
        let mut scripts = std::collections::HashMap::new();
        loop {
            let msg = match sock.read() {
                Ok(tungstenite::Message::Text(t)) => serde_json::from_str::<Value>(&t).unwrap(),
                Ok(_) => continue,
                Err(_) => return hits,
            };
            // The reply to `setBreakpointByUrl`, the third request: the session is ready.
            if msg["id"] == 3 {
                let _ = attached.send(());
            }
            if msg["method"] == "Debugger.scriptParsed" {
                scripts.insert(msg["params"]["scriptId"].as_str().unwrap().to_string(), msg["params"]["url"].clone());
            }
            if msg["method"] == "Debugger.paused" {
                hits += 1;
                let frame = &msg["params"]["callFrames"][0];
                let url = scripts.get(frame["location"]["scriptId"].as_str().unwrap()).cloned().unwrap_or(Value::Null);
                events.send(json!({ "url": url, "line": frame["location"]["lineNumber"] })).unwrap();
                // Wait for the test to look at the frozen game, then resume.
                let _ = wait.recv();
                send(&mut sock, "Debugger.resume", json!({}));
                if hits == 2 {
                    send(&mut sock, "Debugger.disable", json!({}));
                    return hits;
                }
            }
        }
    });

    // The session sets its breakpoint before the generation loads.
    until(&mut app, "the inspector session never set its breakpoint", |_| ready.try_recv().is_ok());
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);

    // The run blocks inside JS while paused; the client resumes it from its thread.
    let resume = std::thread::spawn({
        let go = go.clone();
        move || {
            let first = seen.recv_timeout(BOUND).expect("first breakpoint hit");
            go.send("resume").unwrap();
            let second = seen.recv_timeout(BOUND).expect("breakpoint hit after reload");
            go.send("resume").unwrap();
            (first, second)
        }
    });
    app.update();
    write(&dir, "game.ts", &source("// an unrelated change\n"));
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);
    app.update();
    let (first, second) = resume.join().unwrap();
    assert!(first["url"].as_str().unwrap().ends_with("game.ts?gen=1"), "{first}");
    assert!(second["url"].as_str().unwrap().ends_with("game.ts?gen=2"), "{second}");
    assert_eq!(first["line"], line);
    let deadline = Instant::now() + BOUND;
    while !client.is_finished() && Instant::now() < deadline {
        app.update();
    }
    assert_eq!(client.join().unwrap(), 2, "the session stayed connected across the reload");
}

/// stop-rebuild-split, decision 3: a restart takes the spare instance, and the debugger the
/// backend serves then is the spare's, holding the new generation's globals.
#[test]
fn the_debugger_follows_a_restart_to_the_spare_instance() {
    let src = "globalThis.mark = (globalThis.mark ?? 0) + 1;\n";
    let dir = project("inspector-restart", &[("game.ts", src)]);
    let entry = dir.join("game.ts").canonicalize().unwrap();
    let mut app = app_with_inspector(&entry);
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);
    // The spare boots in a frame of its own.
    app.update();
    assert_eq!(reload(&mut app, true).status, bevy_ts::generation::ReloadStatus::Applied);
    let ws = app.world().non_send::<bevy_ts::runtime::ScriptRuntime>().backend.inspector_address().expect("the debugger is served after the restart");
    let client = std::thread::spawn(move || {
        let (mut sock, _) = tungstenite::connect(ws.as_str()).unwrap();
        sock.send(tungstenite::Message::text(json!({ "id": 1, "method": "Runtime.evaluate", "params": { "expression": "globalThis.mark", "returnByValue": true } }).to_string())).unwrap();
        loop {
            if let Ok(tungstenite::Message::Text(t)) = sock.read() {
                let msg: Value = serde_json::from_str(&t).unwrap();
                if msg["id"] == 1 {
                    return msg["result"]["result"]["value"].clone();
                }
            }
        }
    });
    until(&mut app, "the debugger never answered", |_| client.is_finished());
    assert_eq!(client.join().unwrap(), json!(1), "the instance the restart took, which ran the module once");
}
