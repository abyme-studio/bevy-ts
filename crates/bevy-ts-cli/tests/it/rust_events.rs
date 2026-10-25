//! Rust messages and events from TS, and `commands.entity(e).observe` (openspec rust-events).

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::observers::TsEntityObserver;
use common::*;
use bevy::ecs::reflect::ReflectMessage;
use bevy::input::ButtonState;
use bevy::input::InputPlugin;
use bevy::input::gamepad::{GamepadConnection, GamepadConnectionEvent, GamepadEvent, GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::input::gestures::PinchGesture;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::window::{CursorMoved, ExitCondition, FileDragAndDrop, WindowResized};
use bevy_ts::commands::TsSpawned;
use bevy_ts_native::BevyTsNativeExt;
use std::time::Duration;

const HEAD: &str = r#"import { app, Component, EntityEvent, f32, Startup, u32, Update } from "bevy-ts";
import type { Commands, Entity, On, Query } from "bevy-ts";
export class Tag extends Component { static path = "game::Tag"; n: u32 = 0 }
export class Hit extends EntityEvent { static path = "game::Hit"; static propagate = true; declare amount: f32 }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

fn attachments(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&TsEntityObserver>().iter(world).count()
}

fn lines(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.starts_with(prefix)).collect()
}

#[test]
fn an_entity_observer_runs_for_its_entity_only_and_dies_with_it() {
    let (_, mut app) = load(
        "entity-observe",
        &src(
            r#"let step = 0;
export function setup(c: Commands) {
  const a = c.spawn(Tag.of({ n: 1 }));
  c.spawn(Tag.of({ n: 2 }));
  const p = c.spawn(Tag.of({ n: 10 }));
  const kid = c.spawn(Tag.of({ n: 11 }));
  c.entity(kid).setParent(p);
  c.entity(a).observe(onHit);
  c.entity(p).observe(onHit);
}
export function onHit(hit: On<Hit>, q: Query<[Tag]>) { const r = q.get(hit.entity); const o = q.get(hit.original); console.log(`hit ${r ? r[0].n : "?"} ${hit.amount} ${o ? o[0].n : "?"}`); }
export function drive(q: Query<[Entity, Tag]>, c: Commands) {
  step++;
  if (step === 2) q.each((e, t) => c.trigger(Hit.of({ entity: e, amount: t.n })));
  if (step === 3) q.each((e, t) => { if (t.n === 1) c.entity(e).despawn(); });
  if (step === 4) c.spawn(Tag.of({ n: 3 }));
  if (step === 5) q.each((e, t) => { if (t.n !== 10 && t.n !== 11) c.trigger(Hit.of({ entity: e, amount: t.n })); });
}
app.addSystems(Startup, setup).addSystems(Update, drive);
"#,
        ),
    );
    updates(&mut app, 2);
    assert_eq!(attachments(&mut app), 2);
    updates(&mut app, 4);
    let mut hits = lines(&app, "hit ");
    hits.sort();
    // `a` for its own hit; `p` for its own and, bubbling, for its child's, with `entity` itself
    // and `original` the child.
    assert_eq!(hits, ["hit 1 1 1", "hit 10 10 10", "hit 10 11 11"], "{:?}", messages(&app));
    assert_eq!(attachments(&mut app), 1, "a's observer went with a");
}

#[test]
fn an_observer_that_takes_no_entity_event_fails_its_run() {
    let (_, mut app) = load(
        "entity-observe-global",
        &src(
            r#"import { Event } from "bevy-ts";
export class Ping extends Event { static path = "game::Ping"; declare n: u32 }
export function setup(c: Commands) { c.entity(c.spawn(Tag.of())).observe(onPing); }
export function onPing(p: On<Ping>) {}
app.addSystems(Startup, setup);
"#,
        ),
    );
    app.update();
    let errors: Vec<String> = logs(&app).into_iter().map(|l| l.message).filter(|m| m.contains("onPing")).collect();
    assert!(errors.iter().any(|m| m.contains("EntityEvent")), "{:?}", logs(&app));
    assert_eq!(attachments(&mut app), 0);
}

#[test]
fn a_reload_keeps_entity_observers_and_drops_removed_ones() {
    let body = |say: &str, name: &str| {
        format!(
            r#"export function setup(c: Commands) {{ c.entity(c.spawn(Tag.of({{ n: 1 }}))).observe({name}); }}
export function {name}(hit: On<Hit>) {{ console.log("{say}"); }}
export function drive(q: Query<[Entity, Tag]>, c: Commands) {{ q.each((e) => c.trigger(Hit.of({{ entity: e, amount: 1 }}))); }}
app.addSystems(Startup, setup).addSystems(Update, drive);
"#
        )
    };
    let (dir, mut app) = load("entity-observe-reload", &src(&body("v1", "onHit")));
    updates(&mut app, 2);
    assert!(!lines(&app, "v1").is_empty());
    write(&dir, "game.ts", &src(&body("v2", "onHit")));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    // Console output reaches the log at the next frame's start.
    updates(&mut app, 3);
    assert_eq!(lines(&app, "v2").len(), 2, "the new body runs, once a hit: {:?}", messages(&app));
    assert_eq!(lines(&app, "v1").len(), 2, "the old body no more");
    assert_eq!(attachments(&mut app), 1);

    write(&dir, "game.ts", &src(&body("v3", "renamed")));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    assert!(lines(&app, "v3").is_empty(), "startups do not rerun, and onHit is gone");
    assert_eq!(attachments(&mut app), 0);
    assert!(messages(&app).iter().any(|m| m.contains("onHit no longer exists")), "{:?}", messages(&app));
}

#[test]
fn a_restart_removes_entity_observers() {
    let (_, mut app) = load(
        "entity-observe-restart",
        &src("export function setup(c: Commands) { c.entity(c.spawn(Tag.of())).observe(onHit); }\nexport function onHit(hit: On<Hit>) {}\napp.addSystems(Startup, setup);\n"),
    );
    app.update();
    assert_eq!(attachments(&mut app), 1);
    assert_eq!(reload(&mut app, true).status, ReloadStatus::Applied);
    app.update();
    assert_eq!(attachments(&mut app), 1, "the startup attached it again, once");
}

#[test]
fn an_entity_observer_runs_for_a_trigger_whose_target_is_despawned_before_the_drain() {
    let (_, mut app) = load(
        "entity-observe-despawned",
        &src(
            r#"import type { Remove } from "bevy-ts";
let step = 0;
export function setup(c: Commands) {
  const a = c.spawn(Tag.of({ n: 1 }));
  c.entity(a).observe(onHit);
  const b = c.spawn(Tag.of({ n: 2 }));
  c.entity(b).observe(onGone);
}
export function onHit(hit: On<Hit>) { console.log(`hit ${hit.amount}`); }
export function onGone(gone: On<Remove<Tag>>) { console.log("gone"); }
export function drive(q: Query<[Entity, Tag]>, c: Commands) {
  step++;
  // Trigger, then despawn the target, in one system: Bevy runs the target's observer.
  if (step === 2) q.each((e, t) => { if (t.n === 1) { c.trigger(Hit.of({ entity: e, amount: 7 })); c.entity(e).despawn(); } });
  // Despawning an entity whose observer watches Remove<Tag> runs it.
  if (step === 3) q.each((e, t) => { if (t.n === 2) c.entity(e).despawn(); });
}
app.addSystems(Startup, setup).addSystems(Update, drive);
"#,
        ),
    );
    updates(&mut app, 5);
    assert_eq!(lines(&app, "hit "), ["hit 7"], "{:?}", messages(&app));
    assert_eq!(lines(&app, "gone"), ["gone"], "{:?}", messages(&app));
    assert_eq!(attachments(&mut app), 0, "both went with their entities");
}

// ---------- Rust messages and events, payloads as JSON (stage 2) ----------


/// A project's own message, `app.ts_message::<Scored>()`.
#[derive(Message, Reflect, Debug, Clone, PartialEq)]
struct Scored {
    points: u32,
}

/// A third party's message with only Bevy's `#[reflect(Message)]`.
#[derive(Message, Reflect, Debug, Clone, PartialEq)]
#[reflect(Message)]
struct ThirdPartyPing {
    n: u32,
}

/// A bubbling entity event whose fields take `On`'s own names.
#[derive(EntityEvent, Reflect, Clone)]
#[entity_event(propagate, auto_propagate)]
struct Nudge {
    entity: Entity,
    propagate: bool,
    original: u32,
}

/// What a Rust system read.
#[derive(Resource)]
struct Seen<T: Send + Sync + 'static>(Vec<T>);

impl<T: Send + Sync + 'static> Default for Seen<T> {
    fn default() -> Self {
        Seen(Vec::new())
    }
}

const RUST_HEAD: &str = r#"import { app, chain, Component, onMessage, PostUpdate, rustEntityEvent, rustMessage, Startup, systems, Update } from "bevy-ts";
import type { Commands, Entity, MessageReader, MessageWriter, On, Query } from "bevy-ts";
export class Pad extends Component { static path = "game::Pad" }
"#;

fn rust_src(body: &str) -> String {
    format!("{RUST_HEAD}{body}")
}

/// A headless app with Bevy's input and window plugins and the prebuilt engine's Rust messages
/// and events, `extra` added, `game.ts` loaded.
fn rust_app(name: &str, body: &str, extra: impl FnOnce(&mut App)) -> App {
    let (app, r) = try_rust_app(name, body, extra);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    app
}

fn try_rust_app(name: &str, body: &str, extra: impl FnOnce(&mut App)) -> (App, bevy_ts::generation::ReloadReport) {
    let dir = project(name, &[("game.ts", &rust_src(body))]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(InputPlugin)
        .add_plugins(bevy::window::WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .add_plugins(bevy_ts_cli::rust_events::RustEventsPlugin);
    extra(&mut app);
    let r = reload(&mut app, false);
    (app, r)
}

/// One frame, its console output pumped into the log.
fn frame(app: &mut App) {
    app.update();
    bevy_ts::runtime::pump(app.world_mut());
}

fn key(code: KeyCode, ch: &str) -> KeyboardInput {
    KeyboardInput { key_code: code, logical_key: Key::Character(ch.into()), state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER }
}

#[test]
fn a_ts_reader_sees_keyboard_input_once() {
    let mut app = rust_app(
        "rust-keyboard",
        r#"import { KeyboardInput } from "bevy-ts/bevy";
export function first(r: MessageReader<KeyboardInput>) { r.each((k) => console.log(`first ${k.key_code} ${k.state}`)); }
export function second(r: MessageReader<KeyboardInput>) { for (const k of r) console.log(`second ${JSON.stringify(k.logical_key)} ${r.len}`); }
export function gated() { console.log("gated"); }
app.addSystems(Update, first, second, systems(gated).runIf(onMessage(KeyboardInput)));
"#,
        |_| {},
    );
    frame(&mut app);
    assert!(lines(&app, "gated").is_empty(), "no message, no run: {:?}", messages(&app));
    app.world_mut().write_message(key(KeyCode::KeyA, "a"));
    app.world_mut().write_message(key(KeyCode::KeyB, "b"));
    frame(&mut app);
    assert_eq!(lines(&app, "first "), ["first KeyA Pressed", "first KeyB Pressed"], "{:?}", messages(&app));
    assert_eq!(lines(&app, "second "), [r#"second {"Character":"a"} 2"#, r#"second {"Character":"b"} 2"#]);
    assert_eq!(lines(&app, "gated").len(), 1);
    // Still in Bevy's queue for one more update, but read.
    frame(&mut app);
    frame(&mut app);
    assert_eq!(lines(&app, "first ").len(), 2, "each read once: {:?}", messages(&app));
    assert_eq!(lines(&app, "gated").len(), 1);

    // A restart moves the cursors to the queue's end: a message from before it is not replayed.
    app.world_mut().write_message(key(KeyCode::KeyC, "c"));
    assert_eq!(reload(&mut app, true).status, ReloadStatus::Applied);
    app.world_mut().write_message(key(KeyCode::KeyD, "d"));
    frame(&mut app);
    assert!(lines(&app, "first KeyC").is_empty(), "{:?}", messages(&app));
    assert_eq!(lines(&app, "first KeyD").len(), 1, "{:?}", messages(&app));
}

#[test]
fn a_ts_writer_sends_a_rumble_request_a_rust_reader_sees() {
    let mut app = rust_app(
        "rust-rumble",
        r#"import { GamepadRumbleRequest } from "bevy-ts/bevy";
let pad: Entity | undefined;
export function rumble(w: MessageWriter<GamepadRumbleRequest>, c: Commands) {
  if (pad === undefined) pad = c.spawn(Pad.of());
  else if (pad !== -1) { w.write(GamepadRumbleRequest.of({ Add: { gamepad: pad, duration: 0.5, intensity: { strong_motor: 1, weak_motor: 0.25 } } })); pad = -1; }
}
app.addSystems(Update, rumble);
"#,
        |app| {
            app.init_resource::<Seen<GamepadRumbleRequest>>()
                .add_systems(PostUpdate, |mut r: MessageReader<GamepadRumbleRequest>, mut seen: ResMut<Seen<GamepadRumbleRequest>>| seen.0.extend(r.read().cloned()));
        },
    );
    updates(&mut app, 3);
    let seen = &app.world().resource::<Seen<GamepadRumbleRequest>>().0;
    let [GamepadRumbleRequest::Add { duration, intensity, gamepad }] = seen.as_slice() else { panic!("one Add, got {seen:?}: {:?}", messages(&app)) };
    assert_eq!(*duration, Duration::from_millis(500));
    assert_eq!(*intensity, GamepadRumbleIntensity { strong_motor: 1.0, weak_motor: 0.25 });
    assert!(app.world().get::<TsSpawned>(*gamepad).is_some(), "the pad TS spawned");
}

#[test]
fn window_messages_reach_a_ts_reader() {
    let mut app = rust_app(
        "rust-window",
        r#"import { CursorMoved, FileDragAndDrop, WindowResized } from "bevy-ts/bevy";
export function win(a: MessageReader<WindowResized>, b: MessageReader<CursorMoved>, d: MessageReader<FileDragAndDrop>) {
  a.each((m) => console.log(`resized ${m.width}x${m.height}`));
  b.each((m) => console.log(`cursor ${JSON.stringify(m.position)} ${JSON.stringify(m.delta)}`));
  d.each((m) => console.log(`drop ${JSON.stringify(m)}`));
}
app.addSystems(Update, win);
"#,
        |_| {},
    );
    let window = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(WindowResized { window, width: 640.0, height: 480.0 });
    app.world_mut().write_message(CursorMoved { window, position: Vec2::new(3.0, 4.0), delta: None });
    app.world_mut().write_message(FileDragAndDrop::DroppedFile { window, path_buf: "a.png".into() });
    frame(&mut app);
    assert_eq!(lines(&app, "resized "), ["resized 640x480"], "{:?}", messages(&app));
    assert_eq!(lines(&app, "cursor "), ["cursor [3,4] null"]);
    let drop = lines(&app, "drop ");
    assert!(drop.len() == 1 && drop[0].starts_with(r#"drop {"DroppedFile":{"#) && drop[0].contains(r#""path_buf":"a.png""#), "{drop:?}");
}

#[test]
fn input_messages_reach_a_ts_reader() {
    let mut app = rust_app(
        "rust-input",
        r#"import { GamepadEvent, MouseButtonInput, MouseMotion, MouseWheel, PinchGesture, TouchInput } from "bevy-ts/bevy";
export function input(b: MessageReader<MouseButtonInput>, m: MessageReader<MouseMotion>, w: MessageReader<MouseWheel>, t: MessageReader<TouchInput>, p: MessageReader<PinchGesture>, g: MessageReader<GamepadEvent>) {
  for (const [name, r] of [["button", b], ["motion", m], ["wheel", w], ["touch", t], ["pinch", p], ["gamepad", g]] as const) r.each((v: unknown) => console.log(`${name} ${JSON.stringify(v)}`));
}
app.addSystems(Update, input);
"#,
        |_| {},
    );
    let window = app.world_mut().spawn_empty().id();
    let pad = app.world_mut().spawn_empty().id();
    let w = app.world_mut();
    w.write_message(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window });
    w.write_message(MouseMotion { delta: Vec2::new(1.0, 2.0) });
    w.write_message(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: 1.0, window, phase: TouchPhase::Moved });
    w.write_message(TouchInput { phase: TouchPhase::Started, position: Vec2::new(5.0, 6.0), window, force: None, id: 7 });
    w.write_message(PinchGesture(0.5));
    w.write_message(GamepadEvent::Connection(GamepadConnectionEvent::new(pad, GamepadConnection::Disconnected)));
    frame(&mut app);
    let got = |p: &str| lines(&app, p);
    assert!(got("button ").iter().any(|l| l.contains(r#""button":"Left""#) && l.contains(r#""state":"Pressed""#)), "{:?}", messages(&app));
    assert_eq!(got("motion "), [r#"motion {"delta":[1,2]}"#]);
    assert!(got("wheel ").iter().any(|l| l.contains(r#""unit":"Line""#) && l.contains(r#""y":1"#)), "{:?}", got("wheel "));
    assert!(got("touch ").iter().any(|l| l.contains(r#""phase":"Started""#) && l.contains(r#""position":[5,6]"#) && l.contains(r#""id":7"#)), "{:?}", got("touch "));
    assert_eq!(got("pinch "), ["pinch 0.5"]);
    assert!(got("gamepad ").iter().any(|l| l.contains(r#""Connection""#) && l.contains(r#""Disconnected""#)), "{:?}", got("gamepad "));
}

#[test]
fn asset_event_image_reaches_a_ts_reader() {
    let mut app = rust_app(
        "rust-asset-event",
        r#"import { AssetEvent, Image, Mesh } from "bevy-ts/bevy";
export function images(r: MessageReader<AssetEvent<Image>>) { r.each((m) => console.log(`image ${JSON.stringify(m)}`)); }
export function meshes(r: MessageReader<AssetEvent<Mesh>>) { r.each((m) => console.log(`mesh ${JSON.stringify(m)}`)); }
app.addSystems(Update, images, meshes);
"#,
        |_| {},
    );
    app.world_mut().write_message(bevy::asset::AssetEvent::<Image>::Added { id: bevy::asset::AssetId::default() });
    frame(&mut app);
    let images = lines(&app, "image ");
    assert!(images.len() == 1 && images[0].starts_with(r#"image {"Added":"#), "{:?}", messages(&app));
    assert!(lines(&app, "mesh ").is_empty(), "AssetEvent<Mesh> is another queue");
}

fn click(entity: Entity) -> bevy::picking::events::PointerClick {
    use bevy::picking::backend::HitData;
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    let location = Location { target: bevy::camera::NormalizedRenderTarget::None { width: 1, height: 1 }, position: Vec2::ZERO };
    bevy::picking::events::PointerClick {
        entity,
        pointer: bevy::picking::events::Pointer::new(PointerId::Mouse, location),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        duration: Duration::ZERO,
        count: 1,
    }
}

/// A parent and its child, as `(parent, child)`.
fn family(app: &mut App) -> (Entity, Entity) {
    let parent = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ChildOf(parent)).id();
    (parent, child)
}

#[test]
fn a_global_observer_sees_a_bubbling_pointer_click() {
    let mut app = rust_app(
        "rust-click",
        r#"import { PointerClick } from "bevy-ts/bevy";
export function onClick(click: On<PointerClick>) { console.log(`click ${click.entity} ${click.original} ${click.event.entity} ${click.count}`); }
app.addObserver(onClick);
"#,
        |_| {},
    );
    let (parent, child) = family(&mut app);
    app.world_mut().trigger(click(child));
    frame(&mut app);
    let (p, c) = (parent.to_bits(), child.to_bits());
    // As Bevy's `On::event()`, the event's own `entity` is the current target at each hop.
    assert_eq!(lines(&app, "click "), [format!("click {c} {c} {c} 1"), format!("click {p} {c} {p} 1")], "{:?}", messages(&app));
}

#[test]
fn propagate_false_drops_the_rest_of_a_rust_triggers_hops() {
    let mut app = rust_app(
        "rust-click-stop",
        r#"import { PointerClick } from "bevy-ts/bevy";
export function onClick(click: On<PointerClick>) { console.log(`click ${click.entity}`); click.propagate(false); }
app.addObserver(onClick);
"#,
        |_| {},
    );
    let (_, child) = family(&mut app);
    app.world_mut().trigger(click(child));
    frame(&mut app);
    assert_eq!(lines(&app, "click "), [format!("click {}", child.to_bits())], "{:?}", messages(&app));
}

#[test]
fn a_rust_event_field_named_like_ons_own_is_read_on_event() {
    let mut app = rust_app(
        "rust-nudge",
        r#"export const Nudge = rustEntityEvent("it::rust_events::Nudge");
export function onNudge(n: On<Nudge>) {
  console.log(`nudge ${n.entity} ${n.event.entity} ${n.event.propagate} ${n.event.original} ${typeof n.propagate} ${n.original}`);
  n.propagate(false);
}
app.addObserver(onNudge);
"#,
        |app| {
            app.ts_entity_event::<Nudge>();
        },
    );
    let (_, child) = family(&mut app);
    app.world_mut().trigger(Nudge { entity: child, propagate: true, original: 7 });
    frame(&mut app);
    let c = child.to_bits();
    // `propagate(false)` still stops bubbling: the parent hop never runs.
    assert_eq!(lines(&app, "nudge "), [format!("nudge {c} {c} true 7 function {c}")], "{:?}", messages(&app));
}

#[test]
fn an_entity_observer_of_a_rust_event_runs_for_its_entity_only() {
    let mut app = rust_app(
        "rust-nudge-entity",
        r#"export const Nudge = rustEntityEvent("it::rust_events::Nudge");
let done = false;
export function attach(q: Query<[Entity, Pad]>, c: Commands) { if (!done) q.each((e) => c.entity(e).observe(onNudge)); done = true; }
export function onNudge(n: On<Nudge>) { console.log(`nudge ${n.entity} ${n.original}`); }
app.addSystems(Update, attach);
"#,
        |app| {
            app.ts_entity_event::<Nudge>();
        },
    );
    let parent = spawn_ts(&mut app, "game::Pad", serde_json::json!({}), ());
    let child = spawn_ts(&mut app, "game::Pad", serde_json::json!({}), ChildOf(parent));
    let other = spawn_ts(&mut app, "game::Pad", serde_json::json!({}), ());
    frame(&mut app);
    assert_eq!(attachments(&mut app), 3);
    app.world_mut().despawn(other);
    app.world_mut().trigger(Nudge { entity: child, propagate: true, original: 0 });
    frame(&mut app);
    let (p, c) = (parent.to_bits(), child.to_bits());
    assert_eq!(lines(&app, "nudge "), [format!("nudge {c} {c}"), format!("nudge {p} {c}")], "{:?}", messages(&app));
    assert_eq!(attachments(&mut app), 2, "other's went with it");
}

#[test]
fn an_unlisted_reflect_message_is_writable() {
    let mut app = rust_app(
        "rust-unlisted",
        r#"export const Ping = rustMessage("it::rust_events::ThirdPartyPing");
let sent = false;
export function send(w: MessageWriter<Ping>) { if (!sent) { w.write({ n: 3 }); sent = true; } }
app.addSystems(Update, send);
"#,
        |app| {
            app.add_message::<ThirdPartyPing>().register_type::<ThirdPartyPing>().init_resource::<Seen<ThirdPartyPing>>().add_systems(
                PostUpdate,
                |mut r: MessageReader<ThirdPartyPing>, mut seen: ResMut<Seen<ThirdPartyPing>>| seen.0.extend(r.read().cloned()),
            );
        },
    );
    updates(&mut app, 2);
    assert_eq!(app.world().resource::<Seen<ThirdPartyPing>>().0, [ThirdPartyPing { n: 3 }], "{:?}", messages(&app));

    let (_, r) = try_rust_app(
        "rust-unlisted-reader",
        r#"export const Ping = rustMessage("it::rust_events::ThirdPartyPing");
export function read(r: MessageReader<Ping>) {}
app.addSystems(Update, read);
"#,
        |app| {
            app.add_message::<ThirdPartyPing>().register_type::<ThirdPartyPing>();
        },
    );
    assert!(r.errors.iter().any(|e| e.message.contains("ThirdPartyPing") && e.message.contains("app.ts_message")), "{:?}", r.errors);
}

#[test]
fn a_project_message_is_read_and_written_from_ts() {
    let mut app = rust_app(
        "rust-scored",
        r#"export const Scored = rustMessage("it::rust_events::Scored");
let sent = false;
export function send(w: MessageWriter<Scored>) { if (!sent) { w.write(Scored.of({ points: 5 })); sent = true; } }
export function read(r: MessageReader<Scored>) { r.each((s) => console.log(`scored ${s.points}`)); }
app.addSystems(Update, chain(send, read));
"#,
        |app| {
            app.ts_message::<Scored>();
        },
    );
    app.world_mut().write_message(Scored { points: 9 });
    frame(&mut app);
    frame(&mut app);
    assert_eq!(lines(&app, "scored "), ["scored 9", "scored 5"], "{:?}", messages(&app));
}

#[test]
fn on_event_holds_a_ts_events_whole_value() {
    let (_, mut app) = load(
        "ts-on-event",
        &src(
            r#"export function setup(c: Commands) { const e = c.spawn(Tag.of({ n: 1 })); c.trigger(Hit.of({ entity: e, amount: 3 })); }
export function onHit(hit: On<Hit>) { console.log(`event ${JSON.stringify(hit.event)} ${hit.amount} ${hit.entity}`); }
app.addSystems(Startup, setup).addObserver(onHit);
"#,
        ),
    );
    updates(&mut app, 2);
    let line = lines(&app, "event ").pop().unwrap_or_else(|| panic!("{:?}", messages(&app)));
    let mut parts = line.splitn(4, ' ').skip(1);
    let event: serde_json::Value = serde_json::from_str(parts.next().unwrap()).unwrap();
    let (amount, entity) = (parts.next().unwrap(), parts.next().unwrap());
    assert_eq!(event, serde_json::json!({ "amount": amount.parse::<f64>().unwrap(), "entity": entity.parse::<u64>().unwrap() }), "{line}");
}

#[test]
fn a_failed_reload_keeps_attachments_made_since_the_last_drain() {
    let body = |tail: &str| {
        format!(
            r#"export function setup(c: Commands) {{ c.entity(c.spawn(Tag.of({{ n: 1 }}))).observe(onHit); }}
export function onHit(hit: On<Hit>) {{ console.log(`hit ${{hit.amount}}`); }}
export function drive(q: Query<[Entity, Tag]>, c: Commands) {{ q.each((e) => c.trigger(Hit.of({{ entity: e, amount: 1 }}))); }}
app.addSystems(Startup, setup).addSystems(Update, drive);
{tail}"#
        )
    };
    let (dir, mut app) = load("entity-observe-failed-reload", &src(&body("")));
    app.update();
    // Stand for an attachment no drain has built yet: take the built observer back out and attach
    // its declaration to a second entity as an `observe` command would.
    let (index, decl) = {
        let mut systems = app.world_mut().resource_mut::<bevy_ts::systems::TsSystems>();
        let name = systems.targeted.keys().next().cloned().expect("onHit is built");
        let s = systems.targeted.remove(&name).unwrap();
        (s.key.index, s.decl)
    };
    let e = spawn_ts(&mut app, "game::Tag", serde_json::json!({ "n": 2 }), ());
    bevy_ts::observers::observe(app.world_mut(), e, index, decl);
    write(&dir, "game.ts", &src(&body("this is not TS")));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Failed);
    updates(&mut app, 2);
    assert_eq!(lines(&app, "hit ").len(), 4, "both entities' observers run, twice: {:?}", messages(&app));
}
