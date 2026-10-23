//! Resources, Local, run conditions, messages, observers, states, plugins and the setup-closure
//! fallback.

use crate::common;

use bevy::prelude::*;
use common::*;
use serde_json::json;

const HEAD: &str = r#"import { app, chain, Component, EntityEvent, Event, f32, FixedUpdate, inState, Message, not, OnEnter, OnExit, PostUpdate, PreUpdate, query, res, resMut, Resource, resourceExists, onMessage, Startup, States, system, systems, SystemSet, Time, Transform, u32, Update, entity, Mut as MutOf } from "bevy-ts";
import type { Add, Commands, Entity, Local, MessageReader, MessageWriter, Mut, NextState, On, Query, Res, ResMut, State, With } from "bevy-ts";
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
export class Health extends Component { static path = "game::Health"; hp: f32 = 100; max: f32 = 100 }
export class Enemy extends Component { static path = "game::Enemy" }
export class Damage extends Message { static path = "game::Damage"; declare target: Entity; declare amount: f32 }
export class Explode extends EntityEvent { static path = "game::Explode"; static propagate = true; declare radius: f32 }
export class Ping extends Event { static path = "game::Ping"; declare n: u32 }
export class Game extends States("Menu", "Playing", "Paused") { static path = "game::Game" }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

#[test]
fn a_score_resource_is_written_in_place_and_kept_across_reloads() {
    let body = "export function add(score: ResMut<Score>) { score.value += 1; }\napp.insertResource(Score.of({ value: 10 })).addSystems(Update, add);\n";
    let (dir, mut app) = load("score", &src(body));
    updates(&mut app, 3);
    assert_eq!(read_res(&app, "game::Score").unwrap()["value"], 13);
    // A Rust system reading the component id sees the value that frame.
    let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Score").unwrap().0;
    assert!(app.world().resource_entities().get(id).is_some());
    write(&dir, "game.ts", &src(&body.replace("+= 1", "+= 2")));
    reload(&mut app, false);
    app.update();
    assert_eq!(read_res(&app, "game::Score").unwrap()["value"], 15, "kept 13, not re-inserted");
}

#[test]
fn time_is_a_read_only_view() {
    let (_, mut app) = load("time", &src("export function tock(time: Res<Time>) { console.log(`t ${time.delta_secs >= 0} ${time.elapsed_secs >= 0} ${time.delta_secs_f64 === Math.fround(time.delta_secs_f64) || time.delta_secs_f64 >= 0} ${time.elapsed_secs_f64 >= time.elapsed_secs - 0.01}`); }\napp.addSystems(Update, tock);\n"));
    updates(&mut app, 4);
    assert!(messages(&app).iter().any(|m| m.starts_with("t true true true true")), "{:?}", messages(&app));
}

#[test]
fn local_survives_an_edit_and_a_rename_resets_it() {
    let body = |name: &str, step: u32| format!("export function {name}(s: Local<{{ wave: u32 }}>) {{ s.wave += {step}; console.log(`wave ${{s.wave}}`); }}\napp.addSystems(Update, {name});\n");
    let (dir, mut app) = load("local", &src(&body("waves", 1)));
    updates(&mut app, 4);
    write(&dir, "game.ts", &src(&body("waves", 10)));
    reload(&mut app, false);
    updates(&mut app, 2);
    write(&dir, "game.ts", &src(&body("renamed", 1)));
    reload(&mut app, false);
    updates(&mut app, 2);
    let waves: Vec<String> = messages(&app).into_iter().filter(|m| m.starts_with("wave")).collect();
    assert_eq!(&waves[..4], ["wave 1", "wave 2", "wave 3", "wave 4"], "{waves:?}");
    assert_eq!(waves[4], "wave 14", "Local survived the edit: {waves:?}");
    assert!(waves.contains(&"wave 1".to_string()) && waves.last().unwrap() != "wave 15", "rename starts fresh: {waves:?}");
}

#[test]
fn a_skipped_system_costs_no_js_call() {
    let (_, mut app) = load(
        "skipped",
        &src("export function chase(q: Query<[Health]>) { console.log('chase'); }\napp.initState(Game.Menu).addSystems(Update, systems(chase).runIf(inState(Game.Playing)));\n"),
    );
    app.update();
    let before = calls(&app);
    updates(&mut app, 5);
    assert_eq!(calls(&app), before, "no call into JS while in Menu");
    assert!(!messages(&app).contains(&"chase".to_string()));
}

#[test]
fn resource_exists_and_not_conditions() {
    let (_, mut app) = load(
        "conditions",
        &src("export function has() { console.log('has'); }\nexport function lacks() { console.log('lacks'); }\napp.addSystems(Update, systems(has).runIf(resourceExists(Score)), systems(lacks).runIf(not(resourceExists(Score))));\n"),
    );
    updates(&mut app, 3);
    let m = messages(&app);
    assert!(m.contains(&"lacks".to_string()) && !m.contains(&"has".to_string()), "{m:?}");
}

#[test]
fn startup_runs_once_per_identity() {
    let body = |n: u32| format!("export function scene(c: Commands) {{ c.spawn(Health.of({{ hp: {n} }})); }}\napp.addSystems(Startup, scene);\n");
    let (dir, mut app) = load("startup", &src(&body(1)));
    app.update();
    write(&dir, "game.ts", &src(&body(2)));
    reload(&mut app, false);
    let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Health").unwrap().0;
    let world = app.world_mut();
    assert_eq!(QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).count(), 1, "no second spawn");
    reload(&mut app, true);
    let world = app.world_mut();
    assert_eq!(QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).count(), 1, "restart despawns and reruns");
}

#[test]
fn two_readers_see_the_same_messages_once_and_they_expire() {
    let (_, mut app) = load(
        "messages",
        &src(
            r#"let sent = false;
export function send(w: MessageWriter<Damage>) { if (!sent) { sent = true; w.write({ target: 0 as any, amount: 1 }); w.write(Damage.of({ amount: 2 })); const m = w.next(); m.amount = 3; } }
export function readA(r: MessageReader<Damage>) { const xs: number[] = []; r.each((d) => xs.push(d.amount)); if (xs.length) console.log(`a ${xs.join(",")}`); }
export function readB(r: MessageReader<Damage>) { let n = 0; for (const d of r) n += d.amount; if (r.len) console.log(`b ${r.len} ${n}`); }
export function late(r: MessageReader<Damage>) { if (r.len) console.log(`late ${r.len}`); }
app.addMessage(Damage).addSystems(Update, chain(send, readA, readB)).addSystems(PostUpdate, systems(late).runIf(onMessage(Damage)));
"#,
        ),
    );
    updates(&mut app, 6);
    let m = messages(&app);
    assert_eq!(m.iter().filter(|l| l.starts_with("a ")).collect::<Vec<_>>(), ["a 1,2,3"], "{m:?}");
    assert_eq!(m.iter().filter(|l| l.starts_with("b ")).collect::<Vec<_>>(), ["b 3 6"], "{m:?}");
    assert_eq!(m.iter().filter(|l| l.starts_with("late")).count(), 1, "{m:?}");
    let q = &app.world().resource::<bevy_ts::messages::TsPayloads>().messages[0];
    assert_eq!(q.unread(0)[0].1 + q.unread(0)[1].1, 0, "gone after two updates");
}

#[test]
fn an_observer_triggered_from_ts_runs_before_the_next_system() {
    let (_, mut app) = load(
        "explode",
        &src(
            r#"let fired = false;
export function spawn(c: Commands) { const parent = c.spawn(Health.of()); const e = c.spawn(Health.of(), Enemy.of()); c.entity(e).setParent(parent); }
export function a(q: Query<[Entity], With<Enemy>>, c: Commands) { if (fired) return; fired = true; q.each((e) => c.trigger(Explode.of({ entity: e, radius: 3 }))); }
export function onExplode(ex: On<Explode>, c: Commands) { console.log(`boom ${ex.radius}`); c.entity(ex.entity).despawn(); ex.propagate(false); }
export function b(q: Query<[Entity], With<Enemy>>) { console.log(`after ${q.len}`); }
app.addSystems(Startup, spawn).addSystems(Update, chain(a, b)).addObserver(onExplode);
"#,
        ),
    );
    updates(&mut app, 3);
    let m = messages(&app);
    let boom = m.iter().filter(|l| l.starts_with("boom")).count();
    assert_eq!(boom, 1, "propagate(false) stopped it at the target: {m:?}");
    assert!(m.contains(&"after 0".to_string()), "the next system no longer sees it: {m:?}");
}

#[test]
fn a_lifecycle_observer_on_a_rust_insert_runs_at_the_next_stage() {
    let (_, mut app) = load(
        "lifecycle",
        &src("export function healed(add: On<Add<Health>>, q: Query<[Mut<Health>]>) { const row = q.get(add.entity); if (row) { row[0].hp = row[0].max; console.log('healed'); } }\napp.addObserver(healed);\n"),
    );
    let e = spawn_ts(&mut app, "game::Health", json!({ "hp": 5 }), ());
    app.update();
    app.update();
    assert_eq!(read_ts(&app, e, "game::Health").unwrap()["hp"], 100.0);
    assert!(messages(&app).contains(&"healed".to_string()));
}

#[test]
fn entering_playing_runs_exit_then_enter_and_unlocks_systems() {
    let (_, mut app) = load(
        "states",
        &src(
            r#"export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); }
export function leaveMenu() { console.log('exit menu'); }
export function enterPlaying() { console.log('enter playing'); }
export function play() { console.log('playing'); }
app.initState(Game.Menu)
  .addSystems(Update, go)
  .addSystems(OnExit(Game.Menu), leaveMenu)
  .addSystems(OnEnter(Game.Playing), enterPlaying)
  .addSystems(Update, systems(play).runIf(inState(Game.Playing)));
"#,
        ),
    );
    updates(&mut app, 4);
    let m: Vec<String> = messages(&app).into_iter().filter(|l| ["exit menu", "enter playing", "playing"].contains(&l.as_str())).collect();
    assert_eq!(&m[..3], ["exit menu", "enter playing", "playing"], "{m:?}");
    assert_eq!(bevy_ts::messages::state_values(app.world())["game::Game"].as_deref(), Some("Playing"));
}

#[test]
fn a_plugins_new_system_on_reload_keeps_its_resources() {
    let body = |extra: &str| {
        format!(
            "export function count(s: ResMut<Score>) {{ s.value += 1; }}\n{extra}export function enemyPlugin(a: typeof app) {{ a.insertResource(Score.of({{ value: 0 }})).addSystems(Update, count{more}); }}\napp.addPlugins(enemyPlugin);\n",
            more = if extra.is_empty() { "" } else { ", shout" }
        )
    };
    let (dir, mut app) = load("plugin", &src(&body("")));
    updates(&mut app, 3);
    write(&dir, "game.ts", &src(&body("export function shout(s: Res<Score>) { console.log(`score ${s.value}`); }\n")));
    reload(&mut app, false);
    updates(&mut app, 3);
    assert!(messages(&app).iter().any(|l| l.starts_with("score ") && l != "score 1"), "{:?}", messages(&app));
    assert!(read_res(&app, "game::Score").unwrap()["value"].as_u64().unwrap() >= 5);
}

#[test]
fn the_setup_closure_declares_with_values() {
    let (_, mut app) = load(
        "setup",
        &src(
            r#"export function spawn(c: Commands) { c.spawn(Health.of({ hp: 1 }), Enemy.of(), Transform); c.spawn(Health.of({ hp: 1 }), Transform); }
export const drift = system(() => {
  const q = query(Health, MutOf(Transform)).with(Enemy);
  const score = resMut(Score);
  return () => q.each((h, t) => { t.translation.y += h.hp; score.value += 1; });
});
app.insertResource(Score.of()).addSystems(Startup, spawn).addSystems(Update, drift);
"#,
        ),
    );
    updates(&mut app, 2);
    let mut ys: Vec<f32> = app.world_mut().query::<&Transform>().iter(app.world()).map(|t| t.translation.y).collect();
    ys.sort_by(f32::total_cmp);
    assert_eq!(ys, vec![0.0, 2.0]);
    assert_eq!(read_res(&app, "game::Score").unwrap()["value"], 2);
}

#[test]
fn a_declaring_call_outside_a_setup_fails_with_its_location() {
    let errors = load_errors("outside", &src("const q = query(Health);\n"));
    assert!(errors[0].message.contains("setup"), "{errors:?}");
    assert_eq!(errors[0].location.as_ref().unwrap().line, 10);
}

#[test]
fn the_ruled_example_loads_and_runs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/bevy-ts/test");
    // The type tests' `@ts-expect-error` lines are wrong on purpose; the runtime rejects them too.
    let runnable = |file: &str| {
        let src = std::fs::read_to_string(root.join(file)).unwrap();
        let mut out = Vec::new();
        let mut skip = false;
        for line in src.lines() {
            if skip {
                skip = false;
                out.push("");
                continue;
            }
            skip = line.trim_start().starts_with("// @ts-expect-error");
            out.push(line);
        }
        out.join("\n")
    };
    let dir = project(
        "ruled-example",
        &[
            ("systems.ts", &runnable("systems.ts")),
            ("fallback.ts", &runnable("fallback.ts")),
            ("game.ts", "import \"./systems\";\nimport \"./fallback\";\n"),
        ],
    );
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 5);
    let errors: Vec<String> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let systems = app.world().resource::<bevy_ts::systems::TsSystems>();
    let ids: Vec<String> = systems.iter().map(|s| s.decl.name.clone()).collect();
    for want in ["systems.ts#chase", "systems.ts#spawnWave", "systems.ts#onExplode", "fallback.ts#drift"] {
        assert!(ids.iter().any(|i| i == want), "{want} in {ids:?}");
    }
}
