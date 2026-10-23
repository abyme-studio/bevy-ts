//! Levels' B2 entry: scene component instances expanded by their TS `scene` in one host call,
//! then mounted as an external root, the scene component on each root.

use crate::common;

use bevy::prelude::*;
use bevy_ts::backend::SourceLocation;
use bevy_ts_tsx::{ExpandItem, TsxNode};
use common::*;
use serde_json::{Map, Value, json};

const UNITS: &str = r#"import { app, ref, signal, SceneComponent, type f32, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export const [boost, setBoost] = signal(0);
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ speed = 2 }: { speed?: f32 }) {
    return (
      <entity name="grunt">
        <Transform translation={() => [speed + boost(), 0, 0]} />
        <entity key="gun"><Transform translation={[0, 1, 0]} /></entity>
      </entity>
    );
  }
}
app.scene("hud", () => <entity key="x" />);
"#;

fn props(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn at(line: u32) -> SourceLocation {
    SourceLocation { file: "levels/arena.bsn".into(), line, column: 1 }
}

#[test]
fn instances_expand_in_one_call_and_mount_with_their_component() {
    let dir = project("tsx-expand", &[("package.json", "{}"), ("src/units.tsx", UNITS)]);
    let mut app = app(&dir.join("src/units.tsx"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let calls = app.world().resource::<bevy_ts_tsx::TsxTree>().reactive.calls;
    let items = [
        ExpandItem { component: "units::Grunt".into(), props: props(json!({ "team": 1, "speed": 3 })), at: at(4) },
        ExpandItem { component: "src/units.tsx#Grunt".into(), props: props(json!({})), at: at(9) },
    ];
    let nodes = bevy_ts_tsx::expand(app.world_mut(), &items).expect("expanded");
    assert_eq!(app.world().resource::<bevy_ts_tsx::TsxTree>().reactive.calls, calls + 1, "one host call");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].components[0].ty, "units::Grunt");
    assert_eq!(nodes[0].components[0].props, props(json!({ "team": 1 })));
    assert!(nodes[0].source.iter().any(|s| s.file.ends_with("arena.bsn") && s.line == 4), "{:?}", nodes[0].source);

    // Mounted as a level root: the component on each root, the bound Transform live next frame.
    let mut nodes = nodes;
    nodes[0].key = "g1".into();
    nodes[1].key = "g2".into();
    bevy_ts_tsx::reconcile_external(app.world_mut(), "level:arena", None, &nodes).expect("reconciled");
    updates(&mut app, 2);
    let world = app.world_mut();
    let found: Vec<(Vec<String>, Entity)> = world.query::<(Entity, &TsxNode)>().iter(world).filter(|(_, n)| n.root == "level:arena").map(|(e, n)| (n.key_path.clone(), e)).collect();
    let g1 = found.iter().find(|(p, _)| p == &["g1"]).unwrap().1;
    assert!(found.iter().any(|(p, _)| p == &["g1", "gun"]));
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "units::Grunt").unwrap();
    let team = app.world().get_by_id(g1, id).unwrap();
    // SAFETY: Grunt is one u8.
    assert_eq!(unsafe { *team.as_ptr() }, 1);
    assert_eq!(app.world().get::<Transform>(g1).unwrap().translation.x, 3.0, "{:?}", logs(&app).iter().map(|l| &l.message).collect::<Vec<_>>());
}

#[test]
fn an_unknown_component_or_a_throwing_scene_fails_with_its_location() {
    let src = UNITS.replace("return (", "if (speed > 9) throw new Error(\"too fast\");\n    return (");
    let dir = project("tsx-expand-err", &[("package.json", "{}"), ("src/units.tsx", &src)]);
    let mut app = app(&dir.join("src/units.tsx"));
    assert!(reload(&mut app, false).errors.is_empty());
    let e = bevy_ts_tsx::expand(app.world_mut(), &[ExpandItem { component: "units::Nope".into(), props: Map::new(), at: at(2) }]).unwrap_err();
    assert!(e[0].message.contains("no scene component named units::Nope"), "{e:?}");
    assert_eq!(e[0].location.as_ref().unwrap().line, 2);
    let e = bevy_ts_tsx::expand(app.world_mut(), &[ExpandItem { component: "units::Grunt".into(), props: props(json!({ "speed": 10 })), at: at(5) }]).unwrap_err();
    assert!(e[0].message.contains("too fast"), "{e:?}");
}

#[test]
fn each_item_gets_the_error_its_own_scene_raised() {
    // Booster's scene succeeds, but sets a signal a mounted grunt's binding throws on: that
    // error lands in the same call, after the throwing grunt's.
    let src = UNITS
        .replace("return (", "if (speed > 9) throw new Error(\"too fast\");\n    return (")
        .replace("() => [speed + boost(), 0, 0]", "() => { if (boost() > 0) throw new Error(\"boosted\"); return [speed, 0, 0]; }")
        .replace(
            "app.scene(",
            "export class Booster extends SceneComponent {\n  static scene() {\n    setBoost(1);\n    return <entity name=\"booster\" />;\n  }\n}\napp.scene(",
        );
    let dir = project("tsx-expand-each", &[("package.json", "{}"), ("src/units.tsx", &src)]);
    let mut app = app(&dir.join("src/units.tsx"));
    assert!(reload(&mut app, false).errors.is_empty());
    app.update();
    let mut live = bevy_ts_tsx::expand(app.world_mut(), &[ExpandItem { component: "units::Grunt".into(), props: Map::new(), at: at(2) }]).expect("expanded");
    live[0].key = "g".into();
    bevy_ts_tsx::reconcile_external(app.world_mut(), "level:arena", None, &live).expect("reconciled");
    updates(&mut app, 2);
    let items = [
        ExpandItem { component: "units::Grunt".into(), props: props(json!({ "speed": 10 })), at: at(4) },
        ExpandItem { component: "units::Booster".into(), props: Map::new(), at: at(6) },
    ];
    let out = bevy_ts_tsx::expand_each(app.world_mut(), &items).expect("one call");
    let e = out[0].as_ref().unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].message.contains("too fast"), "{e:?}");
    assert!(out[1].is_ok(), "{:?}", out[1]);
    assert!(logs(&app).iter().any(|l| l.message.contains("boosted")), "the binding's error is logged");
}
