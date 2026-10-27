//! Exported params that write (writable-native-params) through the V8 backend: one fetch at the
//! TS system's ticks, deferred state after the run, refused shapes, slice overlap, the dev
//! guard, `stands_for`, and one test per row of the writing-params coverage table.
// `FilteredResourcesMut` is deprecated in Bevy; its row still needs its test.
#![allow(deprecated)]

use crate::common;

use bevy::ecs::system::{Deferred, SystemBuffer, SystemChangeTick, SystemMeta, SystemParam};
use bevy::ecs::world::{DeferredWorld, EntityMutExcept, FilteredEntityMut, FilteredResourcesMut};
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfig, GizmoConfigStore};
use bevy::gizmos::gizmos::GizmoStorage;
use bevy::prelude::*;
use bevy::time::Fixed;
use bevy_ts::host::HostParams;
use bevy_ts_native::ExportParam;
use bevy_ts_native::prelude::*;
use common::*;

#[derive(Resource, Default)]
pub struct Score(pub f32);

#[derive(Resource, Default)]
pub struct Bonus(pub f32);

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Health {
    pub v: f32,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Armor {
    pub v: f32,
}

#[derive(Component)]
pub struct Marker;

const HEAD: &str = r#"import { app, f32, nativeComponent, PostUpdate, PreUpdate, Update } from "bevy-ts";
import type { Commands, Local, Mut, Query, Res, ResMut } from "bevy-ts";
export class Health extends nativeComponent("Health", { v: f32 }) {}
export class Armor extends nativeComponent("Armor", { v: f32 }) {}
"#;

fn game(name: &str, body: &str, setup: impl FnOnce(&mut App)) -> App {
    let dir = project(name, &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<Health>().register_type::<Armor>();
    setup(&mut app);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.insert_resource(dir);
    app
}

/// Log lines starting with `prefix`, the prefix cut.
fn lines(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_string)).collect()
}

// ---------- Ticks ----------

#[derive(SystemParam)]
pub struct ScoreEdit<'w> {
    score: ResMut<'w, Score>,
    ticks: SystemChangeTick,
}

#[export(ts = "ScoreEdit", module = "bevy-ts/test")]
impl ScoreEdit<'_> {
    /// Writes the score; returns the row's changed tick and the call's run tick.
    pub fn set(&mut self, v: f32) -> Vec2 {
        self.score.0 = v;
        Vec2::new(self.score.last_changed().get() as f32, self.ticks.this_run().get() as f32)
    }
}

#[derive(SystemParam)]
pub struct ScoreRead<'w> {
    score: Res<'w, Score>,
    ticks: SystemChangeTick,
}

#[export(ts = "ScoreRead", module = "bevy-ts/test")]
impl ScoreRead<'_> {
    pub fn changed(&self) -> bool {
        self.score.is_changed()
    }
    pub fn value(&self) -> f32 {
        self.score.0
    }
    pub fn this_run(&self) -> u32 {
        self.ticks.this_run().get()
    }
    pub fn last_run(&self) -> u32 {
        self.ticks.last_run().get()
    }
}

#[test]
fn writes_carry_the_ts_systems_tick() {
    // `before` runs ahead of `writer` every frame; `writer` writes in its first run only.
    // (Before this change each host call spent a world tick and stamped writes with it.)
    let body = r#"
export function before(r: ScoreRead) { console.log(`before ${r.changed()}`); }
export function writer(e: ScoreEdit, r: ScoreRead, q: Query<[Health]>, l: Local<{ n: u32 }>) {
  if (l.n++ === 0) {
    const t = e.set(5);
    console.log(`tick ${t.x === t.y} ${t.y === q.tick}`);
  }
  console.log(`writer ${r.changed()}`);
}
app.addSystems(PreUpdate, before).addSystems(Update, writer);
"#;
    let mut app = game("native-writes-tick", body, |app| {
        app.ts_param::<ScoreEdit<'static>>().ts_param::<ScoreRead<'static>>().init_resource::<Score>();
    });
    updates(&mut app, 4);
    // The write carries the writer's run tick, the same tick TS's own `Mut` writes carry.
    assert_eq!(lines(&app, "tick "), vec!["true true"]);
    // The writer sees its own write as unchanged from its second run on; `before`, which last ran
    // ahead of it, sees it once.
    let w = lines(&app, "writer ");
    let b = lines(&app, "before ");
    assert_eq!(&w[1..3], ["false", "false"], "{w:?}");
    // Frame 1: Score was inserted before `before` first ran, so it is changed; frame 2 sees the
    // writer's frame-1 write; frame 3 sees nothing new.
    assert_eq!(&b[1..3], ["true", "false"], "{b:?}");
}

#[test]
fn a_read_only_params_is_changed_means_since_this_system_last_ran() {
    // Regression: each host call used to spend a world tick, so a second read-only call in the
    // same run saw "changed since the previous call by anyone".
    let body = r#"
export function reads(r: ScoreRead) {
  const a = r.changed();
  const b = r.changed();
  console.log(`reads ${a} ${b} ${r.thisRun() === r.thisRun()} ${r.lastRun() === r.lastRun()}`);
}
app.addSystems(Update, reads);
"#;
    let mut app = game("native-writes-read-ticks", body, |app| {
        app.ts_param::<ScoreRead<'static>>().init_resource::<Score>();
    });
    app.update();
    app.world_mut().resource_mut::<Score>().0 = 3.0;
    app.update();
    app.update();
    // Console output is collected at the next frame's pump.
    app.update();
    let l = lines(&app, "reads ");
    let l = &l[..3];
    // Every call in one run fetches at the same ticks, so both reads agree.
    assert_eq!(l, vec!["true true true true", "true true true true", "false false true true"]);
}

/// The world's change tick, read from inside a run.
#[derive(SystemParam)]
pub struct WorldTick<'w> {
    world: &'w World,
}

#[export(ts = "WorldTick", module = "bevy-ts/test")]
impl WorldTick<'_> {
    pub fn now(&self) -> u32 {
        self.world.read_change_tick().get()
    }
}

#[test]
fn calls_spend_no_ticks() {
    // Before this change each call spent one: 1,000 calls moved the tick by 1,000.
    let body = r#"
export function many(r: ScoreRead, w: WorldTick) {
  const before = w.now();
  for (let i = 0; i < 1000; i++) r.value();
  console.log(`spent ${w.now() - before}`);
}
app.addSystems(Update, many);
"#;
    let mut app = game("native-writes-no-ticks", body, |app| {
        app.ts_param::<ScoreRead<'static>>().ts_param::<WorldTick<'static>>().init_resource::<Score>();
    });
    updates(&mut app, 3);
    assert!(app.world().resource::<HostParams>().calls >= 1000);
    assert_eq!(lines(&app, "spent ")[..2], ["0", "0"]);
}

#[derive(SystemParam)]
pub struct HealthEdit<'w, 's> {
    q: Query<'w, 's, &'static mut Health>,
}

#[export(ts = "HealthEdit", module = "bevy-ts/test")]
impl HealthEdit<'_, '_> {
    pub fn set(&mut self, e: Entity, v: f32) -> bool {
        self.q.get_mut(e).map(|mut h| h.v = v).is_ok()
    }
    pub fn sum(&self, xs: &[f32]) -> f32 {
        xs.iter().sum()
    }
}

#[derive(SystemParam)]
pub struct HealthRead<'w, 's> {
    q: Query<'w, 's, &'static Health>,
}

#[export(ts = "HealthRead", module = "bevy-ts/test")]
impl HealthRead<'_, '_> {
    pub fn total(&self) -> f32 {
        self.q.iter().map(|h| h.v).sum()
    }
    pub fn sum(&self, xs: &[f32]) -> f32 {
        xs.iter().sum()
    }
    pub fn fill(&self, out: &mut [f32]) {
        out.fill(1.0);
    }
}

#[derive(Resource, Default)]
struct SeenChanged(Vec<f32>);

#[test]
fn a_native_write_is_read_back_by_ts_and_seen_by_rust_the_same_frame_without_a_dev_warning() {
    let body = r#"
export function hurt(e: HealthEdit, q: Query<[Entity, Health]>) {
  for (const [ent] of q) e.set(ent, 5);
  q.each((_, h) => console.log(`health ${h.v}`));
}
app.addSystems(Update, hurt);
"#;
    let mut app = game("native-writes-health", body, |app| {
        app.ts_param::<HealthEdit<'static, 'static>>().init_resource::<SeenChanged>();
        app.add_systems(PostUpdate, |q: Query<&Health, Changed<Health>>, mut seen: ResMut<SeenChanged>| {
            seen.0.extend(q.iter().map(|h| h.v));
        });
    });
    app.world_mut().spawn(Health { v: 1.0 });
    app.update();
    app.world_mut().resource_mut::<SeenChanged>().0.clear();
    app.update();
    app.update();
    assert!(lines(&app, "health ").iter().all(|l| l == "5"), "{:?}", messages(&app));
    // Frame 2 writes 5 over 5: still a write, seen by Rust's Changed the same frame.
    assert_eq!(app.world().resource::<SeenChanged>().0.first(), Some(&5.0));
    assert!(!messages(&app).iter().any(|m| m.contains("which no query of it declares Mut")), "{:?}", messages(&app));
}

// ---------- Deferred state ----------

#[derive(SystemParam)]
pub struct Spawner<'w, 's> {
    commands: Commands<'w, 's>,
    q: Query<'w, 's, &'static Marker>,
}

#[export(ts = "Spawner", module = "bevy-ts/test")]
impl Spawner<'_, '_> {
    pub fn spawn(&mut self) {
        self.commands.spawn(Marker);
    }
    pub fn count(&self) -> u32 {
        self.q.iter().count() as u32
    }
}

#[test]
fn a_rust_command_applies_after_the_run() {
    let body = r#"
export function spawns(s: Spawner) {
  const before = s.count();
  s.spawn();
  console.log(`during ${before} ${s.count()}`);
}
export function counts(s: Spawner) { console.log(`after ${s.count()}`); }
app.addSystems(Update, spawns).addSystems(PostUpdate, counts);
"#;
    let mut app = game("native-writes-commands", body, |app| {
        app.ts_param::<Spawner<'static, 'static>>();
    });
    app.update();
    app.update();
    assert_eq!(lines(&app, "during ")[0], "0 0");
    assert_eq!(lines(&app, "after ")[0], "1");
}

#[test]
fn a_throw_drops_the_runs_rust_commands() {
    let body = r#"
export function spawns(s: Spawner, l: Local<{ n: u32 }>) {
  s.spawn();
  if (l.n++ === 0) throw new Error("after a spawn");
}
app.addSystems(Update, spawns);
"#;
    let mut app = game("native-writes-throw", body, |app| {
        app.ts_param::<Spawner<'static, 'static>>();
    });
    app.update();
    assert_eq!(app.world_mut().query::<&Marker>().iter(app.world()).count(), 0);
    // The next run, which does not throw, spawns once: the dropped queue did not come back.
    app.update();
    assert_eq!(app.world_mut().query::<&Marker>().iter(app.world()).count(), 1);
}

#[derive(Resource, Default)]
pub struct Order(pub Vec<String>);

#[derive(SystemParam)]
pub struct First<'w, 's> {
    commands: Commands<'w, 's>,
}

#[export(ts = "First", module = "bevy-ts/test")]
impl First<'_, '_> {
    pub fn note(&mut self) {
        self.commands.queue(|w: &mut World| {
            let ts = w.query::<&Armor>().iter(w).count();
            w.resource_mut::<Order>().0.push(format!("first sees {ts} armor"));
        });
    }
}

#[derive(SystemParam)]
pub struct Second<'w, 's> {
    commands: Commands<'w, 's>,
}

#[export(ts = "Second", module = "bevy-ts/test")]
impl Second<'_, '_> {
    pub fn note(&mut self) {
        self.commands.queue(|w: &mut World| w.resource_mut::<Order>().0.push("second".into()));
    }
}

#[test]
fn ts_commands_apply_first_then_params_in_parameter_order() {
    // Called second-then-first; applied first-then-second, after TS's own spawn.
    let body = r#"
export function order(a: First, b: Second, commands: Commands) {
  b.note();
  a.note();
  commands.spawn(new Armor({ v: 1 }));
}
app.addSystems(Update, order);
"#;
    let mut app = game("native-writes-order", body, |app| {
        app.ts_param::<First<'static, 'static>>().ts_param::<Second<'static, 'static>>().init_resource::<Order>();
    });
    app.update();
    assert_eq!(app.world().resource::<Order>().0, vec!["first sees 1 armor", "second"]);
}

// ---------- Slices over the run's columns ----------

#[test]
fn a_slice_over_a_column_is_refused_for_a_writer_and_for_mut() {
    let body = r#"
function tryCall(label: string, f: () => unknown) {
  try { console.log(`${label} ok ${f()}`); } catch (e) { console.log(`${label} threw ${(e as Error).message}`); }
}
export function slices(q: Query<[Health]>, w: HealthEdit, r: HealthRead) {
  const col = q.tables[0].read(Health).f32!;
  tryCall("writer", () => w.sum(col));
  tryCall("mut", () => r.fill(col));
  tryCall("reader", () => r.sum(col));
  tryCall("copy", () => w.sum(col.slice()));
}
app.addSystems(Update, slices);
"#;
    let mut app = game("native-writes-overlap", body, |app| {
        app.ts_param::<HealthEdit<'static, 'static>>().ts_param::<HealthRead<'static, 'static>>();
    });
    app.world_mut().spawn(Health { v: 2.0 });
    app.world_mut().spawn(Health { v: 3.0 });
    app.update();
    app.update();
    let m = messages(&app);
    let find = |p: &str| m.iter().find(|l| l.starts_with(p)).cloned().unwrap_or_else(|| panic!("{p}: {m:?}"));
    let writer = find("writer ");
    assert!(writer.contains("threw HealthEdit.sum: argument xs") && writer.contains("may write that column"), "{writer}");
    let mutable = find("mut ");
    assert!(mutable.contains("threw HealthRead.fill: argument out") && mutable.contains("&mut slice"), "{mutable}");
    assert_eq!(find("reader "), "reader ok 5");
    assert_eq!(find("copy "), "copy ok 5");
    // The refused `fill` did not run.
    assert_eq!(app.world_mut().query::<&Health>().iter(app.world()).map(|h| h.v).sum::<f32>(), 5.0);
}

#[test]
fn a_param_kept_from_another_systems_run_is_refused() {
    // `b` does not take HealthEdit: its run has no overlap check or settle for it, so the call
    // throws instead of aliasing the column or leaving its deferred state to another run.
    let body = r#"
let kept: HealthEdit | undefined;
export function a(e: HealthEdit) { kept = e; }
export function b(q: Query<[Health]>) {
  try { console.log(`kept ok ${kept!.sum(q.tables[0].read(Health).f32!)}`); } catch (e) { console.log(`kept threw ${(e as Error).message}`); }
}
app.addSystems(Update, a).addSystems(PostUpdate, b);
"#;
    let mut app = game("native-writes-kept", body, |app| {
        app.ts_param::<HealthEdit<'static, 'static>>();
    });
    app.world_mut().spawn(Health { v: 2.0 });
    updates(&mut app, 2);
    let l = lines(&app, "kept ");
    assert!(l[0].starts_with("threw HealthEdit.sum: this system does not take HealthEdit"), "{l:?}");
}

// ---------- stands_for ----------

// Claims are process-wide, as the compile step reads them: these tests claim spellings no other
// test of the binary writes (`ResMut<Time<Virtual>>` is time.rs's read-only case).
#[derive(SystemParam)]
pub struct FixedClock<'w> {
    time: ResMut<'w, Time<Fixed>>,
}

#[export(ts = "FixedClock", module = "bevy-ts/test", stands_for = "ResMut<Time<Fixed>>")]
impl FixedClock<'_> {
    pub fn set_hz(&mut self, hz: f64) {
        self.time.set_timestep_hz(hz);
    }
}

#[test]
fn a_system_taking_res_mut_time_fixed_gets_the_claiming_export() {
    let body = r#"
import type { Time, Fixed } from "bevy-ts";
export function step(time: ResMut<Time<Fixed>>) { time.setHz(30); }
app.addSystems(Update, step);
"#;
    let mut app = game("native-writes-claim", body, |app| {
        app.ts_param::<FixedClock<'static>>();
    });
    assert_ne!(app.world().resource::<Time<Fixed>>().timestep().as_secs_f64(), 1.0 / 30.0);
    app.update();
    assert!((app.world().resource::<Time<Fixed>>().timestep().as_secs_f64() - 1.0 / 30.0).abs() < 1e-9);
    assert_eq!(app.world().resource::<HostParams>().claimed("ResMut< Time<Fixed> >"), Some("FixedClock"));
    // `bevy-ts types` types the spelling as the export (bevy-ts declares `Time<Fixed>`).
    let dts = bevy_ts_cli::types::emit(app.world());
    assert!(dts.contains("\"ResMut<Time<Fixed>>\": [Time<Fixed>, import(\"bevy-ts/test\").FixedClock];"), "{dts}");
}

#[test]
fn an_unclaimed_spelling_reads_as_before() {
    let src = format!("{HEAD}import {{ Time, Real }} from \"bevy-ts\";\nexport function f(time: ResMut<Time<Real>>) {{}}\napp.addSystems(Update, f);\n");
    let dir = project("native-writes-unclaimed", &[("game.ts", &src)]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<Health>().register_type::<Armor>().ts_param::<FixedClock<'static>>();
    let r = reload(&mut app, false);
    // script-time's answer for a clock write, untouched by the claim of another clock.
    assert!(r.errors.iter().any(|e| e.message.contains("read-only")), "{:?}", r.errors);
    assert!(!r.errors.iter().any(|e| e.message.contains("FixedClock")), "{:?}", r.errors);
}

#[derive(Resource)]
pub struct Unused;

#[derive(SystemParam)]
pub struct ClaimA<'w> {
    u: Option<Res<'w, Unused>>,
}

#[export(ts = "ClaimA", module = "bevy-ts/test", stands_for = "Res<Unused>")]
impl ClaimA<'_> {
    pub fn here(&self) -> bool {
        self.u.is_some()
    }
}

#[derive(SystemParam)]
pub struct ClaimB<'w> {
    u: Option<Res<'w, Unused>>,
}

#[export(ts = "ClaimB", module = "bevy-ts/test", stands_for = "Res<Unused>")]
impl ClaimB<'_> {
    pub fn here(&self) -> bool {
        self.u.is_some()
    }
}

#[test]
fn two_claims_of_one_spelling_fail_naming_both() {
    let mut world = World::new();
    let mut host = HostParams::default();
    host.register(&mut world, <ClaimA<'static> as ExportParam>::spec()).unwrap();
    let e = host.register(&mut world, <ClaimB<'static> as ExportParam>::spec()).unwrap_err();
    assert!(e.contains("Res<Unused>") && e.contains("ClaimA") && e.contains("ClaimB"), "{e}");
}

// ---------- Refused at registration ----------

#[derive(SystemParam)]
pub struct AllMut<'w, 's> {
    q: Query<'w, 's, EntityMut<'static>>,
}
#[export(ts = "AllMut", module = "bevy-ts/test")]
impl AllMut<'_, '_> {
    pub fn n(&self) -> u32 {
        self.q.iter().count() as u32
    }
}

#[derive(SystemParam)]
pub struct FilteredMut<'w, 's> {
    q: Query<'w, 's, FilteredEntityMut<'static, 'static>>,
}
#[export(ts = "FilteredMut", module = "bevy-ts/test")]
impl FilteredMut<'_, '_> {
    pub fn n(&self) -> u32 {
        self.q.iter().count() as u32
    }
    /// Tries to write every entity's `Health`; returns whether any write got through.
    pub fn write(&mut self) -> bool {
        self.q.iter_mut().any(|mut e| e.get_mut::<Health>().map(|mut h| h.v = 9.0).is_some())
    }
}

#[derive(SystemParam)]
pub struct ExceptMut<'w, 's> {
    q: Query<'w, 's, EntityMutExcept<'static, 'static, Health>>,
}
#[export(ts = "ExceptMut", module = "bevy-ts/test")]
impl ExceptMut<'_, '_> {
    pub fn n(&self) -> u32 {
        self.q.iter().count() as u32
    }
}

#[allow(deprecated)]
#[derive(SystemParam)]
pub struct ResourcesMut<'w, 's> {
    r: FilteredResourcesMut<'w, 's>,
}
#[export(ts = "ResourcesMut", module = "bevy-ts/test")]
impl ResourcesMut<'_, '_> {
    /// Tries to write `Score`; returns whether it got through.
    pub fn write(&mut self) -> bool {
        self.r.get_mut::<Score>().map(|mut s| s.0 = 9.0).is_ok()
    }
}

#[derive(SystemParam)]
pub struct WholeWorld<'w> {
    w: DeferredWorld<'w>,
}
#[export(ts = "WholeWorld", module = "bevy-ts/test")]
impl WholeWorld<'_> {
    pub fn n(&self) -> u32 {
        let _ = &self.w;
        0
    }
}

#[test]
fn params_that_may_write_everything_are_refused_naming_them() {
    let refused = |spec: bevy_ts::host::ParamSpec| {
        let mut world = World::new();
        let rust = spec.rust;
        let e = HostParams::default().register(&mut world, spec).map(|_| format!("{rust} was not refused")).unwrap_or_else(|e| e);
        assert!(e.contains(rust) && e.contains("may write every component or resource") && e.contains("EntityMut"), "{e}");
    };
    refused(<AllMut<'static, 'static> as ExportParam>::spec());
    refused(<ExceptMut<'static, 'static> as ExportParam>::spec());
    refused(<WholeWorld<'static> as ExportParam>::spec());
}

#[test]
fn builder_params_without_a_builder_reach_nothing() {
    // FilteredEntityMut and FilteredResourcesMut take their access from a builder, which an export
    // never has: they register no access, and every read or write through them fails.
    let body = r#"
export function reach(f: FilteredMut, r: ResourcesMut) { console.log(`reach ${f.n()} ${f.write()} ${r.write()}`); }
app.addSystems(Update, reach);
"#;
    let mut app = game("native-writes-builder", body, |app| {
        app.ts_param::<FilteredMut<'static, 'static>>().ts_param::<ResourcesMut<'static, 'static>>().init_resource::<Score>();
    });
    let e = app.world_mut().spawn(Health { v: 1.0 }).id();
    updates(&mut app, 2);
    assert_eq!(app.world().resource::<HostParams>().writes(app.world().resource::<HostParams>().index("FilteredMut").unwrap()), &[]);
    let l = lines(&app, "reach ");
    assert!(l[0].ends_with(" false false"), "{l:?}");
    assert_eq!(app.world().get::<Health>(e).unwrap().v, 1.0);
    assert_eq!(app.world().resource::<Score>().0, 0.0);
}

#[test]
fn ts_param_panics_on_a_refused_param() {
    let r = std::panic::catch_unwind(|| {
        App::new().ts_param::<AllMut<'static, 'static>>();
    });
    let p = r.unwrap_err();
    let msg = p.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("AllMut"), "{msg}");
}

// ---------- Coverage: one test per Supported row ----------

#[derive(SystemParam)]
pub struct MaybeBonus<'w> {
    b: Option<ResMut<'w, Bonus>>,
}

#[export(ts = "MaybeBonus", module = "bevy-ts/test")]
impl MaybeBonus<'_> {
    /// Adds to the bonus when it exists; returns whether it did.
    pub fn add(&mut self, v: f32) -> bool {
        self.b.as_mut().map(|b| b.0 += v).is_some()
    }
}

#[derive(Asset, TypePath)]
pub struct Blob(pub u32);

#[derive(SystemParam)]
pub struct Blobs<'w> {
    assets: ResMut<'w, Assets<Blob>>,
}

#[export(ts = "Blobs", module = "bevy-ts/test")]
impl Blobs<'_> {
    pub fn add(&mut self, v: u32) -> u32 {
        self.assets.add(Blob(v));
        self.assets.len() as u32
    }
}

#[test]
fn res_mut_option_res_mut_and_assets() {
    let body = r#"
export function writes(s: ScoreEdit, m: MaybeBonus, b: Blobs) {
  s.set(7);
  console.log(`bonus ${m.add(2)}`);
  console.log(`blobs ${b.add(9)}`);
}
app.addSystems(Update, writes);
"#;
    let mut app = game("native-writes-res", body, |app| {
        app.ts_param::<ScoreEdit<'static>>().ts_param::<MaybeBonus<'static>>().ts_param::<Blobs<'static>>();
        app.init_resource::<Score>().init_resource::<Assets<Blob>>();
    });
    app.update();
    app.insert_resource(Bonus(1.0));
    app.update();
    app.update();
    let bonus = app.world().resource::<Bonus>().0;
    let blobs = app.world().resource::<Assets<Blob>>().iter().map(|(_, b)| b.0).sum::<u32>();
    app.update();
    assert_eq!(app.world().resource::<Score>().0, 7.0);
    // Absent on frame 1 (None), present after.
    assert_eq!(lines(&app, "bonus ")[..3], ["false", "true", "true"]);
    assert_eq!(bonus, 5.0);
    assert_eq!(lines(&app, "blobs ")[..3], ["1", "2", "3"]);
    assert_eq!(blobs, 27);
}

#[derive(Component)]
pub struct Fuel(pub f32);

#[derive(SystemParam)]
pub struct Mixed<'w, 's> {
    q: Query<'w, 's, (Entity, &'static mut Health, &'static Armor)>,
    one: Single<'w, 's, &'static mut Armor, Without<Health>>,
    some: Populated<'w, 's, &'static mut Fuel>,
}

#[export(ts = "Mixed", module = "bevy-ts/test")]
impl Mixed<'_, '_> {
    /// Health minus armor for every armored entity; the lone unarmed armor doubles; every fuel
    /// burns one. Returns the fuels.
    pub fn apply(&mut self) -> u32 {
        for (_, mut h, a) in self.q.iter_mut() {
            h.v -= a.v;
        }
        self.one.v *= 2.0;
        self.some.iter_mut().map(|mut f| f.0 -= 1.0).count() as u32
    }
}

#[test]
fn query_mut_with_a_mixed_tuple_single_and_populated() {
    let body = r#"
export function mixed(m: Mixed) {
  try { console.log(`mixed ${m.apply()}`); } catch (e) { console.log(`mixed threw ${(e as Error).message}`); }
}
app.addSystems(Update, mixed);
"#;
    let mut app = game("native-writes-mixed", body, |app| {
        app.ts_param::<Mixed<'static, 'static>>();
    });
    // No lone Armor yet: Single fails validation and the call throws.
    app.update();
    let a = app.world_mut().spawn((Health { v: 10.0 }, Armor { v: 3.0 })).id();
    let lone = app.world_mut().spawn(Armor { v: 1.0 }).id();
    // No Fuel yet: Populated fails validation.
    app.update();
    let fuel = app.world_mut().spawn(Fuel(5.0)).id();
    app.update();
    app.update();
    let l = lines(&app, "mixed ");
    assert!(l[0].starts_with("threw Mixed.apply:"), "{l:?}");
    assert!(l[1].starts_with("threw Mixed.apply:"), "{l:?}");
    assert_eq!(&l[2..], ["1"], "{l:?}");
    // Frames 3 and 4 ran it.
    assert_eq!(app.world().get::<Health>(a).unwrap().v, 4.0);
    assert_eq!(app.world().get::<Armor>(lone).unwrap().v, 4.0);
    assert_eq!(app.world().get::<Fuel>(fuel).unwrap().0, 3.0);
}

#[derive(SystemParam)]
pub struct Sets<'w, 's> {
    set: ParamSet<'w, 's, (Query<'w, 's, &'static mut Health>, Query<'w, 's, &'static Health>)>,
}

#[export(ts = "Sets", module = "bevy-ts/test")]
impl Sets<'_, '_> {
    pub fn heal_then_total(&mut self) -> f32 {
        for mut h in self.set.p0().iter_mut() {
            h.v += 1.0;
        }
        self.set.p1().iter().map(|h| h.v).sum()
    }
}

#[test]
fn param_set_with_writes() {
    let body = r#"
export function heal(s: Sets) { console.log(`total ${s.heal_then_total()}`); }
app.addSystems(Update, heal);
"#;
    let mut app = game("native-writes-paramset", &body.replace("heal_then_total", "healThenTotal"), |app| {
        app.ts_param::<Sets<'static, 'static>>();
    });
    app.world_mut().spawn(Health { v: 1.0 });
    app.world_mut().spawn(Health { v: 2.0 });
    app.update();
    app.update();
    assert_eq!(lines(&app, "total "), vec!["5"]);
}

#[derive(Message, Clone)]
pub struct Ping(pub u32);

#[derive(SystemParam)]
pub struct Pings<'w> {
    writer: MessageWriter<'w, Ping>,
}

#[export(ts = "Pings", module = "bevy-ts/test")]
impl Pings<'_> {
    pub fn send(&mut self, v: u32) {
        self.writer.write(Ping(v));
    }
}

// Bevy refuses a writer and a mutator of one message in one param, so they are two exports.
#[derive(SystemParam)]
pub struct PingScale<'w, 's> {
    mutator: MessageMutator<'w, 's, Ping>,
}

#[export(ts = "PingScale", module = "bevy-ts/test")]
impl PingScale<'_, '_> {
    /// Multiplies every unread ping by `k`; returns how many it changed.
    pub fn scale(&mut self, k: u32) -> u32 {
        self.mutator.read().map(|p| p.0 *= k).count() as u32
    }
}

#[derive(Resource, Default)]
struct Received(Vec<u32>);

#[test]
fn message_writer_and_message_mutator() {
    let body = r#"
export function pings(p: Pings, s: PingScale) {
  p.send(2);
  p.send(3);
  console.log(`scaled ${s.scale(10)}`);
}
app.addSystems(Update, pings);
"#;
    let mut app = game("native-writes-messages", body, |app| {
        app.ts_param::<Pings<'static>>().ts_param::<PingScale<'static, 'static>>().add_message::<Ping>().init_resource::<Received>();
        app.add_systems(PostUpdate, |mut r: MessageReader<Ping>, mut got: ResMut<Received>| got.0.extend(r.read().map(|p| p.0)));
    });
    app.update();
    app.update();
    assert_eq!(lines(&app, "scaled ")[0], "2");
    assert_eq!(&app.world().resource::<Received>().0[..2], [20, 30]);
}

pub struct Counter(pub u32);
pub struct Lamp;

#[derive(SystemParam)]
pub struct MainThread<'w> {
    c: NonSendMut<'w, Counter>,
    maybe: Option<NonSendMut<'w, Lamp>>,
}

#[export(ts = "MainThread", module = "bevy-ts/test")]
impl MainThread<'_> {
    pub fn bump(&mut self) -> u32 {
        self.c.0 += 1;
        self.c.0 + if self.maybe.is_some() { 100 } else { 0 }
    }
}

#[test]
fn non_send_mut_and_option_non_send_mut() {
    let body = r#"
export function bump(m: MainThread) { console.log(`bump ${m.bump()}`); }
app.addSystems(Update, bump);
"#;
    let mut app = game("native-writes-nonsend", body, |app| {
        app.ts_param::<MainThread<'static>>();
        app.insert_non_send(Counter(0));
    });
    app.update();
    app.world_mut().insert_non_send(Lamp);
    app.update();
    app.update();
    assert_eq!(app.world().non_send::<Counter>().0, 3);
    app.update();
    assert_eq!(lines(&app, "bump ")[..3], ["1", "102", "103"]);
}

/// A custom deferred buffer: pushes onto `Order` when applied.
#[derive(Default)]
pub struct Notes(Vec<String>);

impl SystemBuffer for Notes {
    fn queue(&mut self, _: &SystemMeta, mut world: DeferredWorld) {
        world.resource_mut::<Order>().0.append(&mut self.0);
    }
}

#[derive(SystemParam)]
pub struct Drawing<'w, 's> {
    gizmos: Gizmos<'w, 's>,
    notes: Deferred<'s, Notes>,
    parallel: ParallelCommands<'w, 's>,
}

#[export(ts = "Drawing", module = "bevy-ts/test")]
impl Drawing<'_, '_> {
    pub fn draw(&mut self) {
        self.gizmos.line(Vec3::ZERO, Vec3::X, Color::WHITE);
        self.notes.0.push("noted".into());
        self.parallel.command_scope(|mut c| {
            c.spawn(Marker);
        });
    }
}

#[test]
fn gizmos_custom_deferred_and_parallel_commands_apply_after_the_run() {
    let body = r#"
export function draw(d: Drawing, l: Local<{ n: u32 }>) { if (l.n++ === 0) d.draw(); }
app.addSystems(Update, draw);
"#;
    let mut app = game("native-writes-gizmos", body, |app| {
        app.init_resource::<Order>().init_resource::<GizmoConfigStore>();
        app.world_mut().resource_mut::<GizmoConfigStore>().insert(GizmoConfig::default(), DefaultGizmoConfigGroup);
        app.init_resource::<GizmoStorage<DefaultGizmoConfigGroup, ()>>();
        app.ts_param::<Drawing<'static, 'static>>();
    });
    let gizmo_tick = |app: &App| app.world().resource_ref::<GizmoStorage<DefaultGizmoConfigGroup, ()>>().last_changed();
    let before = gizmo_tick(&app);
    app.update();
    let drawn = gizmo_tick(&app);
    assert!(drawn.is_newer_than(before, app.world().read_change_tick()), "the gizmo buffer was not applied");
    assert_eq!(app.world().resource::<Order>().0, vec!["noted"]);
    assert_eq!(app.world_mut().query::<&Marker>().iter(app.world()).count(), 1);
    // A frame without a call applies nothing.
    app.update();
    assert_eq!(gizmo_tick(&app), drawn);
    assert_eq!(app.world_mut().query::<&Marker>().iter(app.world()).count(), 1);
}

#[derive(SystemParam)]
pub struct Shared<'s> {
    n: Local<'s, u32>,
}

#[export(ts = "Shared", module = "bevy-ts/test")]
impl Shared<'_> {
    pub fn count(&mut self) -> u32 {
        *self.n += 1;
        *self.n
    }
}

#[test]
fn a_local_is_shared_by_every_ts_system_that_takes_the_param() {
    let body = r#"
export function a(s: Shared) { console.log(`a ${s.count()}`); }
export function b(s: Shared) { console.log(`b ${s.count()}`); }
app.addSystems(Update, a).addSystems(PostUpdate, b);
"#;
    let mut app = game("native-writes-local", body, |app| {
        app.ts_param::<Shared<'static>>();
    });
    updates(&mut app, 3);
    assert_eq!(lines(&app, "a ")[..2], ["1", "3"]);
    assert_eq!(lines(&app, "b ")[..2], ["2", "4"]);
}

