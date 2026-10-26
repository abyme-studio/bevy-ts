//! ts-conditions: Bevy's run conditions and combinators evaluated in Rust, their state across
//! reloads, collective and distributive `runIf`, and conditions a crate exposes.

use crate::common;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::time::common_conditions::on_timer;
use bevy_ts_native::BevyTsNativeExt;
use common::*;
use serde_json::json;
use std::time::Duration;

const HEAD: &str = r#"import { Message, onMessage, PostUpdate, anyComponentRemoved, anyMatchFilter, anyWithComponent, app, Changed, Component, conditionChanged, conditionChangedTo, inState, nativeCondition, not, resourceAdded, resourceChanged, resourceChangedOrRemoved, resourceEquals, resourceExists, resourceExistsAndChanged, resourceExistsAndEquals, resourceRemoved, runOnce, Resource, States, systems, u32, Update, With, Without } from "bevy-ts";
import type { Commands, MessageReader, MessageWriter, NextState, ResMut } from "bevy-ts";
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
export class Boss extends Resource { static path = "game::Boss"; declare hp: u32 }
export class Enemy extends Component { static path = "game::Enemy" }
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export class Ping extends Message { static path = "game::Ping"; declare n: u32 }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

fn said(app: &mut App) -> Vec<String> {
    // console output reaches the log when the runtime is pumped, in the next frame's `First`.
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|m| !m.contains(' ')).collect()
}

/// Writes a TS resource from Rust, as a Rust system would: its change tick moves.
fn write_res(app: &mut App, name: &str, values: serde_json::Value) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), name).unwrap();
    let bytes = bevy_ts::plugin_api::encode_component(app.world(), name, values.as_object().unwrap()).unwrap();
    let e = app.world().resource_entities().get(id).unwrap();
    let mut em = app.world_mut().entity_mut(e);
    unsafe { bevy_ts::commands::insert_owned(&mut em, id, bytes) };
}

fn remove_res(app: &mut App, name: &str) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), name).unwrap();
    app.world_mut().remove_resource_by_id(id);
}

#[test]
fn resource_changed_runs_on_the_frame_rust_writes_it_and_costs_no_call_otherwise() {
    let (_, mut app) = load("cond-changed", &src("export function redraw() { console.log('redraw'); }\napp.insertResource(Score.of({ value: 0 })).addSystems(Update, systems(redraw).runIf(resourceChanged(Score)));\n"));
    updates(&mut app, 2);
    let n = said(&mut app).len();
    write_res(&mut app, "game::Score", json!({ "value": 1 }));
    app.update();
    assert_eq!(said(&mut app)[n..], ["redraw"]);
    let before = calls(&app);
    updates(&mut app, 2);
    assert_eq!(said(&mut app).len(), n + 1);
    assert_eq!(calls(&app), before, "skipped frames make no call into JS");
}

#[test]
fn resource_equals_runs_from_the_frame_the_value_matches() {
    let body = "export function win() { console.log('win'); }\nexport function bump(s: ResMut<Score>) { s.value += 1; }\napp.insertResource(Score.of({ value: 8 })).addSystems(Update, bump, systems(win).runIf(resourceEquals(Score.of({ value: 10 }))));\n";
    let (_, mut app) = load("cond-equals", &src(body));
    updates(&mut app, 4);
    // 9, 10, 11, 12: equal on the second frame only.
    assert_eq!(said(&mut app), ["win"]);
}

#[test]
fn resource_equals_compares_a_string_by_value() {
    let body = r#"export class Title extends Resource { static path = "game::Title"; text: string = "a" }
export function win() { console.log('win'); }
export function step(t: ResMut<Title>) { t.text = t.text === "a" ? "door" : "z"; }
app.insertResource(Title.of({ text: "a" })).addSystems(Update, step, systems(win).after(step).runIf(resourceEquals(Title.of({ text: "door" }))));
"#;
    let (_, mut app) = load("cond-equals-string", &src(body));
    updates(&mut app, 3);
    // "door", "z", "z": equal on the first frame only, though each string is a new allocation.
    assert_eq!(said(&mut app), ["win"]);
}

#[test]
fn any_component_removed_runs_once_per_removal() {
    let (dir, mut app) = load("cond-removed", &src(""));
    let es: Vec<Entity> = (0..3).map(|_| spawn_ts(&mut app, "game::Enemy", json!({}), ())).collect();
    write(&dir, "game.ts", &src("export function recount() { console.log('recount'); }\napp.addSystems(Update, systems(recount).runIf(anyComponentRemoved(Enemy)));\n"));
    reload(&mut app, false);
    updates(&mut app, 2);
    assert!(said(&mut app).is_empty());
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Enemy").unwrap();
    app.world_mut().entity_mut(es[1]).remove_by_id(id);
    updates(&mut app, 3);
    assert_eq!(said(&mut app), ["recount"]);
}

#[test]
fn short_circuit_skips_b_and_eager_spends_it() {
    let body = r#"export function a() { console.log("a"); }
export function b() { console.log("b"); }
export function play(n: NextState<Game>, s: ResMut<Score>) { if (++s.value === 3) n.set(Game.Playing); }
app.initState(Game.Menu).insertResource(Score.of({ value: 0 })).addSystems(Update, play,
  systems(a).runIf(inState(Game.Playing).andThen(runOnce())),
  systems(b).runIf(inState(Game.Playing).andEager(runOnce())));
"#;
    let (_, mut app) = load("cond-combinators", &src(body));
    updates(&mut app, 6);
    assert_eq!(said(&mut app), ["a"]);
}

#[test]
fn run_once_survives_a_reload_and_resets_on_restart() {
    let body = "export function once() { console.log('once'); }\napp.addSystems(Update, systems(once).runIf(runOnce()));\n";
    let (dir, mut app) = load("cond-runonce", &src(body));
    updates(&mut app, 2);
    write(&dir, "game.ts", &src(&format!("{body}// edited\n")));
    reload(&mut app, false);
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["once"]);
    reload(&mut app, true);
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["once", "once"]);
}

#[test]
fn collective_run_if_is_one_evaluation_and_distributive_is_one_each() {
    let body = r#"export function a() { console.log("a"); }
export function b() { console.log("b"); }
export function c() { console.log("c"); }
export function d() { console.log("d"); }
app.addSystems(Update, systems(a, b).runIf(runOnce()), systems(c, d).distributiveRunIf(runOnce()));
"#;
    let (_, mut app) = load("cond-collective", &src(body));
    updates(&mut app, 3);
    assert_eq!(said(&mut app), ["a", "b", "c", "d"]);
}

#[test]
fn a_groups_condition_holds_for_the_whole_stage_run() {
    let body = r#"export function a(s: ResMut<Score>) { console.log("a"); s.value += 1; }
export function b() { console.log("b"); }
export function c(s: ResMut<Score>) { console.log("c"); s.value += 1; }
export function d() { console.log("d"); }
app.insertResource(Score.of({ value: 0 })).insertResource(Boss.of({ hp: 0 }))
  .addSystems(Update, systems(a, b).runIf(resourceEquals(Score.of({ value: 0 }))))
  .addSystems(PostUpdate, systems(c, d).distributiveRunIf(resourceEquals(Score.of({ value: 1 }))));
"#;
    let (_, mut app) = load("cond-grouponce", &src(body));
    updates(&mut app, 2);
    // a flips the group's condition; b still runs in that stage run. c flips d's own instance.
    assert_eq!(said(&mut app), ["a", "b", "c"]);
}

#[test]
fn every_rust_condition_skips_with_no_call_into_js() {
    let conditions = [
        "inState(Game.Playing)",
        "resourceExists(Boss)",
        "resourceAdded(Boss)",
        "resourceChanged(Boss)",
        "resourceExistsAndChanged(Boss)",
        "resourceRemoved(Boss)",
        "resourceChangedOrRemoved(Boss)",
        "resourceEquals(Score.of({ value: 99 }))",
        "resourceExistsAndEquals(Boss.of({ hp: 1 }))",
        "anyWithComponent(Enemy)",
        "anyComponentRemoved(Enemy)",
        "anyMatchFilter(With(Enemy), Without(Enemy))",
        "anyMatchFilter(Changed(Enemy))",
        "conditionChanged(inState(Game.Playing))",
        "conditionChangedTo(inState(Game.Playing), true)",
        "not(resourceExists(Score))",
        "runOnce().andThen(resourceExists(Boss))",
        "resourceExists(Boss).orElse(inState(Game.Playing))",
        "resourceExists(Score).nandThen(resourceExists(Score))",
        "resourceExists(Score).norElse(runOnce())",
        "runOnce().xor(runOnce())",
        "resourceExists(Score).xnor(inState(Game.Playing))",
    ];
    for (i, c) in conditions.iter().enumerate() {
        let body = format!("export function gated() {{ console.log('ran'); }}\napp.initState(Game.Menu).insertResource(Score.of({{ value: 0 }})).addSystems(Update, systems(gated).runIf({c}));\n");
        let (_, mut app) = load(&format!("cond-skip{i}"), &src(&body));
        // The first frame spends any runOnce.
        updates(&mut app, 2);
        let before = calls(&app);
        updates(&mut app, 3);
        assert_eq!(calls(&app), before, "{c} made a call into JS");
        let ran = said(&mut app).iter().filter(|m| *m == "ran").count();
        assert!(ran <= 1, "{c} ran {ran} times");
    }
}

fn on_timer_factory(args: &serde_json::Value) -> Result<bevy::ecs::schedule::BoxedCondition, String> {
    let secs = args["secs"].as_f64().ok_or_else(|| format!("on-timer takes {{ secs: number }}, got {args}"))?;
    Ok(Box::new(IntoSystem::into_system(on_timer(Duration::from_secs_f64(secs)))))
}

fn timer_app(name: &str, body: &str) -> (App, bevy_ts::generation::ReloadReport) {
    let dir = project(name, &[("game.ts", &src(body))]);
    let mut app = app(&dir.join("game.ts"));
    app.ts_condition("time:on-timer", on_timer_factory);
    let r = reload(&mut app, false);
    (app, r)
}

#[test]
fn a_ports_condition_runs_in_rust() {
    let (mut app, r) = timer_app("cond-ontimer", "export function spawn() { console.log('spawn'); }\napp.addSystems(Update, systems(spawn).runIf(nativeCondition(\"time:on-timer\", { secs: 1 })));\n");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    // Virtual time clamps a frame to 250 ms: nine frames are 2.25 s or so, two firings.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(250)));
    app.update();
    let before = calls(&app);
    updates(&mut app, 9);
    assert_eq!(said(&mut app), ["spawn", "spawn"]);
    assert_eq!(calls(&app) - before, 2, "only the runs call into JS");
}

#[test]
fn a_factory_error_fails_the_load_with_the_call() {
    let (_, r) = timer_app("cond-ontimerbad", "export function spawn() {}\napp.addSystems(Update, systems(spawn).runIf(nativeCondition(\"time:on-timer\", { secs: \"two\" })));\n");
    let e = r.errors.iter().find(|e| e.message.contains("time:on-timer")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("on-timer takes"), "{e:?}");
    let at = e.location.as_ref().unwrap();
    assert!(at.file.ends_with("game.ts") && at.line as usize == HEAD.lines().count() + 2, "{at:?}");
}

#[test]
fn an_unknown_native_condition_fails_the_load() {
    let errors = load_errors("cond-nocondition", &src("export function spawn() {}\napp.addSystems(Update, systems(spawn).runIf(nativeCondition(\"time:nope\")));\n"));
    assert!(errors.iter().any(|e| e.message.contains("time:nope")), "{errors:?}");
}

#[test]
fn removed_and_added_forms_follow_the_resource() {
    let body = r#"export function added() { console.log("added"); }
export function removed() { console.log("removed"); }
export function either() { console.log("either"); }
app.insertResource(Boss.of({ hp: 1 })).addSystems(Update, systems(added).runIf(resourceAdded(Boss)), systems(removed).runIf(resourceRemoved(Boss)), systems(either).runIf(resourceChangedOrRemoved(Boss)));
"#;
    let (_, mut app) = load("cond-removedforms", &src(body));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["added", "either"]);
    remove_res(&mut app, "game::Boss");
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["added", "either", "removed", "either"]);
}

#[test]
fn a_groups_on_message_has_its_own_cursor() {
    let body = r#"export function send(w: MessageWriter<Ping>, s: ResMut<Score>) { if (s.value++ === 0) w.write(Ping.of({ n: 1 })); }
export function a(r: MessageReader<Ping>) { console.log(`a${r.len}`); }
export function b() { console.log("b"); }
app.addMessage(Ping).insertResource(Score.of({ value: 0 })).addSystems(Update, send, systems(a, b).runIf(onMessage(Ping)));
"#;
    let (_, mut app) = load("cond-grouponmessage", &src(body));
    updates(&mut app, 4);
    // One message: the group runs in the one stage run after it was written, not while it stays buffered.
    assert_eq!(said(&mut app), ["a1", "b"]);
}
