//! Script passes without a renderer (change `script-fullscreen-pass`): the `passes` section's
//! check and apply, slots by order then name, and each camera's params following its kind.

use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::log::ScriptLog;
use bevy_ts_material::pass::{self, CameraPasses, PassKinds, frame_fields, slot_bytes};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(crate) fn file(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/passes").join(name).to_string_lossy().into_owned()
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Shader>()
        .init_resource::<PassKinds>()
        .init_resource::<ScriptLog>();
    app
}

/// A `pass()` declaration as `bevy-ts/material` writes it.
fn decl(name: &str, params: Value, order: f64) -> Value {
    decl_of(name, params, "plain.wgsl", order)
}

/// A `pass()` declaration with its fragment from `tests/assets/passes`.
pub(crate) fn decl_of(name: &str, params: Value, fragment: &str, order: f64) -> Value {
    json!({ "name": name, "params": params, "fragment": file(fragment), "order": order, "location": { "file": "game.tsx", "line": 3, "column": 14 } })
}

/// Checks and applies one generation's `passes` section.
pub(crate) fn load(app: &mut App, generation: u64, items: Vec<Value>) -> Result<(), Vec<ScriptError>> {
    let bytes = serde_json::to_vec(&json!({ "items": items })).unwrap();
    pass::check(app.world_mut(), generation, &bytes, None)?;
    pass::apply(app.world_mut(), generation, true);
    Ok(())
}

fn amount() -> Value {
    json!([{ "name": "amount", "type": "f32" }])
}

fn slots(app: &App) -> Vec<(String, u8)> {
    app.world().resource::<PassKinds>().live.iter().map(|(n, k)| (n.clone(), k.slot)).collect()
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn slots_follow_order_then_name_and_a_fifth_kind_is_refused() {
    let mut app = app();
    load(&mut app, 1, vec![decl("b", amount(), 0.0), decl("a", amount(), 0.0), decl("z", amount(), -1.0), decl("c", amount(), 2.0)]).unwrap();
    assert_eq!(slots(&app), [("a".into(), 1), ("b".into(), 2), ("c".into(), 3), ("z".into(), 0)]);
    let errors = load(&mut app, 2, vec![decl("a", amount(), 0.0), decl("b", amount(), 0.0), decl("c", amount(), 0.0), decl("d", amount(), 0.0), decl("e", amount(), 0.0)]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("pass e: at most 4 pass kinds"), "{}", errors[0].message);
    assert_eq!(errors[0].location.as_ref().map(|l| l.line), Some(3), "at the fifth pass() call");
    assert_eq!(slots(&app).len(), 4, "the failed generation changed nothing");
}

#[test]
fn a_reorder_moves_each_cameras_params_with_its_kind() {
    let mut app = app();
    load(&mut app, 1, vec![decl("Tint", amount(), 0.0), decl("Invert", amount(), 1.0)]).unwrap();
    let cam = app.world_mut().spawn(Camera2d).id();
    pass::set(app.world_mut(), cam, "Tint", json!({ "amount": 0.25 }).as_object().unwrap().clone(), None);
    pass::set(app.world_mut(), cam, "Invert", json!({ "amount": 0.75 }).as_object().unwrap().clone(), None);
    let amount_in = |app: &App, slot| slot_bytes(app.world(), cam, slot).map(|b| f32_at(&b, 24));
    assert_eq!((amount_in(&app, 0), amount_in(&app, 1)), (Some(0.25), Some(0.75)));
    let built = app.world().resource::<PassKinds>().shaders_built;
    // The orders swapped: each kind's bytes follow it to its new slot, and so does its shader.
    load(&mut app, 2, vec![decl("Tint", amount(), 1.0), decl("Invert", amount(), 0.0)]).unwrap();
    assert_eq!(slots(&app), [("Invert".into(), 0), ("Tint".into(), 1)]);
    assert_eq!((amount_in(&app, 0), amount_in(&app, 1)), (Some(0.75), Some(0.25)));
    assert_eq!(app.world().resource::<PassKinds>().shaders_built, built + 2, "both shaders moved");
    let shaders = app.world().resource::<Assets<Shader>>();
    let source = |slot: usize| match &shaders.get(&pass::SLOT_SHADERS[slot]).unwrap().source {
        bevy::shader::Source::Wesl(s) | bevy::shader::Source::Wgsl(s) => s.to_string(),
        _ => unreachable!(),
    };
    assert!(source(0).contains("struct InvertParams"), "{}", source(0));
    assert!(source(1).contains("struct TintParams"), "{}", source(1));
    // Dropping a kind takes its slot component off the camera.
    load(&mut app, 3, vec![decl("Invert", amount(), 0.0)]).unwrap();
    assert_eq!((amount_in(&app, 0), amount_in(&app, 1)), (Some(0.75), None));
    pass::remove(app.world_mut(), cam, "Invert");
    assert_eq!(amount_in(&app, 0), None);
    // Tint's element still holds its props, with no kind to draw them; its removal clears the camera.
    assert!(app.world().get::<CameraPasses>(cam).is_some());
    pass::remove(app.world_mut(), cam, "Tint");
    assert!(app.world().get::<CameraPasses>(cam).is_none());
}

#[test]
fn a_reserved_name_or_too_many_bytes_fails_at_pass() {
    let mut app = app();
    let errors = load(&mut app, 1, vec![decl("game::Dusk", json!([{ "name": "time", "type": "f32" }]), 0.0)]).unwrap_err();
    assert!(errors[0].message.contains("pass game::Dusk: param time: the engine writes time every frame"), "{}", errors[0].message);
    assert_eq!(errors[0].location.as_ref().map(|l| (l.line, l.column)), Some((3, 14)));
    let big = json!([{ "name": "m", "type": { "array": "mat4x4f", "len": 4 } }]);
    let errors = load(&mut app, 2, vec![decl("game::Dusk", big, 0.0)]).unwrap_err();
    assert!(errors[0].message.contains("288 bytes with the frame fields; at most 256"), "{}", errors[0].message);
    let screen = load(&mut app, 3, vec![decl("game::Dusk", json!([{ "name": "screen", "type": "f32" }]), 0.0)]).unwrap_err();
    assert!(screen[0].message.contains("generated pass code declares screen"), "{}", screen[0].message);
}

#[test]
fn a_schema_change_re_encodes_by_name() {
    let mut app = app();
    let params = json!([{ "name": "amount", "type": "f32" }, { "name": "tint", "type": "color" }]);
    load(&mut app, 1, vec![decl("game::Dusk", params, 0.0)]).unwrap();
    let cam = app.world_mut().spawn(Camera2d).id();
    pass::set(app.world_mut(), cam, "game::Dusk", json!({ "amount": 0.6, "tint": "#000000" }).as_object().unwrap().clone(), None);
    let bytes = slot_bytes(app.world(), cam, 0).unwrap();
    assert_eq!((f32_at(&bytes, 24), f32_at(&bytes, 32)), (0.6, 0.0));
    // A frame field the engine wrote survives the re-encode.
    let mut with_time = bytes.clone();
    with_time[..4].copy_from_slice(&7.5f32.to_le_bytes());
    app.world_mut().entity_mut(cam).insert(bevy_ts_material::ScriptPass::<0>::from_bytes(&with_time));
    // `amount` moves behind a new `glow`, `tint` goes: each value follows its name.
    let params = json!([{ "name": "glow", "type": "vec4f" }, { "name": "amount", "type": "f32" }]);
    load(&mut app, 2, vec![decl("game::Dusk", params, 0.0)]).unwrap();
    let bytes = slot_bytes(app.world(), cam, 0).unwrap();
    assert_eq!(f32_at(&bytes, 48), 0.6, "amount after glow (32..48)");
    assert_eq!(f32_at(&bytes, 32), 0.0, "glow at its default");
    assert_eq!(frame_fields(&bytes).0, 7.5, "time kept");
    let k = &app.world().resource::<PassKinds>().live["game::Dusk"];
    assert!(k.layout.has("glow") && !k.layout.has("tint"));
}

/// A kind that moves to another slot forgets the failure reported in its old one, so a failure
/// of the new slot's shader is reported, whether its declaration stayed or changed.
#[test]
fn a_moved_kind_forgets_its_old_slots_reported_failure() {
    let mut app = app();
    load(&mut app, 1, vec![decl("game::K", amount(), 0.0)]).unwrap();
    let mark = |app: &mut App| app.world_mut().resource_mut::<PassKinds>().live.get_mut("game::K").unwrap().reported = Some(1);
    let reported = |app: &App| {
        let k = &app.world().resource::<PassKinds>().live["game::K"];
        (k.slot, k.reported)
    };
    mark(&mut app);
    load(&mut app, 2, vec![decl("game::A", amount(), -1.0), decl("game::K", amount(), 0.0)]).unwrap();
    assert_eq!(reported(&app), (1, None), "the same declaration, moved");
    mark(&mut app);
    load(&mut app, 3, vec![decl("game::K", json!([]), 0.0)]).unwrap();
    assert_eq!(reported(&app), (0, None), "a new declaration, moved");
}
