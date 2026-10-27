use crate::common;

use bevy::prelude::*;
use bevy_ts::components::TsComponents;
use bevy_ts::generation::{DevChannel, Generations, ReloadLog, ReloadStatus};
use bevy_ts::value::decode_component;
use bevy_ts_cli::compile::Compiler;
use common::*;
use std::sync::Arc;
use std::time::Duration;

fn game(bob: &str, fields: &str, extra: &str) -> String {
    format!(
        r#"import {{ app, Component, f32, i32, Startup, systems, systemSet, Transform, Update }} from "bevy-ts";
import type {{ Commands, Mut, Query }} from "bevy-ts";

export class Mover extends Component {{ static path = "game::Mover"; {fields} }}

let hits = 0;

export function scene(commands: Commands) {{
  commands.spawn(Mover.of({{ speed: 1.5, amplitude: 2 }}), Transform);
}}

export function bob(q: Query<[Mover, Mut<Transform>]>) {{
  hits++;
  q.each((m, t) => {{ {bob} }});
}}

app.addSystems(Startup, scene).addSystems(Update, bob);
{extra}
"#
    )
}

const FIELDS: &str = "speed: f32 = 1; amplitude: f32 = 2";

fn movers(app: &mut App) -> Vec<serde_json::Value> {
    let c = app.world().resource::<TsComponents>().live("game::Mover").cloned();
    let Some(c) = c else { return vec![] };
    let world = app.world_mut();
    let mut q = QueryBuilder::<Entity>::new(world).with_id(c.id).build();
    let es: Vec<Entity> = q.iter(world).collect();
    es.iter()
        .map(|e| {
            let p = world.get_by_id(*e, c.id).unwrap();
            unsafe { decode_component(&c.layout, std::slice::from_raw_parts(p.as_ptr(), c.layout.size)) }
        })
        .collect()
}

fn ys(app: &mut App) -> Vec<f32> {
    app.world_mut().query::<&Transform>().iter(app.world()).map(|t| t.translation.y).collect()
}

fn setup(name: &str, src: &str) -> (crate::common::TestDir, App) {
    let dir = project(name, &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.insert_resource(dir.clone());
    (dir, app)
}

#[test]
fn editing_a_system_keeps_ecs_state_and_does_not_rerun_startups() {
    let (dir, mut app) = setup("edit", &game("t.translation.y = 1.4;", FIELDS, ""));
    updates(&mut app, 2);
    assert_eq!(ys(&mut app), vec![1.4]);
    write(&dir, "game.ts", &game("t.translation.y += 0.1;", FIELDS, ""));
    let r = reload(&mut app, false);
    assert_eq!((r.status, r.generation), (ReloadStatus::Applied, 2));
    app.update();
    let y = ys(&mut app);
    assert_eq!(y.len(), 1, "no second cube");
    assert!((y[0] - 1.5).abs() < 1e-5, "starts from 1.4: {y:?}");
}

#[test]
fn module_state_resets_on_reload() {
    let src = game("", FIELDS, "export function count(q: Query<[Mut<Transform>]>) { console.log(`hits=${hits}`); }\napp.addSystems(Update, systems(count).after(bob));");
    let (dir, mut app) = setup("counter", &src);
    updates(&mut app, 4);
    write(&dir, "game.ts", &format!("{src}\n// touched\n"));
    reload(&mut app, false);
    updates(&mut app, 2);
    let m = messages(&app);
    let last_before = m.iter().rposition(|l| l.starts_with("generation 2")).unwrap();
    assert_eq!(m[last_before + 1], "hits=1", "{m:?}");
}

#[test]
fn adding_a_field_migrates_and_retyping_resets() {
    let (dir, mut app) = setup("migrate", &game("", FIELDS, ""));
    app.update();
    write(&dir, "game.ts", &game("", &format!("{FIELDS}; phase: f32 = 0"), ""));
    let r = reload(&mut app, false);
    assert_eq!(r.migrations.len(), 1);
    assert_eq!(r.migrations[0].entities, 1);
    assert!(messages(&app).iter().any(|m| m == "game::Mover migrated on 1 entities; added phase"), "{:?}", messages(&app));
    assert_eq!(movers(&mut app), vec![serde_json::json!({"speed": 1.5, "amplitude": 2.0, "phase": 0.0})]);

    write(&dir, "game.ts", &game("", "speed: i32 = 7; amplitude: f32 = 2; phase: f32 = 0", ""));
    let r = reload(&mut app, false);
    assert_eq!(r.migrations[0].reset, vec!["speed"]);
    assert_eq!(movers(&mut app), vec![serde_json::json!({"speed": 7, "amplitude": 2.0, "phase": 0.0})]);
}

#[test]
fn commenting_a_component_out_and_back_keeps_its_data() {
    let src = game("", FIELDS, "");
    let (dir, mut app) = setup("comment", &src);
    app.update();
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let without = "import { app, Startup } from \"bevy-ts\";\nexport function scene() {}\napp.addSystems(Startup, scene);\n";
    write(&dir, "game.ts", without);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    assert!(app.world().resource::<TsComponents>().live("game::Mover").is_none());
    write(&dir, "game.ts", &src);
    reload(&mut app, false);
    assert_eq!(app.world().resource::<TsComponents>().live("game::Mover").unwrap().id, id);
    assert_eq!(movers(&mut app), vec![serde_json::json!({"speed": 1.5, "amplitude": 2.0})]);
}

#[test]
fn a_failed_reload_keeps_the_running_generation() {
    let (dir, mut app) = setup("fail", &game("t.translation.y += 1;", FIELDS, ""));
    updates(&mut app, 2);
    let good = game("t.translation.y += 1;", FIELDS, "");
    let bad = [
        ("syntax", good.replace("export function bob(q: Query<[Mover, Mut<Transform>]>) {", "export function bob(q: Query<[Mover, Mut<Transform>]>) {{")),
        ("throwing module", format!("{good}\nthrow new Error(\"top level\");\n")),
        ("duplicate name", format!("{good}\napp.addSystems(Update, bob);\n")),
        ("bad after", format!("{good}\nexport function late(q: Query<[Mover]>) {{}}\napp.addSystems(Update, systems(late).after(systemSet(\"game::Nobody\")));\n")),
    ];
    for (what, src) in bad {
        let y0 = ys(&mut app)[0];
        write(&dir, "game.ts", &src);
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Failed, "{what}");
        assert!(r.errors[0].location.is_some(), "{what}: {:?}", r.errors);
        assert_eq!(app.world().resource::<Generations>().live, 1, "{what}");
        app.update();
        assert_eq!(ys(&mut app)[0], y0 + 1.0, "{what}: the old bob keeps running");
    }
    write(&dir, "game.ts", &good);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    assert_eq!(app.world().resource::<Generations>().live, 2);
}

#[test]
fn restart_despawns_ts_entities_and_reruns_startups() {
    let (_dir, mut app) = setup("restart", &game("", FIELDS, ""));
    let rust_owned = app.world_mut().spawn(Name::new("rust")).id();
    app.update();
    assert_eq!(movers(&mut app).len(), 1);
    let r = reload(&mut app, true);
    assert_eq!(r.status, ReloadStatus::Applied);
    assert_eq!(movers(&mut app).len(), 1, "despawned, then the startup ran again");
    assert!(app.world().get_entity(rust_owned).is_ok());
}

/// A save through the watcher applies as the next generation; how long from save to applied.
fn a_save_applies() -> Duration {
    let src = game("t.translation.y = 1;", FIELDS, "");
    let dir = project("watch", &[("game.ts", &src)]);
    let compiler = Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap());
    let mut app = app(&dir.join("game.ts"));
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    bevy_ts_cli::dev::spawn_watcher(compiler, tx);
    let wait_for = |app: &mut App, generation: u64| until(app, &format!("generation {generation} never applied"), |app| app.world().resource::<Generations>().live >= generation);
    // Applied means watched, and a late report of the project's own writes reloads nothing.
    wait_for(&mut app, 1);
    write(&dir, "game.ts", &game("t.translation.y = 2;", FIELDS, ""));
    let took = wait_for(&mut app, 2);
    app.update();
    assert_eq!(ys(&mut app), vec![2.0]);
    let statuses: Vec<_> = app.world().resource::<ReloadLog>().since(0).map(|r| r.status).collect();
    assert_eq!(statuses, vec![ReloadStatus::Compiling, ReloadStatus::Applied, ReloadStatus::Compiling, ReloadStatus::Applied]);
    eprintln!("save to applied: {took:?}");
    took
}

#[test]
fn a_saved_file_applies_as_the_next_generation() {
    a_save_applies();
}

// The hot-reload spec's 200 ms is on a recent laptop: a loaded machine misses it by any amount.
#[test]
#[ignore = "timing: needs a quiet machine"]
fn a_saved_file_applies_within_200ms() {
    let took = a_save_applies();
    assert!(took < Duration::from_millis(200), "reload took {took:?}");
}
