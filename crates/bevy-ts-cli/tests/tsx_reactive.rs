//! TSX reactivity end to end in V8: bindings that read `ref.get(C)` with no hooks, Rust's
//! per-frame change-tick check, the one host call per frame, UI intrinsics, `<Show>`, `<For>`
//! and signals set by systems.

mod common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::plugin_api;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;
use std::path::{Path, PathBuf};

const GAME: &str = r##"import { app, Component, f32, u8, ref, signal, px, Show, For, Update, type Query, type Mut, type With } from "bevy-ts";

export class Health extends Component { static path = "game::Health"; hp: f32 = 100; max: f32 = 100 }
export class Enemy extends Component { static path = "game::Enemy"; declare tag: u8 }
export class Alarm extends Component { static path = "game::Alarm"; declare on: u8 }

const player = ref();
export const [open, setOpen] = signal(false);
export const [items, setItems] = signal(["a", "b", "c"]);

export function hurtEnemies(q: Query<[Mut<Health>], With<Enemy>>) {
  q.each((h) => {
    h.hp -= 1;
  });
}

export function toggle(q: Query<[Alarm]>) {
  q.each((a) => {
    if (a.on === 1 && !open()) setOpen(true);
    if (a.on === 2) setItems(["a", "c"]);
  });
}

app.addSystems(Update, hurtEnemies, toggle);

app.scene("arena", () => (
  <>
    <entity key="player" ref={player}>
      <Health initialHp={100} max={100} />
    </entity>
    <entity key="e1"><Health initialHp={50} /><Enemy /></entity>
    <entity key="e2"><Health initialHp={50} /><Enemy /></entity>
    <node key="hud" style={{ position: "absolute", top: px(12), left: px(12), padding: px(8) }} background="#0008">
      <text key="hp" size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
      <text key="static">Inventory</text>
      <text key="warn" color={() => (player.get(Health).hp < 20 ? "#ff3333" : "#ffffff")}>!</text>
      <Show key="menu" when={open} fallback={<text key="closed">closed</text>}>
        <For each={items} key={(i: string) => i}>
          {(i: string) => <text key="row">{i}</text>}
        </For>
      </Show>
    </node>
  </>
));
"##;

fn setup(name: &str, src: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    (dir, app)
}

fn node(app: &mut App, path: &[&str]) -> Option<Entity> {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e)
}

fn text(app: &mut App, path: &[&str]) -> String {
    let e = node(app, path).unwrap_or_else(|| panic!("no node {path:?}"));
    app.world().get::<Text>(e).map(|t| t.0.clone()).unwrap_or_default()
}

fn calls(app: &App) -> u64 {
    app.world().resource::<TsxTree>().reactive.calls
}

fn set_hp(app: &mut App, e: Entity, v: f32) {
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let mut m = app.world_mut().get_mut_by_id(e, id).unwrap();
    // SAFETY: Health is two f32s.
    unsafe { std::ptr::copy_nonoverlapping(v.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
}

fn set_alarm(app: &mut App, v: u8) {
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Alarm").unwrap();
    let e = app.world_mut().spawn_empty().id();
    // SAFETY: Alarm is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(e), id, &[v]) };
}

fn errors(app: &App) -> Vec<String> {
    logs(app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect()
}

#[test]
fn the_hud_shows_the_players_health_on_mount() {
    let (_dir, mut app) = setup("rx-mount", GAME);
    assert_eq!(text(&mut app, &["hud", "hp"]), "HP 100 / 100", "{:?}", errors(&app));
    assert_eq!(text(&mut app, &["hud", "static"]), "Inventory");
    assert_eq!(text(&mut app, &["hud", "menu", "else", "closed"]), "closed");
    let warn = node(&mut app, &["hud", "warn"]).unwrap();
    assert_eq!(app.world().get::<TextColor>(warn).unwrap().0, Color::from(Srgba::hex("#ffffff").unwrap()));
    let hud = node(&mut app, &["hud"]).unwrap();
    let n = app.world().get::<Node>(hud).unwrap();
    assert_eq!(n.padding, UiRect::all(Val::Px(8.0)));
    assert_eq!(n.position_type, PositionType::Absolute);
    assert_eq!(app.world().get::<BackgroundColor>(hud).unwrap().0, Color::srgba(0.0, 0.0, 0.0, 0x88 as f32 / 255.0));
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
}

#[test]
fn enemies_taking_damage_cost_no_host_call() {
    let (_dir, mut app) = setup("rx-quiet", GAME);
    let before = calls(&app);
    updates(&mut app, 60);
    assert_eq!(calls(&app), before, "only enemies' Health changed: no call into JS for the HUD");
    assert_eq!(text(&mut app, &["hud", "hp"]), "HP 100 / 100");
}

#[test]
fn a_player_hit_writes_exactly_one_text_in_one_call() {
    let (_dir, mut app) = setup("rx-hit", GAME);
    updates(&mut app, 2);
    let player = node(&mut app, &["player"]).unwrap();
    let text_id = app.world_mut().register_component::<Text>();
    let texts: Vec<Entity> = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Text>>().iter(world).collect()
    };
    let ticks = |app: &App| texts.iter().map(|e| plugin_api::change_ticks(app.world(), *e, text_id).unwrap().changed).collect::<Vec<_>>();
    let before_ticks = ticks(&app);
    let before = calls(&app);
    set_hp(&mut app, player, 63.2);
    app.update();
    assert_eq!(calls(&app), before + 1, "one host call");
    assert_eq!(text(&mut app, &["hud", "hp"]), "HP 64 / 100");
    let after = ticks(&app);
    let moved = before_ticks.iter().zip(&after).filter(|(a, b)| a != b).count();
    assert_eq!(moved, 1, "exactly one Text written");
    // The colour binding read the same pair and did not change: TextColor untouched.
    set_hp(&mut app, player, 10.0);
    app.update();
    let warn = node(&mut app, &["hud", "warn"]).unwrap();
    assert_eq!(app.world().get::<TextColor>(warn).unwrap().0, Color::from(Srgba::hex("#ff3333").unwrap()));
}

#[test]
fn a_signal_a_system_sets_mounts_show_and_for_updates_by_key() {
    let (_dir, mut app) = setup("rx-show", GAME);
    set_alarm(&mut app, 1);
    updates(&mut app, 2);
    assert!(node(&mut app, &["hud", "menu", "else", "closed"]).is_none(), "fallback unmounted: {:?}", errors(&app));
    let rows: Vec<String> = ["a", "b", "c"].iter().map(|k| text(&mut app, &["hud", "menu", "then", "For#0", k, "row"])).collect();
    assert_eq!(rows, ["a", "b", "c"]);
    let a = node(&mut app, &["hud", "menu", "then", "For#0", "a", "row"]).unwrap();
    let c = node(&mut app, &["hud", "menu", "then", "For#0", "c", "row"]).unwrap();
    set_alarm(&mut app, 2);
    updates(&mut app, 2);
    assert!(node(&mut app, &["hud", "menu", "then", "For#0", "b", "row"]).is_none());
    assert_eq!(node(&mut app, &["hud", "menu", "then", "For#0", "a", "row"]), Some(a), "kept items keep their entities");
    assert_eq!(node(&mut app, &["hud", "menu", "then", "For#0", "c", "row"]), Some(c));
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
}

#[test]
fn a_reload_rebuilds_bindings_and_writes_nothing_that_did_not_change() {
    let (dir, mut app) = setup("rx-reload", GAME);
    updates(&mut app, 2);
    let hp = node(&mut app, &["hud", "hp"]).unwrap();
    let text_id = app.world_mut().register_component::<Text>();
    let tick = plugin_api::change_ticks(app.world(), hp, text_id).unwrap().changed;
    let r = save(&mut app, &dir, &GAME.replace("Inventory", "Bag"));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    assert_eq!(text(&mut app, &["hud", "static"]), "Bag");
    assert_eq!(node(&mut app, &["hud", "hp"]), Some(hp));
    assert_eq!(plugin_api::change_ticks(app.world(), hp, text_id).unwrap().changed, tick, "same bound value: not written");
    // The new generation's binding is live.
    let player = node(&mut app, &["player"]).unwrap();
    set_hp(&mut app, player, 5.0);
    app.update();
    assert_eq!(text(&mut app, &["hud", "hp"]), "HP 5 / 100");
}

#[test]
fn a_bound_component_field_follows_its_binding() {
    let src = r##"import { app, Component, f32, signal, Time, Update, type Local, type Query, type Res, type u32 } from "bevy-ts";
export class Light extends Component { static path = "game::Light"; declare intensity: f32 }
export const [alarm, setAlarm] = signal(false);
export function flip(q: Query<[Light]>, frames: Local<{ n: u32 }>) { frames.n += 1; if (frames.n === 5) setAlarm(true); } app.addSystems(Update, flip);
app.scene("s", () => <entity key="l"><Light intensity={() => (alarm() ? 8 : 2)} /></entity>);
"##;
    let (_dir, mut app) = setup("rx-field", src);
    let l = node(&mut app, &["l"]).unwrap();
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Light").unwrap();
    let read = |app: &App| {
        let p = app.world().get_by_id(l, id).unwrap();
        // SAFETY: one f32.
        f32::from_le_bytes(unsafe { std::slice::from_raw_parts(p.as_ptr(), 4) }.try_into().unwrap())
    };
    assert_eq!(read(&app), 2.0);
    updates(&mut app, 8);
    assert_eq!(read(&app), 8.0, "{:?}", errors(&app));
    let n = app.world().get::<TsxNode>(l).unwrap();
    assert_eq!(n.components["game::Light"].fields["intensity"].provenance, bevy_ts_tsx::Provenance::Bound);
}

fn save(app: &mut App, dir: &Path, src: &str) -> bevy_ts::generation::ReloadReport {
    write(dir, "game.tsx", src);
    reload(app, false)
}

#[test]
fn slot_content_keeps_its_entities_across_a_reload_and_goes_with_its_slot() {
    let (dir, mut app) = setup("rx-slot-reload", GAME);
    set_alarm(&mut app, 1);
    updates(&mut app, 2);
    let a = node(&mut app, &["hud", "menu", "then", "For#0", "a", "row"]).expect("row a");
    // The new generation starts with `open` false again, but the alarm is still on, so the
    // system opens it in the first frame: the rows mount at the same key paths.
    let r = save(&mut app, &dir, &GAME.replace("Inventory", "Bag"));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(node(&mut app, &["hud", "menu", "then", "For#0", "a", "row"]), Some(a), "{:?}", errors(&app));
    // A save without the Show drops its content.
    let without = GAME.replace(
        "<Show key=\"menu\" when={open} fallback={<text key=\"closed\">closed</text>}>",
        "<entity key=\"nomenu\">",
    )
    .replace("</Show>", "</entity>")
    .replace("<For each={items} key={(i: string) => i}>\n          {(i: string) => <text key=\"row\">{i}</text>}\n        </For>", "");
    let r = save(&mut app, &dir, &without);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    assert!(app.world().get_entity(a).is_err(), "slot content went with its slot");
}

#[test]
fn a_forgotten_arrow_warns_with_its_location() {
    let src = r##"import { app, signal } from "bevy-ts";
const [label] = signal("hi");
app.scene("s", () => <text key="t">{label()}</text>);
"##;
    let (_dir, mut app) = setup("rx-arrow", src);
    assert_eq!(text(&mut app, &["t"]), "hi", "the value at mount");
    let w = logs(&app).into_iter().find(|e| e.message.contains("read outside a binding")).expect("warned");
    assert!(w.message.contains("game.tsx:3"), "{}", w.message);
}

#[test]
fn a_system_writing_a_bound_component_is_named_with_the_site() {
    let src = r##"import { app, Component, f32, Update, type Query, type Mut } from "bevy-ts";
export class Light extends Component { static path = "game::Light"; declare intensity: f32 }
export function dim(q: Query<[Mut<Light>]>) {} app.addSystems(Update, dim);
app.scene("s", () => (
  <entity key="l">
    <Light intensity={() => 3} />
  </entity>
));
"##;
    let (_dir, app) = setup("rx-two-writers", src);
    let w = logs(&app).into_iter().find(|e| e.message.contains("one writer")).expect("warned");
    assert!(w.message.contains("system game.tsx#dim") && w.message.contains("intensity"), "{}", w.message);
    assert_eq!(w.location.as_ref().map(|l| l.line), Some(6));
}

fn click(app: &mut App, e: Entity, x: f32) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, PointerClick};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    let pointer = Pointer::new(PointerId::Mouse, Location { target: NormalizedRenderTarget::None { width: 1, height: 1 }, position: Vec2::new(x, 1.0) });
    let hit = HitData::new(Entity::PLACEHOLDER, 0.0, None, None);
    app.world_mut().trigger(PointerClick { entity: e, pointer, button: PointerButton::Primary, hit, duration: std::time::Duration::ZERO, count: 1 });
}

const BUTTONS: &str = r##"import { app, Component, f32, signal } from "bevy-ts";
export class Health extends Component { static path = "game::Health"; declare hp: f32 }
const [log, setLog] = signal("");
const Enemy = () => <entity name="enemy"><Health initialHp={10} /></entity>;
const Button = ({ label }: { label: string }) => (
  <button onPointerClick={({ entity, position, button }) => setLog((l) => `${l}${label}:${button}@${position[0]} `)}>
    <text>{label}</text>
  </button>
);
app.scene("ui", () => (
  <node key="bar">
    <Button key="a" label="A" />
    <Button key="b" label="B" />
    <Button key="c" label="C" />
    <button key="spawn" onPointerClick={({ commands, frame }) => commands.mount(<Enemy key={`e${frame}`} />)}>
      <text>Spawn</text>
    </button>
    <text key="log">{() => log()}</text>
    <entity key="inner-parent" onPointerClick={() => setLog((l) => `${l}parent `)}>
      <entity key="child" />
    </entity>
  </node>
));
"##;

#[test]
fn three_clicks_in_one_frame_run_their_handlers_in_order_in_one_call() {
    let (dir, mut app) = setup("rx-clicks", BUTTONS);
    updates(&mut app, 2);
    let [a, b, c] = ["a", "b", "c"].map(|k| node(&mut app, &["bar", k]).unwrap());
    let before = calls(&app);
    click(&mut app, b, 2.0);
    click(&mut app, a, 1.0);
    click(&mut app, c, 3.0);
    app.update();
    assert_eq!(calls(&app), before + 1, "one host call for three clicks");
    assert_eq!(text(&mut app, &["bar", "log"]), "B:primary@2 A:primary@1 C:primary@3 ", "{:?}", errors(&app));

    // Events do not bubble: a click on the child does not run its parent's handler.
    let child = node(&mut app, &["bar", "inner-parent", "child"]).unwrap();
    click(&mut app, child, 9.0);
    app.update();
    assert!(!text(&mut app, &["bar", "log"]).contains("parent"));

    // A handler mounts an enemy through the tsx op stream.
    let spawn = node(&mut app, &["bar", "spawn"]).unwrap();
    click(&mut app, spawn, 0.0);
    app.update();
    let mounted = {
        let world = app.world_mut();
        world.query::<&TsxNode>().iter(world).filter(|n| n.root == "$runtime").count()
    };
    assert_eq!(mounted, 1, "{:?}", errors(&app));

    // Editing a handler: the next click runs the new body, the button entity is unchanged.
    let r = save(&mut app, &dir, &BUTTONS.replace("${label}:${button}", "new-${label}"));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    assert_eq!(node(&mut app, &["bar", "a"]), Some(a));
    click(&mut app, a, 1.0);
    app.update();
    assert_eq!(text(&mut app, &["bar", "log"]), "new-A@1 ");
}

#[test]
fn a_binding_reads_a_resource_with_res_and_follows_it() {
    let src = r##"import { app, Resource, u32, res } from "bevy-ts";
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
app.insertResource(Score.of({ value: 7 }));
app.scene("hud", () => <text key="score" color={() => (res(Score).value > 10 ? "#ffdd00" : "#ffffff")}>{() => `Score ${res(Score).value}`}</text>);
"##;
    let (_dir, mut app) = setup("rx-res", src);
    assert_eq!(text(&mut app, &["score"]), "Score 7", "{:?}", errors(&app));
    let before = calls(&app);
    updates(&mut app, 5);
    assert_eq!(calls(&app), before, "a still resource costs no call");
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Score").unwrap();
    let e = app.world().resource_entities().get(id).expect("the resource's entity");
    let mut m = app.world_mut().get_mut_by_id(e, id).unwrap();
    // SAFETY: Score is one u32.
    unsafe { std::ptr::copy_nonoverlapping(42u32.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
    app.update();
    assert_eq!(calls(&app), before + 1);
    assert_eq!(text(&mut app, &["score"]), "Score 42");
    let t = node(&mut app, &["score"]).unwrap();
    assert_eq!(app.world().get::<TextColor>(t).unwrap().0, Color::from(Srgba::hex("#ffdd00").unwrap()));
}

const BOUND_IN_SLOTS: &str = r##"import { app, Component, u8, signal, Show, For, Update, type Query } from "bevy-ts";
export class Alarm extends Component { static path = "game::Alarm"; declare on: u8 }
const [s, setS] = signal<string | null>("x");
const [later, setLater] = signal(false);
const [clicked, setClicked] = signal(false);
const [list, setList] = signal(["a"]);
// Alarms pile up: the highest one says how far the test has got.
export function step(q: Query<[Alarm]>) {
  let top = 0;
  q.each((a) => {
    if (a.on > top) top = a.on;
  });
  if (top >= 1) setLater(top < 4);
  if (top >= 2) setS("y");
  if (top === 3) setList(["a", "b"]);
}
app.addSystems(Update, step);
app.scene("slots", () => (
  <node key="root">
    <Show key="pre" when={() => s() !== null}>
      <text key="bound">{() => `show ${s()}`}</text>
      <text key="static">static in show</text>
    </Show>
    <Show key="else" when={() => s() === null} fallback={<text key="bound">{() => `fallback ${s()}`}</text>}>
      <text key="never">never</text>
    </Show>
    <For each={list} key={(i: string) => i}>
      {(i: string) => <text key="row">{() => `${i} ${s()}`}</text>}
    </For>
    <Show key="outer" when={() => true}>
      <Show key="inner" when={() => true}>
        <text key="bound">{() => `nested ${s()}`}</text>
      </Show>
    </Show>
    <Show key="later" when={later}>
      <text key="bound">{() => `later ${s()}`}</text>
    </Show>
    <button key="btn" onPointerClick={() => setClicked(true)} />
    <Show key="clicked" when={clicked}>
      <text key="bound">{() => `clicked ${s()}`}</text>
    </Show>
  </node>
));
"##;

#[test]
fn bound_text_in_show_and_for_renders_on_first_mount_and_on_toggle() {
    let (_dir, mut app) = setup("rx-slot-bound", BOUND_IN_SLOTS);
    let paths: [(&[&str], &str); 5] = [
        (&["root", "pre", "then", "bound"], "show x"),
        (&["root", "pre", "then", "static"], "static in show"),
        (&["root", "else", "else", "bound"], "fallback x"),
        (&["root", "For#0", "a", "row"], "a x"),
        (&["root", "outer", "then", "inner", "then", "bound"], "nested x"),
    ];
    for (path, want) in paths {
        assert_eq!(text(&mut app, path), want, "{path:?} on mount: {:?}", errors(&app));
    }

    // Mounted later by a system's signal, and by a click handler (inside the host call).
    set_alarm(&mut app, 1);
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["root", "later", "then", "bound"]), "later x", "{:?}", errors(&app));
    let btn = node(&mut app, &["root", "btn"]).unwrap();
    click(&mut app, btn, 0.0);
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["root", "clicked", "then", "bound"]), "clicked x", "{:?}", errors(&app));

    // Every binding follows its signal afterwards.
    set_alarm(&mut app, 2);
    updates(&mut app, 2);
    for (path, want) in [
        (&["root", "pre", "then", "bound"][..], "show y"),
        (&["root", "else", "else", "bound"], "fallback y"),
        (&["root", "For#0", "a", "row"], "a y"),
        (&["root", "outer", "then", "inner", "then", "bound"], "nested y"),
        (&["root", "later", "then", "bound"], "later y"),
        (&["root", "clicked", "then", "bound"], "clicked y"),
    ] {
        assert_eq!(text(&mut app, path), want, "{path:?} after set: {:?}", errors(&app));
    }

    // A new For item, and a Show toggled off and on again.
    set_alarm(&mut app, 3);
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["root", "For#0", "b", "row"]), "b y");
    set_alarm(&mut app, 4);
    updates(&mut app, 2);
    assert!(node(&mut app, &["root", "later", "then", "bound"]).is_none());
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
}

const SLOT_LIFETIMES: &str = r##"import { app, Component, u8, signal, Show, Update, type Query } from "bevy-ts";
export class Alarm extends Component { static path = "game::Alarm"; declare on: u8 }
const [open, setOpen] = signal(true);
const [outer, setOuter] = signal(true);
const [name, setName] = signal("x");
// Alarms pile up: the highest one says how far the test has got.
export function step(q: Query<[Alarm]>) {
  let top = 0;
  q.each((a) => {
    if (a.on > top) top = a.on;
  });
  if (top === 1) {
    setOpen(false);
    setOpen(true);
  }
  if (top >= 2) setOuter(false);
  if (top >= 3) setName("y");
}
app.addSystems(Update, step);
app.scene("lifetimes", () => (
  <node key="root">
    <Show key="flip" when={open} fallback={<text key="closed">closed</text>}>
      <text key="open">{() => `open ${name()}`}</text>
    </Show>
    <Show key="outer" when={outer}>
      <Show key="inner" when={() => true}>
        <text key="bound">{() => `inner ${name()}`}</text>
      </Show>
    </Show>
  </node>
));
"##;

#[test]
fn a_show_toggled_twice_in_a_batch_and_a_nested_show_unmounted_leave_nothing_behind() {
    let (_dir, mut app) = setup("rx-slot-lifetimes", SLOT_LIFETIMES);
    assert_eq!(text(&mut app, &["root", "outer", "then", "inner", "then", "bound"]), "inner x", "{:?}", errors(&app));

    // then -> else -> then inside one system: no stale fallback, the content still bound.
    set_alarm(&mut app, 1);
    updates(&mut app, 3);
    assert!(node(&mut app, &["root", "flip", "else", "closed"]).is_none(), "the fallback was withdrawn");
    assert_eq!(text(&mut app, &["root", "flip", "then", "open"]), "open x", "{:?}", errors(&app));

    // The outer Show unmounts: the inner Show's content goes, and its binding with it.
    set_alarm(&mut app, 2);
    updates(&mut app, 2);
    assert!(node(&mut app, &["root", "outer", "then", "inner", "then", "bound"]).is_none());
    set_alarm(&mut app, 3);
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["root", "flip", "then", "open"]), "open y");
    let before = calls(&app);
    updates(&mut app, 5);
    assert_eq!(calls(&app), before, "no binding left running");
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
}
