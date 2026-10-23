//! `Cameras` (`bevy-ts/camera`) in a headless 960x540 engine: world points projected to the
//! viewport in logical pixels, `undefined` where Bevy cannot project, one host call per batch.
//! macOS (Metal), as the other GPU tests.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts::host::HostParams;
use bevy_ts_cli::compile::Compiler;
use common::*;
use std::path::Path;
use std::sync::Arc;

const SRC: &str = r#"import { app, Startup, Update } from "bevy-ts";
import type { Added, Commands, Entity, Query } from "bevy-ts";
import { Camera } from "bevy-ts/bevy";
import type { Cameras } from "bevy-ts/camera";

let cam: Entity | undefined;
let light: Entity | undefined;
let seen = 0;
const out = new Float32Array(4);

const at = (v: { x: number; y: number } | undefined) => (v ? `${v.x.toFixed(1)},${v.y.toFixed(1)}` : "none");

export function spawn(commands: Commands) {
  light = commands.spawnNative("directionalLight", {});
}

export function project(cameras: Cameras, fresh: Query<[Entity], Added<Camera>>) {
  let added = false;
  fresh.each((e) => {
    cam = e;
    added = true;
    seen = 0;
  });
  if (cam === undefined || light === undefined) return;
  if (!added) seen++;
  if (added || seen === 2) {
    const tag = added ? "spawn" : "later";
    const size = at(cameras.logicalViewportSize(cam));
    const centre = at(cameras.worldToViewport(cam, [0, 0, 0]));
    const right = at(cameras.worldToViewport(cam, [100, 0, 0]));
    const behind = at(cameras.worldToViewport(cam, [0, 0, 20]));
    console.log(`${tag} size ${size} centre ${centre} right ${right} behind ${behind}`);
    try {
      cameras.worldToViewport(light, [0, 0, 0]);
    } catch (e) {
      console.log(`${tag} threw ${(e as Error).message} light ${light}`);
    }
    try {
      cameras.worldsToViewport(cam, new Float32Array(6), new Float32Array(2));
    } catch (e) {
      console.log(`${tag} short ${(e as Error).message}`);
    }
  }
  if (seen === 3) {
    const n = cameras.worldsToViewport(cam, new Float32Array([0, 0, 0, 0, 0, 20]), out);
    console.log(`batch ${n} ${out[0].toFixed(1)},${out[1].toFixed(1)} ${out[2]},${out[3]}`);
  }
}

app.addSystems(Startup, spawn).addSystems(Update, project);
"#;

fn gpu_app(entry: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts_cli::headless::HeadlessPlugin)
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
    .add_plugins(bevy_ts_cli::camera::CamerasPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app.finish();
    app.cleanup();
    app
}

fn line<'a>(m: &'a [String], prefix: &str) -> &'a str {
    m.iter().find(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix:?} in {m:?}"))
}

#[test]
fn world_points_project_to_logical_pixels() {
    let dir = project("cameras", &[("game.ts", SRC)]);
    let mut app = gpu_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);

    // Spawned between frames: the next frame's systems see it before Bevy sizes its viewport.
    let looking = Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    app.world_mut().spawn((Camera3d::default(), looking));
    updates(&mut app, 3);

    // The fourth frame: the batch is one host call. It also collects the third's console output.
    let before = app.world().resource::<HostParams>().calls;
    app.update();
    assert_eq!(app.world().resource::<HostParams>().calls - before, 1);
    let m = messages(&app);
    assert_eq!(line(&m, "spawn size"), "spawn size none centre none right none behind none");

    // Two frames on: the centre of the 960x540 image, off screen to the right, and behind.
    let later = line(&m, "later size");
    let parts: Vec<&str> = later.split(' ').collect();
    assert_eq!(parts[2], "960.0,540.0", "{later}");
    assert_eq!(parts[4], "480.0,270.0", "{later}");
    let right_x: f32 = parts[6].split(',').next().unwrap().parse().unwrap();
    assert!(right_x > 960.0, "{later}");
    assert_eq!(parts[8], "none", "{later}");

    // Not a camera: throws, naming param, method and entity.
    let threw = line(&m, "later threw");
    let light = threw.rsplit(' ').next().unwrap();
    assert!(threw.contains("Cameras.worldToViewport") && threw.contains(&format!("entity {light} is not a camera")), "{threw}");

    // A batch whose `out` cannot hold every point throws rather than drop the rest.
    let short = line(&m, "later short");
    assert!(short.contains("Cameras.worldsToViewport: points holds 6 floats and out 2"), "{short}");

    // The batch: one projected, `NaN, NaN` for the point behind.
    app.update();
    let m = messages(&app);
    assert_eq!(line(&m, "batch"), "batch 1 480.0,270.0 NaN,NaN");
}
