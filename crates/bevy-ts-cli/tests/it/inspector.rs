//! A CDP client sets a breakpoint in a TS system the way an editor does (source-mapped line,
//! URL regex), sees the game stop there, resumes it, and sees it hit again after a soft reload.

use crate::common;

use bevy::prelude::*;
use bevy_ts::runtime::RuntimeInfo;
use common::*;
use serde_json::{Value, json};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

    // Let the session attach before the generation loads.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(300) {
        app.update();
    }
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);

    // The run blocks inside JS while paused; the client resumes it from its thread.
    let resume = std::thread::spawn({
        let go = go.clone();
        move || {
            let first = seen.recv_timeout(Duration::from_secs(10)).expect("first breakpoint hit");
            go.send("resume").unwrap();
            let second = seen.recv_timeout(Duration::from_secs(10)).expect("breakpoint hit after reload");
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while !client.is_finished() && Instant::now() < deadline {
        app.update();
    }
    assert_eq!(client.join().unwrap(), 2, "the session stayed connected across the reload");
}
