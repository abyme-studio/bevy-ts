//! `bevy-ts/screenshot` (change `brp-screenshot`): a shot at a chosen frame headless, paths it
//! refuses, one plain response over HTTP, and `BEVY_TS_SCREENSHOT`'s one shot. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::remote::{BrpMessage, BrpResult, BrpSender, RemotePlugin};
use bevy::render::view::screenshot::Screenshot;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::runtime::{ScriptClock, ScriptFrame};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, SIZE};
use common::*;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BEFORE: Color = Color::srgb(0.1, 0.2, 0.9);
const AFTER: Color = Color::srgb(0.9, 0.3, 0.1);

/// Frames the game has run: bevy-ts's frame counts every update, paused or not.
#[derive(Resource, Default)]
struct Ran(u64);

/// A headless engine as `bevy-ts dev --headless` builds it, with the core methods, a camera, and a
/// clear colour that changes once the game has run 30 frames.
fn engine(name: &str) -> (PathBuf, App) {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", "import { app } from \"bevy-ts\";\n")]);
    let mut app = App::new();
    app.add_plugins(headless::default_plugins(true))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(RemotePlugin::default())
        .insert_resource(ClearColor(BEFORE))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera3d::default());
        })
        .init_resource::<Ran>()
        .add_systems(Update, |clock: Res<ScriptClock>, mut ran: ResMut<Ran>, mut clear: ResMut<ClearColor>| {
            if clock.running_this_frame {
                ran.0 += 1;
            }
            if ran.0 >= 30 {
                clear.0 = AFTER;
            }
        });
    bevy_ts_remote::add_methods(&mut app);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    // `RemotePlugin` makes its mailbox in `PreStartup`.
    app.update();
    (dir, app)
}

/// Sends a request through Bevy's mailbox, as a transport does, and updates until it answers.
fn ask(app: &mut App, method: &str, params: Value) -> BrpResult {
    let (tx, rx) = async_channel::bounded(1);
    let message = BrpMessage { method: method.into(), params: Some(params), sender: tx };
    app.world().resource::<BrpSender>().try_send(message).unwrap();
    for _ in 0..600 {
        app.update();
        if let Ok(r) = rx.try_recv() {
            return r;
        }
    }
    panic!("{method} did not answer within 600 frames");
}

fn frame(app: &App) -> u64 {
    app.world().resource::<ScriptFrame>().0
}

fn ran(app: &App) -> u64 {
    app.world().resource::<Ran>().0
}

/// Pauses, then steps until the game has run `to` frames.
fn step_to(app: &mut App, to: u64) {
    ask(app, "bevy-ts/pause", json!({ "paused": true })).unwrap();
    let now = ran(app);
    assert!(now < to, "already ran {now} frames");
    ask(app, "bevy-ts/step", json!({ "frames": to - now })).unwrap();
    for _ in 0..200 {
        if ran(app) == to && app.world().resource::<ScriptClock>().step_frames == 0 {
            return;
        }
        app.update();
    }
    panic!("never ran {to} frames, ran {}", ran(app));
}

fn png(path: &Path) -> Image {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).unwrap()
}

fn near(a: Color, b: Color) -> bool {
    (a.to_srgba().to_vec4() - b.to_srgba().to_vec4()).truncate().length() < 0.02
}

fn shots_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("{name}-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn a_shot_at_a_chosen_frame_shows_that_frame() {
    let (_, mut app) = engine("screenshot-chosen");
    let dir = shots_dir("screenshot-chosen");
    for (at, colour) in [(29, BEFORE), (30, AFTER)] {
        step_to(&mut app, at);
        let path = dir.join(format!("at-{at}.png"));
        // The shot renders in the next update, the frame the answer names.
        let next = frame(&app) + 1;
        let r = ask(&mut app, "bevy-ts/screenshot", json!({ "path": path })).unwrap();
        assert_eq!(r, json!({ "path": path, "width": SIZE.0, "height": SIZE.1, "frame": next }));
        assert_eq!(ran(&app), at, "still paused after {at} frames");
        let image = png(&path);
        assert_eq!((image.width(), image.height()), SIZE);
        let pixel = image.get_color_at(SIZE.0 / 2, SIZE.1 / 2).unwrap();
        assert!(near(pixel, colour), "frame {at}: {pixel:?}, not {colour:?}");
    }
}

#[test]
fn a_bad_path_is_refused_naming_it_and_nothing_is_captured() {
    let (_, mut app) = engine("screenshot-refused");
    let dir = shots_dir("screenshot-refused");
    let missing = dir.join("no/such/dir");
    let cases = [
        (missing.join("shot.png").display().to_string(), missing.display().to_string()),
        ("relative.png".to_string(), "relative.png".to_string()),
        (dir.join("shot.jpg").display().to_string(), "shot.jpg".to_string()),
    ];
    for (path, named) in cases {
        let e = ask(&mut app, "bevy-ts/screenshot", json!({ "path": path })).unwrap_err();
        assert!(e.message.contains(&named), "{path}: {}", e.message);
        assert_eq!(e.code, bevy::remote::error_codes::INVALID_PARAMS);
    }
    let shots = app.world_mut().query::<&Screenshot>().iter(app.world()).count();
    assert_eq!(shots, 0, "no capture was started");
    assert!(!dir.join("shot.jpg").exists());
}

/// A free TCP port on 127.0.0.1.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// One HTTP POST, the whole reply (headers and body).
fn post(port: u16, body: &Value) -> String {
    let body = body.to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    write!(s, "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

/// An HTTP/1.1 chunked body, joined.
fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or_else(|e| panic!("{e}: chunk size {size:?}"));
        if n == 0 {
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].trim_start_matches("\r\n");
    }
    out
}

/// `bevy-ts dev --headless`, out of process: `BEVY_TS_SCREENSHOT` writes its one file, and the
/// method over Bevy's HTTP transport is one plain JSON-RPC response.
#[test]
fn a_dev_engine_takes_its_env_shot_once_and_answers_the_method_over_http_plainly() {
    let dir = project("screenshot-dev", &[("tsconfig.json", "{}"), ("src/game.ts", "import { app } from \"bevy-ts\";\n")]);
    let shots = shots_dir("screenshot-dev");
    let env_shot = shots.join("env.png");
    let port = free_port();
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut engine = Kill(Command::new(BIN)
        .args(["dev", dir.join("src/game.ts").to_str().unwrap(), "--headless", "--remote-port", &port.to_string()])
        .env("NO_COLOR", "1")
        .env("BEVY_TS_SCREENSHOT", &env_shot)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap());
    let deadline = Instant::now() + Duration::from_secs(120);
    while !env_shot.exists() {
        assert!(Instant::now() < deadline, "no {} within 120 s", env_shot.display());
        assert!(engine.0.try_wait().unwrap().is_none(), "the engine exited");
        std::thread::sleep(Duration::from_millis(100));
    }
    let path = shots.join("http.png");
    let reply = post(port, &json!({ "jsonrpc": "2.0", "id": 7, "method": "bevy-ts/screenshot", "params": { "path": path } }));
    let (head, body) = reply.split_once("\r\n\r\n").unwrap_or_else(|| panic!("no body: {reply}"));
    let head = head.to_ascii_lowercase();
    assert!(head.contains("content-type: application/json"), "not a plain response: {head}");
    assert!(!head.contains("text/event-stream"), "{head}");
    let body = if head.contains("transfer-encoding: chunked") { dechunk(body) } else { body.to_string() };
    let body: Value = serde_json::from_str(body.trim()).unwrap_or_else(|e| panic!("{e}: {body}"));
    assert_eq!(body["id"], 7);
    assert_eq!(body["result"]["path"], json!(path), "{body}");
    assert_eq!((body["result"]["width"].as_u64(), body["result"]["height"].as_u64()), (Some(SIZE.0 as u64), Some(SIZE.1 as u64)));
    assert_eq!((png(&path).width(), png(&env_shot).width()), (SIZE.0, SIZE.0));
    // Frame 75 is long past: the variable took one shot.
    let mut files: Vec<_> = std::fs::read_dir(&shots).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, ["env.png", "http.png"]);
}
