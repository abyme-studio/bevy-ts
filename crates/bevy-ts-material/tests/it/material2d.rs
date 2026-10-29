//! Script 2D materials without a renderer (change `script-material2d`): the `materials` section's
//! 2D kinds, their keyed assets and instance rows, and the WGSL they get.

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::backend::ScriptError;
use bevy_ts::log::ScriptLog;
use bevy_ts_material::layout::INSTANCE_IMPORT_2D;
use bevy_ts_material::{MaterialKinds, ScriptInstance, ScriptMaterial2d, ScriptMaterialRenderPlugin, instance, kinds};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

fn file(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/materials2d").join(name).to_string_lossy().into_owned()
}

fn app() -> App {
    let mut app = App::new();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render),
    )
    .add_plugins(ScriptMaterialRenderPlugin)
    .init_resource::<MaterialKinds>()
    .init_resource::<ScriptLog>();
    app.finish();
    app.cleanup();
    app
}

/// A `material2d()` declaration as `bevy-ts/material` writes it.
fn decl(name: &str, params: Value, instance: Value, alpha: Value) -> Value {
    json!({
        "name": name, "params": params, "textures": [], "instance": instance, "fragment": file("planet.wgsl"),
        "alphaMode": alpha, "space": "2d", "location": { "file": "game.tsx", "line": 3, "column": 14 },
    })
}

fn planet(params: Value, instance: Value) -> Value {
    decl("game::Planet", params, instance, json!("opaque"))
}

/// Checks and applies one generation's `materials` section.
fn load(app: &mut App, generation: u64, items: Vec<Value>) -> Result<(), Vec<ScriptError>> {
    let bytes = serde_json::to_vec(&json!({ "items": items })).unwrap();
    kinds::check(app.world_mut(), generation, &bytes, None)?;
    kinds::apply(app.world_mut(), generation, true);
    Ok(())
}

fn props(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn sea_seed() -> (Value, Value) {
    (json!([{ "name": "sea", "type": "color" }]), json!([{ "name": "seed", "type": "f32" }]))
}

#[test]
fn equal_props_share_one_asset_and_instance_rows_hold_their_own() {
    let mut app = app();
    let (params, inst) = sea_seed();
    load(&mut app, 1, vec![planet(params, inst)]).unwrap();
    let world = app.world_mut();
    let mut out = Vec::new();
    for i in 0..100 {
        let e = world.spawn_empty().id();
        let h = instance::apply2d(world, e, "game::Planet", &props(json!({ "sea": "#24a", "seed": i })), None).unwrap();
        out.push((e, h));
    }
    assert!(out.iter().all(|(_, h)| *h == out[0].1), "one asset for equal props");
    assert_eq!(world.resource::<Assets<ScriptMaterial2d>>().len(), 1);
    let bindings = world.resource::<MaterialKinds>().live["game::Planet"].kind.bindings.clone();
    for (i, (e, _)) in out.iter().enumerate() {
        let row = world.get::<ScriptInstance>(*e).expect("a row");
        assert_eq!(row.get(&bindings, "seed"), Some(json!(i as f64)), "entity {i}");
        assert_eq!(bevy_ts_material::instance::packed(world, *e).map(|d| f32::from_bits(d[0])), Some(i as f32));
    }
    // A new sea is a new asset.
    let e = world.spawn_empty().id();
    let other = instance::apply2d(world, e, "game::Planet", &props(json!({ "sea": "#000", "seed": 1 })), None).unwrap();
    assert_ne!(other, out[0].1);
}

#[test]
fn a_3d_alpha_mode_or_a_shared_name_fails_at_material2d() {
    let mut app = app();
    let (params, inst) = sea_seed();
    let errors = load(&mut app, 1, vec![decl("game::Planet", params.clone(), inst.clone(), json!("add"))]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains(r#"use "opaque", "blend" or { mask }"#), "{}", errors[0].message);
    assert_eq!(errors[0].location.as_ref().map(|l| l.line), Some(3));
    // A 3D kind, then a 2D one of its name: the second fails, naming the first.
    let mut glow3d = decl("game::Glow", params.clone(), json!([]), json!("add"));
    glow3d["space"] = json!("3d");
    let mut glow2d = decl("game::Glow", params, inst, json!("opaque"));
    glow2d["location"]["line"] = json!(9);
    let errors = load(&mut app, 2, vec![glow3d, glow2d]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("is declared twice; material game::Glow at game.tsx:3 took the name"), "{}", errors[0].message);
    assert_eq!(errors[0].location.as_ref().map(|l| l.line), Some(9), "at the second call");
}

#[test]
fn a_schema_change_re_encodes_assets_and_rows_by_name() {
    let mut app = app();
    let params = json!([{ "name": "glow", "type": "f32" }, { "name": "sea", "type": "color" }]);
    let inst = json!([{ "name": "seed", "type": "f32" }]);
    load(&mut app, 1, vec![planet(params, inst)]).unwrap();
    let world = app.world_mut();
    let e = world.spawn_empty().id();
    let h = instance::apply2d(world, e, "game::Planet", &props(json!({ "sea": "#ff0000", "glow": 2.0, "seed": 7 })), None).unwrap();
    // Reordered params, and an instance param put before `seed`.
    let params = json!([{ "name": "sea", "type": "color" }, { "name": "glow", "type": "f32" }]);
    let inst = json!([{ "name": "tilt", "type": "vec4f" }, { "name": "seed", "type": "f32" }]);
    load(&mut app, 2, vec![planet(params, inst)]).unwrap();
    let world = app.world();
    let kind = world.resource::<MaterialKinds>().live["game::Planet"].kind.clone();
    let m = world.resource::<Assets<ScriptMaterial2d>>().get(&h).expect("the same asset, re-encoded");
    assert!(std::sync::Arc::ptr_eq(&m.kind, &kind), "on the new kind");
    assert_eq!(kind.bindings.params().decode("glow", &m.data), Some(json!(2.0)));
    assert_eq!(kind.bindings.params().decode("sea", &m.data), Some(json!({ "LinearRgba": { "red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0 } })));
    let row = world.get::<ScriptInstance>(e).unwrap();
    assert_eq!(row.get(&kind.bindings, "seed"), Some(json!(7.0)));
}

#[test]
fn the_2d_declarations_sit_at_group_2_with_the_2d_get_tag() {
    let mut app = app();
    let (params, inst) = sea_seed();
    load(&mut app, 1, vec![planet(params, inst)]).unwrap();
    let h = app.world().resource::<MaterialKinds>().live["game::Planet"].fragment.handle.clone();
    let src = match &app.world().resource::<Assets<Shader>>().get(&h).expect("installed").source {
        bevy::shader::Source::Wesl(s) => s.to_string(),
        other => panic!("not WESL: {other:?}"),
    };
    assert!(src.starts_with(INSTANCE_IMPORT_2D), "{src}");
    assert!(src.contains("@group(2) @binding(0) var<uniform> params: PlanetParams;"), "{src}");
    assert!(src.contains("@group(2) @binding(1) var<storage, read> instances: array<PlanetInstance>;"), "{src}");
    assert!(!src.contains("@group(3)"), "{src}");
}
