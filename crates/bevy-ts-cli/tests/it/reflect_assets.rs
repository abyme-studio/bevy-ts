//! Reflected assets from scripts (reflect-assets): `assets.add`/`get`/`set` in a system and in a
//! TSX render, refs and `T.of` values in `Handle<T>` slots, wrong types and stale refs. Headless,
//! on the V8 engine.

use crate::common;

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
function make() {
  mat = assets.add(StandardMaterial, { base_color: "#ff8800", unlit: true });
  console.log(`added ${mat.$asset} unlit=${assets.get(mat).unlit}`);
}
function tweak() {
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
function make() {
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
function make() {
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
function probe() {
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
fn assets_throw_outside_a_run() {
    let dir = project("assets-outside", &[("game.tsx", &format!("{HEAD}assets.add(StandardMaterial, {{}});\n"))]);
    let mut app = assets_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let m = r.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(m.contains("assets.add can only be called while a system runs or TSX renders"), "{m}");
}
