//! `Cameras` and `MeshRayCast` (`bevy-ts/camera`) in a headless 960x540 engine: world points
//! projected to the viewport in logical pixels and back as rays, `undefined` where Bevy cannot
//! convert, one host call per batch, and rays cast at meshes. macOS (Metal), as the other GPU
//! tests.
#![cfg(target_os = "macos")]

use crate::common;

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

const CONVERT: &str = r#"import { app, Startup, Update } from "bevy-ts";
import type { Added, Commands, Entity, Query } from "bevy-ts";
import { Camera } from "bevy-ts/bevy";
import type { Cameras, Ray3d } from "bevy-ts/camera";

let cam: Entity | undefined;
let light: Entity | undefined;
let seen = 0;
const out = new Float32Array(12);

type V = { x: number; y: number; z?: number } | undefined;
const v = (p: V) => (p ? [p.x, p.y, p.z].filter((n) => n !== undefined).map((n) => n!.toFixed(4)).join(",") : "none");
const ray = (r: Ray3d | undefined) => (r ? `${v(r.origin)} ${v(r.direction)}` : "none");

export function spawn(commands: Commands) {
  light = commands.spawnNative("directionalLight", {});
}

export function convert(cameras: Cameras, fresh: Query<[Entity], Added<Camera>>) {
  let added = false;
  fresh.each((e) => {
    cam = e;
    added = true;
    seen = 0;
  });
  if (cam === undefined || light === undefined) return;
  if (!added) seen++;
  const centre = { x: 480, y: 270 };
  if (added) console.log(`spawn ray ${ray(cameras.viewportToWorld(cam, centre))}`);
  if (seen === 2) {
    console.log(`later ray ${ray(cameras.viewportToWorld(cam, centre))}`);
    const p = { x: 1, y: 0.5, z: 0 };
    const ndc = cameras.worldToNdc(cam, p);
    console.log(`later back ${v(ndc && cameras.ndcToWorld(cam, ndc))}`);
    const at = cameras.worldToViewport(cam, p);
    console.log(`later ndc ${v(ndc)} ${v(at && cameras.viewportToNdc(cam, at))}`);
    console.log(`later depth ${v(cameras.worldToViewportWithDepth(cam, { x: 0, y: 0, z: 0 }))}`);
    console.log(`later flat ${v(cameras.viewportToWorld2d(cam, centre))}`);
    try {
      cameras.viewportToWorld(light, centre);
    } catch (e) {
      console.log(`later threw ${(e as Error).message} light ${light}`);
    }
  }
  if (seen === 3) {
    const n = cameras.viewportsToWorld(cam, new Float32Array([480, 270, 0, 0]), out);
    console.log(`batch ${n} ${Array.from(out, (x) => x.toFixed(4)).join(",")}`);
  }
}

app.addSystems(Startup, spawn).addSystems(Update, convert);
"#;

const RAYS: &str = r#"import { app, Update } from "bevy-ts";
import type { Added, Entity, Query } from "bevy-ts";
import { Camera } from "bevy-ts/bevy";
import type { Cameras, MeshRayCast, RayMeshHit } from "bevy-ts/camera";

let cam: Entity | undefined;
let frame = 0;
const none = new Float64Array(0);
const any = { visibility: "Any", earlyExit: true } as const;
const hits = new Float32Array(28);
const entity = new Float64Array(4);

const v = (p: { x: number; y: number; z: number }) => `${p.x.toFixed(4)},${p.y.toFixed(4)},${p.z.toFixed(4)}`;
const hit = (h: RayMeshHit | undefined) => (h ? `${h.entity} ${v(h.point)} ${v(h.normal)} ${h.distance.toFixed(4)}` : "none");
const down = (x: number) => ({ origin: { x, y: 0, z: 10 }, direction: { x: 0, y: 0, z: -1 } });

export function cast(cameras: Cameras, rays: MeshRayCast, fresh: Query<[Entity], Added<Camera>>) {
  frame++;
  fresh.each((e) => {
    cam = e;
  });
  console.log(`f${frame} c ${hit(rays.castRay(down(20), any, none))}`);
  if (cam === undefined) return;
  const ray = cameras.viewportToWorld(cam, { x: 480, y: 270 });
  if (ray === undefined) return;
  const near = rays.castRay(ray, any, none);
  console.log(`f${frame} near ${v(ray.origin)} ${hit(near)}`);
  console.log(`f${frame} default ${hit(rays.castRay(ray, undefined, none))}`);
  const n = rays.castRayAll(ray, { visibility: "Any", earlyExit: false }, none, hits, entity);
  console.log(`f${frame} all ${n} ${entity[0]} ${hits[2].toFixed(4)} ${entity[1]} ${hits[9].toFixed(4)}`);
  if (near) console.log(`f${frame} excluded ${hit(rays.castRay(ray, any, new Float64Array([near.entity])))}`);
  console.log(`f${frame} beside ${hit(rays.castRay({ origin: { x: 5, y: 5, z: 10 }, direction: { x: 0, y: 0, z: -1 } }, any, none))}`);
  try {
    rays.castRay({ origin: { x: 0, y: 0, z: 10 }, direction: { x: 0, y: 0, z: 0 } }, any, none);
  } catch (e) {
    console.log(`f${frame} bad ${(e as Error).message}`);
  }
}

app.addSystems(Update, cast);
"#;

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

/// The numbers in `s`, split on spaces and commas.
fn nums(s: &str) -> Vec<f32> {
    s.split([' ', ',']).filter_map(|t| t.parse().ok()).collect()
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn viewport_points_convert_to_rays_and_back() {
    let dir = project("cameras-convert", &[("game.ts", CONVERT)]);
    let mut app = gpu_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);

    let looking = Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    app.world_mut().spawn((Camera3d::default(), looking));
    updates(&mut app, 3);

    // The fourth frame converts the batch in one host call.
    let before = app.world().resource::<HostParams>().calls;
    app.update();
    assert_eq!(app.world().resource::<HostParams>().calls - before, 1);
    let m = messages(&app);

    // No viewport size on the spawn frame.
    assert_eq!(line(&m, "spawn ray"), "spawn ray none");

    // The centre pixel's ray starts on the near plane on the camera's axis and looks down -z.
    let ray = nums(line(&m, "later ray"));
    assert!(near(ray[0], 0.0, 1e-4) && near(ray[1], 0.0, 1e-4) && ray[2] > 9.0 && ray[2] < 10.0, "{ray:?}");
    assert!(near(ray[3], 0.0, 1e-4) && near(ray[4], 0.0, 1e-4) && near(ray[5], -1.0, 1e-4), "{ray:?}");

    // World to NDC and back; viewport to NDC agrees with world to NDC.
    let back = nums(line(&m, "later back"));
    assert!(near(back[0], 1.0, 1e-4) && near(back[1], 0.5, 1e-4) && near(back[2], 0.0, 1e-4), "{back:?}");
    let ndc = nums(line(&m, "later ndc"));
    assert!(near(ndc[0], ndc[3], 1e-4) && near(ndc[1], ndc[4], 1e-4), "{ndc:?}");

    // The origin at the centre, ten units deep.
    let depth = nums(line(&m, "later depth"));
    assert!(near(depth[0], 480.0, 1.0) && near(depth[1], 270.0, 1.0) && near(depth[2], 10.0, 1e-3), "{depth:?}");

    // The centre on the near plane, for the 2D conversion.
    let flat = nums(line(&m, "later flat"));
    assert!(near(flat[0], 0.0, 1e-4) && near(flat[1], 0.0, 1e-4), "{flat:?}");

    let threw = line(&m, "later threw");
    let light = threw.rsplit(' ').next().unwrap();
    assert!(threw.contains("Cameras.viewportToWorld") && threw.contains(&format!("entity {light} is not a camera")), "{threw}");

    // Two rays: the centre's as above, the top-left corner's up and to the left.
    app.update();
    let m = messages(&app);
    let batch = nums(line(&m, "batch"));
    assert_eq!(batch[0], 2.0, "{batch:?}");
    assert!(batch[1..7].iter().zip(&ray).all(|(a, b)| near(*a, *b, 1e-4)), "{batch:?} {ray:?}");
    assert!(batch[10] < 0.0 && batch[11] > 0.0 && batch[12] < 0.0, "{batch:?}");
}

#[test]
fn rays_hit_meshes_nearest_first() {
    let dir = project("cameras-rays", &[("game.ts", RAYS)]);
    let mut app = gpu_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);

    // Two unit cubes on the camera's axis, the second three behind the first.
    let cube = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::from_length(1.0));
    let a = app.world_mut().spawn((Mesh3d(cube.clone()), Transform::default())).id().to_bits();
    let b = app.world_mut().spawn((Mesh3d(cube.clone()), Transform::from_xyz(0.0, 0.0, -3.0))).id().to_bits();
    let looking = Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    app.world_mut().spawn((Camera3d::default(), looking));
    updates(&mut app, 4);

    // A third cube, spawned before frame 5: its `Aabb` comes in frame 5's `PostUpdate`.
    let c = app.world_mut().spawn((Mesh3d(cube), Transform::from_xyz(20.0, 0.0, 0.0))).id().to_bits();
    updates(&mut app, 3);
    let m = messages(&app);
    assert_eq!(line(&m, "f5 c"), "f5 c none");
    assert!(line(&m, "f6 c").starts_with(&format!("f6 c {c} ")), "{m:?}");

    // The nearest: the front face of the first cube, its distance from the ray's origin.
    let hit = line(&m, "f4 near");
    assert_eq!(hit.split(' ').nth(3), Some(a.to_string().as_str()), "{hit}");
    let n = nums(hit);
    let p = &n[4..];
    assert!(near(p[0], 0.0, 1e-3) && near(p[1], 0.0, 1e-3) && near(p[2], 0.5, 1e-3), "{hit}");
    assert!(near(p[3], 0.0, 1e-3) && near(p[4], 0.0, 1e-3) && near(p[5], 1.0, 1e-3), "{hit}");
    assert!(near(p[6], n[2] - 0.5, 1e-3), "{hit}");

    // Bevy's default settings hit it too, once the camera has drawn it.
    assert!(line(&m, "f4 default").starts_with(&format!("f4 default {a} ")), "{m:?}");

    // Every hit, nearest first; excluding the first cube finds the second.
    assert_eq!(line(&m, "f4 all"), format!("f4 all 2 {a} 0.5000 {b} -2.5000"));
    let excluded = line(&m, "f4 excluded");
    assert_eq!(excluded.split(' ').nth(2), Some(b.to_string().as_str()), "{excluded}");
    assert!(near(nums(excluded)[3], -2.5, 1e-3), "{excluded}");

    // Beside every mesh, and a ray with no direction.
    assert_eq!(line(&m, "f4 beside"), "f4 beside none");
    let bad = line(&m, "f4 bad");
    assert!(bad.contains("MeshRayCast.castRay") && bad.contains("ray direction"), "{bad}");
}
