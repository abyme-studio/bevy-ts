//! World saves from a script (openspec world-saves): `commands.saveWorld` and `loadWorld` through
//! V8, the dev save folder, and a name with a path refused in TS. The save and load rules
//! themselves are `crates/bevy-ts/tests/it/saves.rs`'s.

use crate::common;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_ts::saves::{SaveDir, WorldLoaded, WorldSaved};
use common::*;
use serde_json::json;
use std::time::{Duration, Instant};

const GAME: &str = r#"import { app, Component, Resource, Save, Startup, Transform, u32, f32, Update, With } from "bevy-ts";
import type { Commands, Entity, Query, ResMut } from "bevy-ts";
export class Health extends Component { static path = "game::Health"; declare hp: f32 }
export class Follow extends Component { static path = "game::Follow"; declare target: Entity }
export class Phase extends Resource { static path = "game::Phase"; declare n: u32 }
export function setup(c: Commands) {
  const a = c.spawn(Save, Health.of({ hp: 3 }), Transform.of({ translation: [1, 2, 3] }));
  const b = c.spawn(Save, Health.of({ hp: 5 }));
  c.entity(a).insert(Follow.of({ target: b }));
  c.spawn(Health.of({ hp: 9 }));
}
export function drive(p: ResMut<Phase>, q: Query<[Entity], With<Save>>, c: Commands) {
  if (p.n === 1) {
    for (const bad of ["../x", "a/b", ""]) {
      try { c.saveWorld(bad); } catch (e) { console.log(`refused ${(e as Error).message}`); }
    }
    c.saveWorld("slot1");
    p.n = 2;
  } else if (p.n === 3) {
    q.each((e) => c.entity(e).despawn());
    c.loadWorld("slot1");
    p.n = 4;
  }
}
export function report(p: ResMut<Phase>, q: Query<[Entity, Health, Transform, Follow]>, h: Query<[Health], With<Save>>) {
  if (p.n !== 5) return;
  q.each((e, hp, t, f) => {
    const target = h.get(f.target);
    console.log(`a ${hp.hp} ${t.translation.x},${t.translation.y},${t.translation.z} follows ${target ? target[0].hp : "none"}`);
  });
  console.log(`saved ${h.len}`);
  p.n = 6;
}
app.insertResource(Phase.of({ n: 0 })).addSystems(Startup, setup).addSystems(Update, drive, report);
"#;

#[derive(Resource, Default)]
struct Got(Vec<String>);

fn collect(mut s: MessageReader<WorldSaved>, mut l: MessageReader<WorldLoaded>, mut got: ResMut<Got>) {
    got.0.extend(s.read().map(|m| format!("saved {} {:?}", m.name, m.error)));
    got.0.extend(l.read().map(|m| format!("loaded {} {} {:?}", m.name, m.entities.len(), m.error)));
}

fn set_phase(app: &mut App, n: u32) {
    let world = app.world_mut();
    let (id, _) = bevy_ts::plugin_api::component_by_name(world, "game::Phase").unwrap();
    let e = world.resource_entities().get(id).unwrap();
    let bytes = bevy_ts::plugin_api::encode_component(world, "game::Phase", json!({ "n": n }).as_object().unwrap()).unwrap();
    // SAFETY: encoded with the resource's own layout.
    unsafe { bevy_ts::commands::insert_bytes(&mut world.entity_mut(e), id, &bytes) };
}

fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(app) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}: {:?} {:?}", app.world().resource::<Got>().0, messages(app));
        app.update();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_script_saves_and_loads_in_the_dev_folder() {
    let dir = project("saves-script", &[("game.ts", GAME)]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(AssetPlugin::default());
    app.insert_resource(SaveDir(bevy_ts_cli::cli::dev_save_dir(&dir))).init_resource::<Got>().add_systems(Last, collect);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);

    set_phase(&mut app, 1);
    until(&mut app, "the save", |a| a.world().resource::<Got>().0.iter().any(|m| m.starts_with("saved")));
    assert_eq!(app.world().resource::<Got>().0, ["saved slot1 None"]);
    let file = dir.join(".bevy-ts/saves/slot1.scn.ron");
    assert!(file.is_file(), "the dev save is in the project's .bevy-ts/saves");
    let refused: Vec<String> = messages(&app).into_iter().filter(|m| m.starts_with("refused")).collect();
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert!(refused.iter().all(|m| m.contains("1 to 64 of A-Z, a-z, 0-9, _ and -")), "{refused:?}");
    assert!(!dir.join(".bevy-ts/x.scn.ron").exists() && !dir.join("x.scn.ron").exists());

    set_phase(&mut app, 3);
    until(&mut app, "the load", |a| a.world().resource::<Got>().0.iter().any(|m| m.starts_with("loaded")));
    assert_eq!(app.world().resource::<Got>().0[1], "loaded slot1 2 None", "{:?}", messages(&app));
    set_phase(&mut app, 5);
    until(&mut app, "the report", |a| messages(a).iter().any(|m| m.starts_with("saved ")));
    let lines: Vec<String> = messages(&app).into_iter().filter(|m| m.starts_with("a ") || m.starts_with("saved ")).collect();
    assert_eq!(lines, ["a 3 1,2,3 follows 5", "saved 2"]);
}

#[test]
fn a_played_game_saves_in_the_players_data_folder() {
    let dir = bevy_ts_cli::play::save_dir("arena").unwrap();
    assert!(dir.ends_with("arena/saves"), "{}", dir.display());
    #[cfg(target_os = "macos")]
    assert!(dir.to_string_lossy().contains("Library/Application Support/arena/saves"), "{}", dir.display());
    for bad in ["", "..", "a/b", "a\\b"] {
        assert_eq!(bevy_ts_cli::play::save_dir(bad), None, "{bad}");
    }
}
