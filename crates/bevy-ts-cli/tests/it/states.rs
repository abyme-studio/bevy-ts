//! ts-states: TS state machines are Bevy states, in a pool of 32 slots; Bevy drives their
//! transitions on a single-threaded `StateTransition`.

use crate::common;

use bevy::ecs::schedule::MultiThreadedExecutor;
use bevy::prelude::*;
use bevy::state::state::{StateTransition, StateTransitionEvent};
use bevy_ts::states::{TsState, TsStates};
use common::*;

const HEAD: &str = r#"import { app, OnEnter, OnExit, OnTransition, States, stateChanged, systems, Update, inState } from "bevy-ts";
import type { NextState, PreviousState, State } from "bevy-ts";
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

fn said(app: &mut App) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|m| !m.contains(' ')).collect()
}

/// `Game` goes Menu → Playing on the second frame, from a TS system.
const GAME: &str = r#"export class Game extends States("Menu", "Playing", "Paused") { static path = "game::Game" }
export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); }
export function enterPlaying() { console.log("enter"); }
export function exitMenu() { console.log("exit"); }
app.initState(Game.Menu).addSystems(Update, go).addSystems(OnEnter(Game.Playing), enterPlaying).addSystems(OnExit(Game.Menu), exitMenu);
"#;

fn slot(app: &App, path: &str) -> u8 {
    app.world().resource::<TsStates>().slot(path).unwrap_or_else(|| panic!("{path} has no slot"))
}

fn value(app: &App) -> &'static str {
    bevy_ts::states::name(app.world().resource::<State<TsState<0>>>().get().0)
}

#[test]
fn rust_reads_a_ts_state_and_its_transition_event() {
    let (_, mut app) = load("states-rust-reads", &src(GAME));
    assert_eq!(slot(&app, "game::Game"), 0);
    let mut cursor = app.world().resource::<Messages<StateTransitionEvent<TsState<0>>>>().get_cursor();
    updates(&mut app, 3);
    assert_eq!(value(&app), "Playing");
    let messages = app.world().resource::<Messages<StateTransitionEvent<TsState<0>>>>();
    let moves: Vec<(Option<&str>, Option<&str>)> =
        cursor.read(messages).map(|e| (e.exited.map(|s| bevy_ts::states::name(s.0)), e.entered.map(|s| bevy_ts::states::name(s.0)))).collect();
    assert!(moves.contains(&(Some("Menu"), Some("Playing"))), "{moves:?}");
    assert_eq!(said(&mut app), ["exit", "enter"]);
}

#[test]
fn a_rust_next_state_write_runs_ts_transition_systems() {
    let body = r#"export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function enterPlaying() { console.log("enter"); }
app.initState(Game.Menu).addSystems(OnEnter(Game.Playing), enterPlaying);
"#;
    let (_, mut app) = load("states-rust-writes", &src(body));
    app.update();
    app.world_mut().resource_mut::<NextState<TsState<0>>>().set(TsState(bevy_ts::states::sym("Playing")));
    app.update();
    assert_eq!(value(&app), "Playing");
    assert_eq!(said(&mut app), ["enter"]);
}

#[test]
fn reordering_values_on_reload_keeps_the_value_and_runs_no_transition() {
    let (dir, mut app) = load("states-reorder", &src(GAME));
    updates(&mut app, 3);
    let n = said(&mut app).len();
    write(&dir, "game.ts", &src(&GAME.replace(r#"States("Menu", "Playing", "Paused")"#, r#"States("Paused", "Playing", "Menu")"#)));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(value(&app), "Playing");
    assert_eq!(said(&mut app).len(), n, "no exit or enter ran");
}

#[test]
fn removing_the_current_value_returns_to_the_initial_one() {
    let (dir, mut app) = load("states-removed", &src(GAME));
    updates(&mut app, 3);
    // Playing is gone: the machine goes back to Menu, as a transition.
    let body = r#"export class Game extends States("Menu", "Paused") { static path = "game::Game" }
export function enterMenu() { console.log("menu"); }
app.initState(Game.Menu).addSystems(OnEnter(Game.Menu), enterMenu);
"#;
    write(&dir, "game.ts", &src(body));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(value(&app), "Menu");
    assert_eq!(said(&mut app).last().map(String::as_str), Some("menu"));
}

#[test]
fn on_transition_runs_on_exactly_its_transition_and_previous_state_reads_the_last_value() {
    let body = r#"export class Game extends States("Menu", "Playing", "Paused") { static path = "game::Game" }
export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); }
export function fromMenu() { console.log("fromMenu"); }
export function fromPaused() { console.log("fromPaused"); }
export function was(p: PreviousState<Game>) { if (p.previous) console.log(`was${p.previous.value}`); }
app.initState(Game.Menu).addSystems(Update, go, was)
  .addSystems(OnTransition({ exited: Game.Menu, entered: Game.Playing }), fromMenu)
  .addSystems(OnTransition({ exited: Game.Paused, entered: Game.Playing }), fromPaused);
"#;
    let (_, mut app) = load("states-on-transition", &src(body));
    updates(&mut app, 3);
    let m = said(&mut app);
    assert!(m.contains(&"fromMenu".to_string()) && !m.contains(&"fromPaused".to_string()), "{m:?}");
    assert!(m.contains(&"wasMenu".to_string()), "{m:?}");
}

#[test]
fn state_changed_is_true_on_the_frame_of_a_transition_only() {
    let body = r#"export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); }
export function hud() { console.log("hud"); }
app.initState(Game.Menu).addSystems(Update, go, systems(hud).runIf(stateChanged(Game)));
"#;
    let (_, mut app) = load("states-changed", &src(body));
    // Frame 1 enters Menu (a change), frame 2 enters Playing; later frames change nothing.
    updates(&mut app, 6);
    assert_eq!(said(&mut app), ["hud", "hud"]);
}

#[test]
fn a_33rd_state_machine_fails_the_load_naming_the_limit() {
    let mut body = String::new();
    for i in 0..33 {
        body.push_str(&format!("export class M{i} extends States(\"A\", \"B\") {{ static path = \"game::M{i}\" }}\napp.initState(M{i}.A);\n"));
    }
    let errors = load_errors("states-limit", &src(&body));
    assert!(errors.iter().any(|e| e.message.contains("at most 32 state machines") && e.message.contains("game::M32")), "{errors:?}");
}

#[test]
fn transitions_wait_while_the_game_is_paused() {
    let body = r#"export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function enterPlaying() { console.log("enter"); }
app.initState(Game.Menu).addSystems(OnEnter(Game.Playing), enterPlaying);
"#;
    let (_, mut app) = load("states-paused", &src(body));
    app.update();
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
    app.world_mut().resource_mut::<NextState<TsState<0>>>().set(TsState(bevy_ts::states::sym("Playing")));
    updates(&mut app, 2);
    assert_eq!(value(&app), "Menu", "no transition while paused");
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = false;
    app.update();
    assert_eq!(value(&app), "Playing");
    assert_eq!(said(&mut app), ["enter"]);
}

/// bevy-ts sets `StateTransition` to the single-threaded executor; Bevy offers no way to read it
/// back, so this checks the opt-out keeps TS states working.
#[test]
fn a_game_can_opt_state_transition_back_into_the_multi_threaded_executor() {
    let (_, mut app) = load("states-executor", &src(GAME));
    app.edit_schedule(StateTransition, |s| {
        s.set_executor(MultiThreadedExecutor::new());
    });
    updates(&mut app, 3);
    assert_eq!(value(&app), "Playing");
    assert_eq!(said(&mut app), ["exit", "enter"]);
}

#[test]
fn in_state_gates_on_the_bevy_state() {
    let body = r#"export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); }
export function play() { console.log("play"); }
app.initState(Game.Menu).addSystems(Update, go, systems(play).runIf(inState(Game.Playing)));
"#;
    let (_, mut app) = load("states-in-state", &src(body));
    updates(&mut app, 4);
    // Frame 1 sets Playing; frames 2 to 4 are in Playing.
    assert_eq!(said(&mut app), ["play", "play", "play"]);
}

const SUB: &str = r#"import { SubStates, ComputedStates } from "bevy-ts";
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export class Pause extends SubStates(Game.Playing, "Running", "Paused") { static path = "game::Pause" }
export class InGame extends ComputedStates(Game, (g) => { console.log("compute"); return g === Game.Playing ? InGame.Yes : undefined; }, "Yes") { static path = "game::InGame" }
"#;

#[test]
fn a_sub_state_exists_only_in_its_source_value_and_ignores_next_state_otherwise() {
    let body = r#"export function early(next: NextState<Pause>, g: State<Game>, p: State<Pause>) { if (g.current === Game.Menu) next.set(Pause.Paused); }
export function go(next: NextState<Game>, g: State<Game>, p: State<Pause>) { console.log(`pause${p.current ? p.current.value : "-"}`); if (g.current === Game.Menu) next.set(Game.Playing); }
app.initState(Game.Menu).addSystems(Update, early, go);
"#;
    let (_, mut app) = load("states-sub", &src(&format!("{SUB}{body}")));
    updates(&mut app, 3);
    let m: Vec<_> = said(&mut app).into_iter().filter(|m| m.starts_with("pause")).collect();
    // Frame 1: Menu, no Pause (its NextState ignored). Frame 2: Playing, Pause at its first value.
    assert_eq!(m[..2], ["pause-", "pauseRunning"], "{m:?}");
}

#[test]
fn leaving_a_parent_exits_the_sub_state_first() {
    let body = r#"export function step(ng: NextState<Game>, np: NextState<Pause>, g: State<Game>, p: State<Pause>) {
  if (g.current === Game.Menu) ng.set(Game.Playing);
  else if (p.current === Pause.Running) np.set(Pause.Paused);
  else if (p.current === Pause.Paused) ng.set(Game.Menu);
}
export function exitPaused() { console.log("exitPaused"); }
export function exitPlaying() { console.log("exitPlaying"); }
export function enterMenu() { console.log("enterMenu"); }
app.initState(Game.Menu).addSystems(Update, step)
  .addSystems(OnExit(Pause.Paused), exitPaused).addSystems(OnExit(Game.Playing), exitPlaying).addSystems(OnEnter(Game.Menu), enterMenu);
"#;
    let (_, mut app) = load("states-sub-order", &src(&format!("{SUB}{body}")));
    updates(&mut app, 6);
    let m: Vec<String> = said(&mut app).into_iter().filter(|l| l != "compute").collect();
    // The first enterMenu is the initial one.
    let tail: Vec<&str> = m.iter().skip_while(|l| *l == "enterMenu").map(String::as_str).take(3).collect();
    assert_eq!(tail, ["exitPaused", "exitPlaying", "enterMenu"], "{m:?}");
}

#[test]
fn a_computed_state_is_computed_only_when_a_source_transitions() {
    let body = r#"export function go(next: NextState<Game>, g: State<Game>, i: State<InGame>) { console.log(`in${i.current ? "Yes" : "-"}`); if (g.current === Game.Menu) next.set(Game.Playing); }
app.initState(Game.Menu).addSystems(Update, go);
"#;
    let (_, mut app) = load("states-computed", &src(&format!("{SUB}{body}")));
    updates(&mut app, 5);
    let m = said(&mut app);
    assert_eq!(m.iter().filter(|l| *l == "compute").count(), 2, "Menu once, Playing once: {m:?}");
    assert!(m.contains(&"in-".to_string()) && m.contains(&"inYes".to_string()), "{m:?}");
}

#[test]
fn a_sub_state_takes_no_init_state() {
    let errors = load_errors("states-sub-init", &src(&format!("{SUB}app.initState(Game.Menu).initState(Pause.Running);\n")));
    assert!(errors.iter().any(|e| e.message.contains("game::Pause") && e.message.contains("takes no initState")), "{errors:?}");
}

#[test]
fn a_ts_system_gates_on_a_rust_state() {
    let body = r#"import { EngineMode } from "bevy-ts/states";
export function play() { console.log("play"); }
export function edit() { console.log("edit"); }
app.addSystems(Update, systems(play).runIf(inState(EngineMode.Play)), systems(edit).runIf(inState(EngineMode.Edit)));
"#;
    let (_, mut app) = load("states-rust-state", &src(body));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["play", "play"]);
    app.world_mut().resource_mut::<NextState<bevy_ts::EngineMode>>().set(bevy_ts::EngineMode::Edit);
    updates(&mut app, 2);
    assert!(said(&mut app).ends_with(&["edit".to_string(), "edit".to_string()]));
}

#[test]
fn state_scoped_components_despawn_on_exit() {
    let body = r#"import { DespawnOnExit, Component } from "bevy-ts";
import type { Commands } from "bevy-ts";
export class Enemy extends Component { static path = "game::Enemy" }
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function spawn(c: Commands) { c.spawn(Enemy, DespawnOnExit.of(Game.Playing)); }
export function go(next: NextState<Game>, s: State<Game>) { if (s.current === Game.Menu) next.set(Game.Playing); else next.set(Game.Menu); }
app.initState(Game.Menu).addSystems(OnEnter(Game.Playing), spawn).addSystems(Update, go);
"#;
    let (_, mut app) = load("states-scoped", &src(body));
    let enemies = |app: &mut App| {
        let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Enemy").unwrap().0;
        let world = app.world_mut();
        QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).count()
    };
    // Frame 1: Menu → Playing queued. Frame 2: enters Playing, spawns. Frame 3: back to Menu.
    updates(&mut app, 2);
    assert_eq!(enemies(&mut app), 1);
    app.update();
    assert_eq!(enemies(&mut app), 0, "despawned on leaving Playing");
}

#[test]
fn set_reenters_the_current_value_and_set_if_neq_does_not() {
    let body = |call: &str| {
        format!(
            r#"export class Game extends States("Menu", "Playing") {{ static path = "game::Game" }}
export function again(next: NextState<Game>) {{ next.{call}(Game.Menu); }}
export function enterMenu() {{ console.log("enterMenu"); }}
app.initState(Game.Menu).addSystems(Update, again).addSystems(OnEnter(Game.Menu), enterMenu);
"#
        )
    };
    let (_, mut app) = load("states-set-if-neq", &src(&body("setIfNeq")));
    updates(&mut app, 4);
    assert_eq!(said(&mut app), ["enterMenu"], "only the initial enter");
    let (_, mut app) = load("states-set-same", &src(&body("set")));
    updates(&mut app, 4);
    assert!(said(&mut app).len() >= 3, "set re-enters Menu each frame");
}

#[test]
fn a_reload_that_makes_a_state_a_sub_state_moves_its_slot_and_derives_it_after_its_source() {
    // Gen 1: Pause is a plain state, declared before Game.
    let gen1 = r#"export class Pause extends States("Running", "Paused") { static path = "game::Pause" }
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
app.initState(Pause.Running).initState(Game.Menu);
"#;
    let (dir, mut app) = load("states-kind-change", &src(gen1));
    updates(&mut app, 2);
    let before = slot(&app, "game::Pause");
    let gen2 = r#"import { SubStates } from "bevy-ts";
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export class Pause extends SubStates(Game.Playing, "Running", "Paused") { static path = "game::Pause" }
export function go(next: NextState<Game>, g: State<Game>, p: State<Pause>) { console.log(`g${g.current.value}p${p.current ? p.current.value : "-"}`); if (g.current === Game.Menu) next.set(Game.Playing); }
app.initState(Game.Menu).addSystems(Update, go);
"#;
    write(&dir, "game.ts", &src(gen2));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 3);
    assert_ne!(slot(&app, "game::Pause"), before, "a slot keeps its depth: Pause moved");
    let m = said(&mut app);
    assert!(m.contains(&"gMenup-".to_string()) && m.contains(&"gPlayingpRunning".to_string()), "{m:?}");
    assert!(!m.contains(&"gPlayingp-".to_string()), "the sub state follows its source in the same frame: {m:?}");
}

#[test]
fn a_machine_given_its_first_value_while_paused_runs_its_ts_on_enter_when_the_game_resumes() {
    let (dir, mut app) = load("states-first-paused", &src("app;\n"));
    app.update();
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
    let body = r#"export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export function enterMenu() { console.log("enterMenu"); }
app.initState(Game.Menu).addSystems(OnEnter(Game.Menu), enterMenu);
"#;
    write(&dir, "game.ts", &src(body));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert!(said(&mut app).is_empty(), "no OnEnter while paused");
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = false;
    app.update();
    assert_eq!(said(&mut app), ["enterMenu"]);
}

#[test]
fn removing_a_rust_state_scoped_component_removes_it() {
    let body = r#"import { DespawnOnExit, Component } from "bevy-ts";
import type { Commands, Entity, Query } from "bevy-ts";
import { EngineMode } from "bevy-ts/states";
export class Enemy extends Component { static path = "game::Enemy" }
let frame = 0;
export function step(c: Commands, q: Query<[Entity, Enemy]>) {
  frame++;
  if (frame === 1) c.spawn(Enemy, DespawnOnExit.of(EngineMode.Play));
  if (frame === 2) q.each((e) => c.entity(e).remove(DespawnOnExit));
}
app.addSystems(Update, step);
"#;
    let (_, mut app) = load("states-rust-scoped", &src(body));
    updates(&mut app, 2);
    let scoped = app.world_mut().query::<&bevy::state::state_scoped::DespawnOnExit<bevy_ts::EngineMode>>().iter(app.world()).count();
    assert_eq!(scoped, 0, "the component is gone");
    app.world_mut().resource_mut::<NextState<bevy_ts::EngineMode>>().set(bevy_ts::EngineMode::Edit);
    updates(&mut app, 2);
    let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Enemy").unwrap().0;
    let world = app.world_mut();
    assert_eq!(QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).count(), 1, "leaving Play keeps the enemy");
}

#[test]
fn a_reload_rederives_sub_and_computed_states() {
    let gen = |values: &str, tag: &str| {
        format!(
            r#"import {{ SubStates, ComputedStates }} from "bevy-ts";
export class Game extends States("Menu", "Playing") {{ static path = "game::Game" }}
export class Pause extends SubStates(Game.Playing, {values}) {{ static path = "game::Pause" }}
export class InGame extends ComputedStates(Game, (g) => {{ console.log("{tag}"); return g === Game.Playing ? InGame.Yes : undefined; }}, "Yes") {{ static path = "game::InGame" }}
export function step(ng: NextState<Game>, np: NextState<Pause>, g: State<Game>, p: State<Pause>) {{
  console.log(`p${{p.current ? p.current.value : "-"}}`);
  if (g.current === Game.Menu) ng.set(Game.Playing);
  else if (p.current && p.current.value === "Running" && (Pause as any).Paused) np.set((Pause as any).Paused);
}}
app.initState(Game.Menu).addSystems(Update, step);
"#
        )
    };
    let (dir, mut app) = load("states-rederive", &src(&gen(r#""Running", "Paused""#, "compute1")));
    updates(&mut app, 4);
    assert_eq!(said(&mut app).iter().rev().find(|l| l.starts_with('p')).map(String::as_str), Some("pPaused"));
    // Paused is gone, and the compute function changed.
    write(&dir, "game.ts", &src(&gen(r#""Running""#, "compute2")));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    let m = said(&mut app);
    assert_eq!(m.iter().rev().find(|l| l.starts_with('p')).map(String::as_str), Some("pRunning"), "{m:?}");
    assert_eq!(m.iter().filter(|l| *l == "compute2").count(), 1, "recomputed once: {m:?}");
}
