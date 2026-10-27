//! A script's ready hold (change `ready-hold`): `ReadyHold` from `bevy-ts/ready`, bevy-ts's own
//! (Bevy has none). Held from the script's `Startup`, no frame is ready and the fixed step does not
//! step until it is released, so data that comes after a wall delay starts at the same step at any
//! load. `bevy-ts/screenshot` waits it out, and a hold never released fails naming it.
//! macOS (Metal), as `screenshot_ready`.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::asset::io::AssetSourceBuilder;
use bevy::asset::io::file::FileAssetReader;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::remote::{BrpMessage, BrpSender, RemotePlugin};
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless;
use bevy_ts_remote::screenshot::{FixedStep, Ready, ReadyHold, ReadyPlugin, ReadyTimeout, ShotWhenReady, pipelines_waiting};
use common::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A script that holds `"data"` from its `Startup`.
const HOLDS: &str = r#"import { app, Startup } from "bevy-ts";
import type { ReadyHold } from "bevy-ts/ready";

export function wait(ready: ReadyHold) {
  ready.hold("data");
  console.log(`held ${ready.isHeld("data")}`);
}
app.addSystems(Startup, wait);
"#;

/// A script that holds nothing.
const FREE: &str = "import { app } from \"bevy-ts\";\n";

/// A headless engine with `bevy-ts/ready`, a camera and the script `src`, its env shot at the
/// `at`th ready frame into `path` (none when `path` is `None`).
fn engine(name: &str, src: &str, path: Option<&Path>, at: u64) -> (TestDir, App) {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", src)]);
    let mut app = render_app();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models").canonicalize().unwrap();
    app.register_asset_source("fox", AssetSourceBuilder::new(move || Box::new(FileAssetReader::new(&root))));
    app.add_plugins(headless::default_plugins(true))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(ReadyPlugin)
        .add_plugins(bevy_ts_cli::ready::ScriptReadyPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera3d::default(), Tonemapping::None, Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
            commands.spawn((DirectionalLight::default(), Transform::from_xyz(1.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
        });
    if let Some(path) = path {
        app.insert_resource({
            let mut shot = ShotWhenReady::new(path);
            shot.at = at;
            shot
        });
    }
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    app.insert_resource(dir.clone());
    (dir, app)
}

fn start(app: &mut App) {
    app.finish();
    app.cleanup();
}

fn apply_script(app: &mut App) {
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

/// Bevy's fox, its run clip looping, spawned with the release of `"data"` once `came` is set: the
/// data a game waits for, and the system that applies it.
fn fox_on_data(app: &mut App, came: Arc<AtomicBool>) {
    app.add_plugins(bevy_ts_animation::PlayAnimationPlugin).add_systems(
        Update,
        move |mut commands: Commands, server: Res<AssetServer>, mut hold: ResMut<ReadyHold>, info: Res<RuntimeInfo>, mut done: Local<bool>| {
            if *done || info.generation == 0 || !came.load(Ordering::Acquire) {
                return;
            }
            *done = true;
            commands.spawn((
                WorldAssetRoot(server.load("fox://Fox.glb#Scene0")),
                bevy_ts_animation::PlayAnimation { clip: server.load("fox://Fox.glb#Animation2"), ..default() },
                Transform::from_xyz(0.0, -1.2, 0.0).with_scale(Vec3::splat(0.03)).with_rotation(Quat::from_rotation_y(1.2)),
            ));
            hold.release("data");
        },
    );
}

/// The fox shot at the 30th ready frame on the fixed step, its data coming `delay` after the
/// script applied (`None`: not before the shot), with `work` of extra time each frame.
fn fox_shot(name: &str, src: &str, delay: Option<Duration>, work: Duration) -> Vec<u8> {
    let shots = TestDir::new(&format!("{name}-shots"));
    let path = shots.join("fox.png");
    let (_game, mut app) = engine(name, src, Some(&path), 30);
    let came = Arc::new(AtomicBool::new(false));
    fox_on_data(&mut app, came.clone());
    app.insert_resource(FixedStep).add_systems(Update, move || std::thread::sleep(work));
    start(&mut app);
    apply_script(&mut app);
    let reply = std::thread::spawn(move || {
        if let Some(delay) = delay {
            std::thread::sleep(delay);
            came.store(true, Ordering::Release);
        }
    });
    until(&mut app, &format!("{} written", path.display()), |app| {
        std::fs::metadata(&path).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>()
    });
    reply.join().unwrap();
    // `save_to_disk` writes from a task; a few more updates let it finish.
    updates(&mut app, 5);
    let bytes = std::fs::read(&path).unwrap();
    png_pixels(&bytes)
}

fn png_pixels(bytes: &[u8]) -> Vec<u8> {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let image = Image::from_buffer(bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).unwrap();
    image.data.expect("the shot's pixels")
}

fn differ(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(a, b)| a != b).count()
}

/// The hold a script sets in `Startup` holds from the first frame the script counts as started:
/// no step comes before it, and time steps from the first frame after the release.
#[test]
fn a_hold_in_startup_holds_from_the_first_started_frame() {
    let (_game, mut app) = engine("ready-hold-startup", HOLDS, None, 0);
    app.insert_resource(FixedStep);
    start(&mut app);
    updates(&mut app, 3);
    apply_script(&mut app);
    assert!(app.world().resource::<ReadyHold>().is_held("data"), "held once the script applied");
    for _ in 0..30 {
        app.update();
        assert_eq!(app.world().resource::<Ready>().steps, 0, "a step while held");
        assert_eq!(app.world().resource::<Time<Virtual>>().elapsed(), Duration::ZERO);
    }
    assert!(messages(&app).iter().any(|m| m == "held true"), "isHeld: {:?}", messages(&app));
    app.world_mut().resource_mut::<ReadyHold>().release("data");
    until(&mut app, "a step after the release", |app| app.world().resource::<Ready>().steps > 0);
    assert_eq!(app.world().resource::<Time<Virtual>>().elapsed(), bevy_ts_remote::screenshot::STEP);
}

/// The fox's data comes at once, then after 400 ms with 30 ms of work a frame, twice: held until
/// it comes, the three shots are one image. Without the hold, data that comes after the shot is
/// missing from it.
#[test]
fn data_that_comes_late_is_shot_the_same_under_load() {
    let quick = fox_shot("ready-hold-quick", HOLDS, Some(Duration::ZERO), Duration::ZERO);
    let lit = quick.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count();
    assert!(lit > 1000, "the fox is in the shot: {lit} pixels");
    for i in 0..2 {
        let late = fox_shot(&format!("ready-hold-late-{i}"), HOLDS, Some(Duration::from_millis(400)), Duration::from_millis(30));
        assert!(quick == late, "held run {} differs in {} of {} bytes", i + 2, differ(&quick, &late), quick.len());
    }
    // Without the hold nothing waits for data that has not come: data that comes after the shot
    // (whenever load makes it late) is missing from it. Data that comes before it starts a load,
    // which the ready count already waits out, so only data after the shot shows the bug here.
    let free_late = fox_shot("ready-free-late", FREE, None, Duration::from_millis(30));
    assert!(free_late != quick, "without the hold the shot waited for the data: the test does not see the bug");
}

/// Asks `bevy-ts/screenshot` for `path`; its answer comes on the receiver.
fn ask(app: &App, path: &Path) -> async_channel::Receiver<bevy::remote::BrpResult> {
    let (tx, rx) = async_channel::bounded(1);
    let message = BrpMessage { method: "bevy-ts/screenshot".into(), params: Some(json!({ "path": path })), sender: tx };
    app.world().resource::<BrpSender>().try_send(message).unwrap();
    rx
}

/// An engine serving `bevy-ts/screenshot`, its script holding `"data"`, a frame rendered.
fn method_engine(name: &str) -> (TestDir, TestDir, App) {
    let shots = TestDir::new(&format!("{name}-shots"));
    let (game, mut app) = engine(name, HOLDS, None, 0);
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    start(&mut app);
    apply_script(&mut app);
    until(&mut app, "a frame rendered", |app| pipelines_waiting(app.world()).is_some());
    (game, shots, app)
}

/// Asked while held, the method captures nothing until the release, then answers with the shot.
#[test]
fn the_method_waits_out_a_hold() {
    let (_game, shots, mut app) = method_engine("ready-hold-method");
    let path: PathBuf = shots.join("method.png");
    let rx = ask(&app, &path);
    for _ in 0..30 {
        app.update();
        assert!(rx.try_recv().is_err(), "answered while held");
    }
    assert!(!path.exists(), "shot while held");
    app.world_mut().resource_mut::<ReadyHold>().release("data");
    let mut answer = None;
    until(&mut app, "the method's answer", |_| {
        answer = rx.try_recv().ok();
        answer.is_some()
    });
    assert_eq!(answer.unwrap().unwrap()["path"], json!(path));
}

/// A hold never released: the method fails after the no-progress window, naming the path and the
/// hold.
#[test]
fn the_method_fails_under_a_hold_never_released() {
    let (_game, shots, mut app) = method_engine("ready-hold-never");
    app.insert_resource(ReadyTimeout(Duration::from_secs(1)));
    let path = shots.join("never.png");
    let rx = ask(&app, &path);
    let mut answer = None;
    until(&mut app, "the method's answer", |_| {
        answer = rx.try_recv().ok();
        answer.is_some()
    });
    let error = answer.unwrap().expect_err("the method failed").message;
    assert!(error.contains(&path.display().to_string()) && error.contains("held by the script: data"), "{error}");
    assert!(!path.exists(), "the method saved a file");
}
