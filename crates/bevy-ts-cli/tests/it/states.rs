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
