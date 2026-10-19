//! Data classes (operator rulings 2026-10-20): Rust-style names from the module path, statics at
//! definition, scene components found by name, and `ScriptHidden`.

mod common;

use bevy_ts::manifest::{PropType, StringKind};
use bevy_ts::ScriptHidden;
use common::*;
use serde_json::json;

const ENEMIES: &str = r#"import { app, Component, SceneComponent, Update, type f32, type u8, type Query } from "bevy-ts";
export class Health extends Component { hp: f32 = 100 }
export class Grunt extends SceneComponent {
  team: u8 = 1;
  static scene({ speed = 2, label }: { speed?: f32; label: string }) {
    return <entity name="grunt" />;
  }
}
console.log(`names ${Health.componentName} ${Grunt.componentName} ${Grunt.source} ${Grunt.fieldNames.join(",")} ${Grunt.sceneProps.join(",")}`);
export function count(q: Query<[Health]>) { console.log(`health ${q.len}`); }
app.addSystems(Update, count);
"#;

#[test]
fn classes_take_rust_style_names_and_scene_components_are_found_by_name() {
    let dir = project("classes-names", &[("package.json", "{}"), ("src/units/enemies.tsx", ENEMIES)]);
    let mut app = app(&dir.join("src/units/enemies.tsx"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let m = messages(&app);
    assert!(m.contains(&"names units::enemies::Health units::enemies::Grunt ts team speed,label".to_string()), "statics at module load: {m:?}");

    let grunt = bevy_ts::plugin_api::component_decl(app.world(), "units::enemies::Grunt").expect("Grunt by name");
    assert_eq!(bevy_ts::plugin_api::component_decl(app.world(), "src/units/enemies.tsx#Grunt").as_ref(), Some(&grunt), "and by module#export");
    assert_eq!(grunt.fields.len(), 1);
    assert_eq!(grunt.fields[0].default, Some(json!(1)));
    let props = grunt.scene.expect("a scene component").props;
    assert_eq!(props.len(), 2);
    assert_eq!((props[0].name.as_str(), props[0].default.clone(), props[0].optional), ("speed", Some(json!(2)), true));
    assert_eq!(props[1].ty, PropType::String { kind: StringKind::String });
    assert!(!props[1].optional);
    let placeable: Vec<_> = bevy_ts::plugin_api::scene_components(app.world()).into_iter().map(|d| d.name).collect();
    assert_eq!(placeable, ["units::enemies::Grunt"]);
}

#[test]
fn script_hidden_entities_are_not_seen_by_ts_queries() {
    let dir = project("classes-hidden", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES)]);
    let mut app = app(&dir.join("src/enemies.tsx"));
    assert!(reload(&mut app, false).errors.is_empty());
    spawn_ts(&mut app, "enemies::Health", json!({}), ());
    spawn_ts(&mut app, "enemies::Health", json!({}), ScriptHidden);
    updates(&mut app, 3);
    let m = messages(&app);
    let seen: Vec<_> = m.iter().filter(|l| l.starts_with("health ")).collect();
    assert!(!seen.is_empty() && seen.iter().all(|l| *l == "health 1"), "{m:?}");
}

#[test]
fn two_classes_with_one_name_fail_the_load_naming_both() {
    let a = "import { Component, type f32 } from \"bevy-ts\";\nexport class Health extends Component { static path = \"game::Health\"; hp: f32 = 1 }\n";
    let dir = project(
        "classes-twice",
        &[("package.json", "{}"), ("src/a.ts", a), ("src/b.ts", &a.replace("Health extends", "Hp extends")), ("src/game.ts", "import \"./a\";\nimport \"./b\";\n")],
    );
    let mut app = app(&dir.join("src/game.ts"));
    let r = reload(&mut app, false);
    let e = r.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(e.contains("game::Health is declared twice: src/a.ts:2 and src/b.ts:2"), "{e}");
}

#[test]
fn a_string_field_fails_at_its_line() {
    let e = load_errors("classes-string", "import { Component } from \"bevy-ts\";\n\nexport class Tagged extends Component { label: string = \"x\" }\n");
    let e = &e[0];
    assert!(e.message.contains("game.ts:3:") && e.message.contains("Tagged field label: strings are not supported"), "{e:?}");
}
