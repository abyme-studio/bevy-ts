//! Reflected assets from scripts (reflect-assets): `assets.add`/`get`/`set` in a system and in a
//! TSX render, refs and `T.of` values in `Handle<T>` slots, wrong types and stale refs. Headless,
//! on the V8 engine.

use crate::common;

use bevy::asset::AssetEvent;
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use common::*;
use std::path::{Path, PathBuf};

const HEAD: &str = "import { app, asset, assets, Startup, Update } from \"bevy-ts\";\nimport { MeshMaterial3dStandardMaterial } from \"bevy-ts/bevy\";\nconst StandardMaterial = asset(\"bevy_pbr::pbr_material::StandardMaterial\");\n";

fn assets_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Image>().init_asset::<StandardMaterial>();
    app.register_asset_reflect::<Image>().register_asset_reflect::<StandardMaterial>();
    app.register_type::<MeshMaterial3d<StandardMaterial>>();
    app
}

fn setup(name: &str, body: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", &format!("{HEAD}{body}"))]);
    let mut app = assets_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    (dir, app)
}

fn materials(app: &App) -> Vec<StandardMaterial> {
    app.world().resource::<Assets<StandardMaterial>>().iter().map(|(_, m)| m.clone()).collect()
}

fn entity_materials(app: &mut App) -> Vec<AssetId<StandardMaterial>> {
    let world = app.world_mut();
    world.query::<&MeshMaterial3d<StandardMaterial>>().iter(world).map(|m| m.id()).collect()
}

fn logged(app: &App, needle: &str) -> bool {
    messages(app).iter().any(|m| m.contains(needle))
}

#[test]
fn a_system_adds_reads_and_sets_a_material() {
    let (_dir, app) = setup(
        "assets-add-set",
        r##"
let mat: any = null;
let done = false;
export function make() {
  mat = assets.add(StandardMaterial, { base_color: "#ff8800", unlit: true });
  console.log(`added ${mat.$asset} unlit=${assets.get(mat).unlit}`);
}
export function tweak() {
  if (done) return;
  done = true;
  assets.set(mat, { perceptual_roughness: 0.25 });
  console.log(`roughness=${assets.get(mat).perceptual_roughness}`);
}
app.addSystems(Startup, make).addSystems(Update, tweak);
"##,
    );
    assert!(logged(&app, "added 1 unlit=true"), "{:?}", messages(&app));
    assert!(logged(&app, "roughness=0.25"), "{:?}", messages(&app));
    let ms = materials(&app);
    assert_eq!(ms.len(), 1);
    assert!(ms[0].unlit);
    assert_eq!(ms[0].perceptual_roughness, 0.25);
    assert_eq!(ms[0].base_color, Color::srgb_u8(0xff, 0x88, 0x00));
}

#[test]
fn a_ref_from_a_render_goes_on_a_mesh_entity() {
    let (_dir, mut app) = setup(
        "assets-ref-tsx",
        r##"
app.scene("s", () => {
  const mat = assets.add(StandardMaterial, { unlit: true });
  return <entity name="it"><MeshMaterial3dStandardMaterial value={mat} /></entity>;
});
"##,
    );
    let ids = entity_materials(&mut app);
    assert_eq!(ids.len(), 1);
    let m = app.world().resource::<Assets<StandardMaterial>>().get(ids[0]).expect("the entity's material is in Assets");
    assert!(m.unlit);
}

#[test]
fn equal_values_share_one_material() {
    let (_dir, mut app) = setup(
        "assets-of-share",
        r##"
app.scene("s", () => (
  <entity name="root">
    <entity name="a"><MeshMaterial3dStandardMaterial value={StandardMaterial.of({ unlit: true, base_color: "#fff" })} /></entity>
    <entity name="b"><MeshMaterial3dStandardMaterial value={StandardMaterial.of({ base_color: "#fff", unlit: true })} /></entity>
  </entity>
));
"##,
    );
    let ids = entity_materials(&mut app);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1], "one handle for equal props");
    assert_eq!(materials(&app).len(), 1, "one material built");
}

#[test]
fn a_ref_of_the_wrong_type_names_both() {
    let (_dir, app) = setup(
        "assets-wrong-type",
        r##"
export function make() {
  const mat = assets.add(StandardMaterial, {});
  try {
    assets.add(StandardMaterial, { base_color_texture: mat });
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Startup, make);
"##,
    );
    let m = messages(&app);
    let line = m.iter().find(|l| l.starts_with("threw ")).unwrap_or_else(|| panic!("no throw: {m:?}"));
    assert!(line.contains("StandardMaterial") && line.contains("Image"), "{line}");
}

#[test]
fn a_ref_from_before_a_reload_is_stale() {
    let (dir, mut app) = setup(
        "assets-stale",
        r##"
export function make() {
  (globalThis as any).old = assets.add(StandardMaterial, {});
}
app.addSystems(Startup, make);
"##,
    );
    write(
        &dir,
        "game.tsx",
        &format!(
            "{HEAD}{}",
            r##"
let tried = false;
export function probe() {
  if (tried) return;
  tried = true;
  try {
    assets.get((globalThis as any).old);
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, probe);
"##
        ),
    );
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    assert!(logged(&app, "stale asset ref"), "{:?}", messages(&app));
    assert_eq!(materials(&app).len(), 1, "a hot reload keeps the old generation's asset");
}

#[test]
fn the_protocol_reads_a_script_made_handle_as_its_ref_and_writes_refs_and_values() {
    use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
    const MAT3D: &str = "bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::pbr_material::StandardMaterial>";
    const MAT: &str = "bevy_pbr::pbr_material::StandardMaterial";
    let dir = project(
        "assets-protocol",
        &[(
            "game.tsx",
            &format!(
                "{HEAD}{}",
                r##"
app.scene("s", () => {
  const a = assets.add(StandardMaterial, { unlit: true });
  assets.add(StandardMaterial, { perceptual_roughness: 0.5 });
  return <entity name="it"><MeshMaterial3dStandardMaterial value={a} /></entity>;
});
"##
            ),
        )],
    );
    let mut app = assets_app(&dir.join("game.tsx"));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    let call = |app: &mut App, method: &str, params: serde_json::Value| {
        let id = *app.world().resource::<RemoteMethods>().get(method).unwrap();
        match id {
            RemoteMethodSystemId::Instant(id) => app.world_mut().run_system_with(id, Some(params)).unwrap().unwrap(),
            RemoteMethodSystemId::Watching(_) => unreachable!(),
        }
    };
    let world = app.world_mut();
    let e = world.query_filtered::<Entity, With<MeshMaterial3d<StandardMaterial>>>().single(world).unwrap();
    let got = call(&mut app, "bevy-ts/get_components", serde_json::json!({ "entity": e, "components": [MAT3D] }));
    assert_eq!(got["components"][MAT3D], serde_json::json!({ "value": { "$asset": 1, "type": MAT } }), "{got:#}");

    call(&mut app, "bevy-ts/mutate_components", serde_json::json!({ "entity": e, "component": MAT3D, "path": "", "value": { "value": { "$asset": 2, "type": MAT } } }));
    let id = entity_materials(&mut app)[0];
    let m = app.world().resource::<Assets<StandardMaterial>>().get(id).unwrap().clone();
    assert_eq!(m.perceptual_roughness, 0.5, "the entity holds ref 2's material");

    call(&mut app, "bevy-ts/mutate_components", serde_json::json!({ "entity": e, "component": MAT3D, "path": "", "value": { "value": { "$of": MAT, "props": { "base_color": "#f00" } } } }));
    let id = entity_materials(&mut app)[0];
    let m = app.world().resource::<Assets<StandardMaterial>>().get(id).expect("a value's material is in Assets after the write");
    assert_eq!(m.base_color, Color::srgb(1.0, 0.0, 0.0));
}

#[test]
fn a_failed_reload_leaves_the_live_refs_working() {
    let (dir, mut app) = setup(
        "assets-failed-reload",
        r##"
let mat: any = null;
export function make() {
  mat = assets.add(StandardMaterial, {});
}
export function tweak() {
  if (!(globalThis as any).go) return;
  (globalThis as any).go = false;
  try {
    assets.set(mat, { perceptual_roughness: 0.5 });
    console.log("set ok");
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Startup, make).addSystems(Update, tweak);
"##,
    );
    write(&dir, "game.tsx", &format!("{HEAD}{}", "(globalThis as any).go = true;\nassets.add(StandardMaterial, { unlit: true });\nthrow new Error(\"boom\");\n"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed, "{:?}", r.errors);
    updates(&mut app, 2);
    assert!(logged(&app, "set ok"), "{:?}", messages(&app));
    let ms = materials(&app);
    assert_eq!(ms.len(), 1, "the failed load's material never stays");
    assert_eq!(ms[0].perceptual_roughness, 0.5);
}

#[test]
fn assets_throw_outside_a_run() {
    let (_dir, mut app) = setup(
        "assets-outside",
        r##"
setTimeout(() => {
  try {
    assets.add(StandardMaterial, {});
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}, 0);
"##,
    );
    updates(&mut app, 3);
    assert!(logged(&app, "assets.add can only be called while a system runs, TSX renders or the script loads"), "{:?}", messages(&app));
}

/// `assets_app` with meshes, which `<material>` needs.
fn material_setup(name: &str, body: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", &format!("{HEAD}{body}"))]);
    let mut app = assets_app(&dir.join("game.tsx"));
    app.init_asset::<Mesh>();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    (dir, app)
}

#[test]
fn an_element_and_a_value_share_one_material() {
    let (_dir, mut app) = material_setup(
        "assets-element-value-share",
        r##"
app.scene("s", () => (
  <entity name="root">
    <entity name="a"><material color="#fff" roughness={0.3} /></entity>
    <entity name="b"><MeshMaterial3dStandardMaterial value={StandardMaterial.of({ perceptual_roughness: 0.3, base_color: "#fff" })} /></entity>
  </entity>
));
"##,
    );
    let ids = entity_materials(&mut app);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1], "<material> is the StandardMaterial.of value of its props");
    let ms = materials(&app);
    assert_eq!(ms.len(), 1, "one material built");
    assert_eq!((ms[0].base_color, ms[0].perceptual_roughness), (Color::srgb(1.0, 1.0, 1.0), 0.3));
}

#[test]
fn a_value_changed_on_re_render_takes_its_new_material() {
    let (_dir, mut app) = material_setup(
        "assets-of-rerender",
        r##"
import { signal, type Local, type u32 } from "bevy-ts";
export const [dark, setDark] = signal(false);
export function flip(frames: Local<{ n: u32 }>) { frames.n += 1; if (frames.n === 3) setDark(true); }
app.addSystems(Update, flip);
app.scene("s", () => (
  <entity name="a"><MeshMaterial3dStandardMaterial value={() => StandardMaterial.of({ base_color: dark() ? "#000" : "#fff" })} /></entity>
));
"##,
    );
    updates(&mut app, 3);
    let ids = entity_materials(&mut app);
    let ms = app.world().resource::<Assets<StandardMaterial>>();
    assert_eq!(ms.get(ids[0]).unwrap().base_color, Color::srgb(0.0, 0.0, 0.0), "the #000 material");
    assert_eq!(ms.len(), 2, "#fff built once, then #000; nothing rebuilt");
    updates(&mut app, 3);
    assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 2, "no build per frame");
}

#[test]
fn a_texture_path_is_the_image_load_of_its_value() {
    let (_dir, mut app) = material_setup(
        "assets-texture-load-share",
        r##"
import { Image } from "bevy-ts/bevy";
app.scene("s", () => (
  <entity name="root">
    <entity name="a"><material baseColorTexture="textures/a.png" /></entity>
    <entity name="b"><MeshMaterial3dStandardMaterial value={StandardMaterial.of({ base_color_texture: Image.load("textures/a.png") })} /></entity>
    <entity name="c"><material normalMapTexture="textures/n.png" /></entity>
  </entity>
));
"##,
    );
    let ids = entity_materials(&mut app);
    assert_eq!(ids.len(), 3);
    let ms = app.world().resource::<Assets<StandardMaterial>>();
    let textured: Vec<_> = ids.iter().map(|id| ms.get(*id).unwrap()).collect();
    assert_eq!(textured.iter().filter(|m| m.base_color_texture.is_some()).count(), 2);
    assert_eq!(ms.len(), 2, "the element and Image.load's value share one material; the normal map is its own");
    let server = app.world().resource::<AssetServer>();
    let path = |h: &Option<Handle<Image>>| h.as_ref().and_then(|h| server.get_path(h.id())).map(|p| p.to_string());
    assert!(textured.iter().any(|m| path(&m.normal_map_texture).as_deref() == Some("textures/n.png")));
}

#[test]
fn a_failed_reload_adds_no_asset() {
    let (dir, mut app) = setup("assets-failed-reload", "assets.add(StandardMaterial, { unlit: true });\n");
    assert_eq!(materials(&app).len(), 1);
    let mut cursor = app.world().resource::<Messages<AssetEvent<StandardMaterial>>>().get_cursor_current();
    // The new generation adds while it loads, then throws.
    write(&dir, "game.tsx", &format!("{HEAD}assets.add(StandardMaterial, {{ unlit: false }});\nthrow new Error(\"boom\");\n"));
    let r = reload(&mut app, false);
    assert!(!r.errors.is_empty(), "the reload fails");
    app.update();
    let events = cursor.read(app.world().resource::<Messages<AssetEvent<StandardMaterial>>>()).count();
    assert_eq!(events, 0, "the failed load's asset never went into Assets");
    let live = materials(&app);
    assert!(live.len() == 1 && live[0].unlit, "the live generation's material is unchanged");
}
