//! Overlay roots (levels' T4): a sidecar level's description layered over an `app.scene` root by
//! key path, applied now and with every generation after.

mod common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::desc::NodeDesc;
use bevy_ts_tsx::{Provenance, TsxNode};
use common::*;
use serde_json::json;

const GAME: &str = r#"import { app, Component, type f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100 }
const spawnPoint = (i: number): [number, number, number] => [i * 2, 0, 0];
app.scene("arena", () => (
  <>
    <entity key="a"><Transform translation={spawnPoint(2)} /><Health initialHp={10} /></entity>
    <entity key="b"><Transform translation={[1, 0, 0]} /></entity>
  </>
));
app.scene("menu", () => <entity key="m" />, { level: "levels/ui/menu.bsn" });
"#;

fn overlay(v: serde_json::Value) -> Vec<NodeDesc> {
    serde_json::from_value(v).unwrap()
}

fn node(app: &mut App, key: &str) -> (Entity, TsxNode) {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.root == "arena" && n.key_path == [key]).map(|(e, n)| (e, n.clone())).unwrap()
}

#[test]
fn a_sidecar_overlay_overrides_a_computed_prop_and_survives_reloads() {
    let dir = project("tsx-overlay", &[("package.json", "{}"), ("src/game.tsx", GAME)]);
    let mut app = app(&dir.join("src/game.tsx"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app.update();
    assert_eq!(bevy_ts_tsx::scene_level(app.world(), "arena").as_deref(), Some("levels/arena.bsn"));
    assert_eq!(bevy_ts_tsx::scene_level(app.world(), "menu").as_deref(), Some("levels/ui/menu.bsn"));
    let (a, _) = node(&mut app, "a");
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 4.0);

    // The viewport dragged a; levels writes the override to the sidecar and layers it.
    let at = json!({ "file": "levels/arena.bsn", "line": 3, "column": 5 });
    let o = overlay(json!([{ "key": "a", "type": "entity", "components": [
        { "type": "Transform", "kind": "rust", "props": { "translation": [9, 0, 0] }, "overrides": { "translation": at } },
        { "type": "game::Health", "kind": "ts", "props": { "hp": 55 }, "overrides": { "hp": at } }
    ] }]));
    let stats = bevy_ts_tsx::set_overlay(app.world_mut(), "arena", o.clone()).expect("applies");
    assert_eq!(stats.written, 2, "{stats:?}");
    let (a2, n) = node(&mut app, "a");
    assert_eq!(a2, a, "same entity");
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 9.0);
    let t = &n.components["Transform"].fields["translation"];
    assert_eq!(t.provenance, Provenance::Override, "{t:?}");

    // A reload layers it again: the computed prop stays overridden.
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 9.0);

    // An overlay that no longer fits the scene fails the generation, at the level's line.
    let mut bad = o.clone();
    bad[0].components[0].props.insert("transl".into(), json!([0, 0, 0]));
    assert!(bevy_ts_tsx::set_overlay(app.world_mut(), "arena", bad).is_err(), "refused, nothing changes");
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 9.0);

    // A save that renames the field the overlay sets: the generation fails at the level's line.
    write(&dir, "src/game.tsx", &GAME.replace("hp: f32 = 100", "life: f32 = 100").replace("initialHp={10}", "initialLife={10}"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors.iter().any(|e| e.message.contains("no field hp") && e.location.as_ref().is_some_and(|l| l.file.ends_with("arena.bsn") && l.line == 3)), "{:?}", r.errors);
    write(&dir, "src/game.tsx", GAME);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);

    // Cleared: the scene's own value comes back.
    bevy_ts_tsx::clear_overlay(app.world_mut(), "arena");
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 4.0);
}
