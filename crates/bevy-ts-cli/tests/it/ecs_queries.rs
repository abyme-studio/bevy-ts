//! The `ecs-coverage` rows: Bevy 0.20's query, command, lifecycle and hierarchy items under
//! Bevy's names in TS, one test each (openspec/changes/ecs-queries).

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use common::*;
use serde_json::json;
use std::path::PathBuf;

/// A sparse-set marker, as Bevy advises for markers added and removed often.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
struct Tag;

/// Sparse-set data: refused as query data.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
struct Spark;

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Armor)]
pub struct Knight {
    pub level: f32,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Armor {
    pub v: f32,
}

const HEAD: &str = r#"import { app, Component, EntityEvent, f32, nativeComponent, PostUpdate, rustComponent, Startup, Transform, u32, Update } from "bevy-ts";
import { ChildOf, Disabled } from "bevy-ts/bevy";
import type { Add, Added, AnyOf, Changed, Commands, Despawn, Discard, Entity, Has, Mut, On, Option, Or, Populated, Query, Ref, Remove, RemovedComponents, Single, Spawned, SystemChangeTick, With, Without } from "bevy-ts";
export class Health extends Component { static path = "game::Health"; hp: f32 = 100 }
export class Shield extends Component { static path = "game::Shield"; v: f32 = 1 }
export class Plain extends Component { static path = "game::Plain"; n: u32 = 0 }
export class Enemy extends Component { static path = "game::Enemy" }
export class Player extends Component { static path = "game::Player" }
export class Frozen extends Component { static path = "game::Frozen" }
export const Tag = rustComponent("Tag");
export const Spark = rustComponent("Spark");
export class Knight extends nativeComponent("Knight", { level: f32 }) {}
export class Armor extends nativeComponent("Armor", { v: f32 }) {}
export class Hit extends EntityEvent { static path = "game::Hit"; static propagate = true; declare n: u32 }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

/// The test app: the Rust components registered, and assets so `<mesh>` builds.
fn test_app(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    app.register_type::<Knight>().register_type::<Armor>();
    app.world_mut().register_component::<Tag>();
    app.world_mut().register_component::<Spark>();
    app
}

/// Loads `HEAD`, lets `setup` spawn entities, then loads `body` with `$0`, `$1`, ... standing for
/// the entities `setup` returned.
fn staged(name: &str, setup: impl FnOnce(&mut App) -> Vec<Entity>, body: &str) -> (PathBuf, App, Vec<Entity>) {
    let dir = project(name, &[("game.tsx", &src(""))]);
    let mut app = test_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let es = setup(&mut app);
    let mut body = body.to_string();
    for (i, e) in es.iter().enumerate().rev() {
        body = body.replace(&format!("${i}"), &e.to_bits().to_string());
    }
    write(&dir, "game.tsx", &src(&body));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app, es)
}

/// A project that loads `body`, with nothing spawned first.
fn loaded(name: &str, body: &str) -> App {
    staged(name, |_| vec![], body).1
}

/// The errors `body` fails to load with.
fn errors(name: &str, body: &str) -> Vec<bevy_ts::backend::ScriptError> {
    let dir = project(name, &[("game.tsx", &src(body))]);
    let mut app = test_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed, "expected a failed load");
    r.errors
}

fn ts(app: &mut App, component: &str, hp: serde_json::Value) -> Entity {
    spawn_ts(app, component, hp, ())
}

fn has(app: &App, e: Entity, component: &str) -> bool {
    read_ts(app, e, component).is_some()
}

/// Log lines starting with `prefix`, in order, `console.*` output pumped first.
fn lines(app: &mut App, prefix: &str) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|l| l.starts_with(prefix)).collect()
}

fn bits(es: &[Entity]) -> String {
    let mut v: Vec<u64> = es.iter().map(|e| e.to_bits()).collect();
    v.sort();
    v.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

/// Every entity with a TS component.
fn with(app: &mut App, component: &str) -> Vec<Entity> {
    let id = bevy_ts::plugin_api::component_by_name(app.world(), component).unwrap().0;
    let world = app.world_mut();
    QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).collect()
}

// ---------------------------------------------------------------------------------------------
// 1. Fixes and tests for what exists

#[test]
fn sparse_set_data_is_refused() {
    let e = errors("ecsq-spark", "export function s(q: Query<[Spark]>) {}\napp.addSystems(Update, s);\n");
    assert!(e.iter().any(|e| e.message.contains("`Spark` is a sparse-set component; it can be a filter but not query data")), "{e:?}");
    assert!(e.iter().all(|e| e.location.as_ref().is_some_and(|l| l.file.ends_with("game.tsx"))), "located: {e:?}");
    let e = errors("ecsq-spark-has", "export function s(q: Query<[Entity, Has<Tag>]>) {}\napp.addSystems(Update, s);\n");
    assert!(e.iter().any(|e| e.message.contains("sparse-set filters need the sparse-set query path")), "{e:?}");
    // A declined item (ecs-coverage): not query data, named, located.
    let e = errors("ecsq-declined", "export function s(q: Query<[SpawnDetails]>) {}\napp.addSystems(Update, s);\n");
    assert!(e.iter().any(|e| e.message.contains("SpawnDetails is not query data")), "{e:?}");
    assert!(e.iter().all(|e| e.location.is_some()), "{e:?}");
}

#[test]
fn probe_disabled_and_sparse_filters() {
    let body = r#"export function probe(all: Query<[Entity, Plain]>, off: Query<[Entity, Plain], With<Disabled>>, both: Query<[Entity, Plain, Has<Disabled>]>) {
  console.log(`probe ${all.len} ${off.len} ${both.len}`);
}
app.addSystems(PostUpdate, probe);
"#;
    let (_, mut app, _) = staged(
        "ecsq-probe",
        |app| {
            let es: Vec<Entity> = (0..4).map(|_| ts(app, "game::Plain", json!({}))).collect();
            app.world_mut().entity_mut(es[1]).insert(bevy::ecs::entity_disabling::Disabled);
            app.world_mut().entity_mut(es[2]).insert(Tag);
            es
        },
        body,
    );
    updates(&mut app, 3);
    assert_eq!(lines(&mut app, "probe"), vec!["probe 3 1 4"; 3]);
    // Sparse-set filters are refused at load until queries take the sparse-set path.
    for (i, f) in ["With<Tag>", "Without<Tag>", "Changed<Tag>", "Or<[With<Tag>, Added<Plain>]>"].iter().enumerate() {
        let e = errors(&format!("ecsq-probe-sparse-{i}"), &format!("export function s(q: Query<[Entity, Plain], {f}>) {{}}\napp.addSystems(Update, s);\n"));
        assert!(e.iter().any(|e| e.message.contains("sparse-set filters need the sparse-set query path")), "{f}: {e:?}");
    }
}

#[test]
fn despawn_takes_children() {
    let (_, mut app, es) = staged(
        "ecsq-despawn",
        |app| {
            let a = app.world_mut().spawn_empty().id();
            let b = app.world_mut().spawn(ChildOf(a)).id();
            vec![a, b]
        },
        "let done = false;\nexport function kill(c: Commands) { if (done) return; done = true; c.entity($0).despawn(); }\napp.addSystems(Update, kill);\n",
    );
    app.update();
    assert!(app.world().get_entity(es[0]).is_err());
    assert!(app.world().get_entity(es[1]).is_err(), "the child went with its parent");
}

#[test]
fn remove_takes_only_named() {
    let (_, mut app, es) = staged(
        "ecsq-remove",
        |app| {
            let e = ts(app, "game::Health", json!({}));
            insert_ts(app, e, "game::Shield", json!({}));
            vec![e]
        },
        "export function strip(c: Commands) { c.entity($0).remove(Shield); }\napp.addSystems(Update, strip);\n",
    );
    app.update();
    assert!(has(&app, es[0], "game::Health"));
    assert!(!has(&app, es[0], "game::Shield"));
}

#[test]
fn entity_event_bubbles_until_stopped() {
    let (_, mut app, es) = staged(
        "ecsq-bubble",
        |app| {
            let a = app.world_mut().spawn_empty().id();
            let b = app.world_mut().spawn(ChildOf(a)).id();
            let c = app.world_mut().spawn(ChildOf(b)).id();
            vec![a, b, c]
        },
        r#"let done = false;
export function hit(c: Commands) { if (done) return; done = true; c.trigger(Hit.of({ entity: $2, n: 1 })); }
export function onHit(on: On<Hit>) { console.log(`hit ${on.entity}`); if (on.entity === $1) on.propagate(false); }
app.addSystems(Update, hit).addObserver(onHit);
"#,
    );
    updates(&mut app, 2);
    assert_eq!(lines(&mut app, "hit"), vec![format!("hit {}", es[2].to_bits()), format!("hit {}", es[1].to_bits())]);
}

// ---------------------------------------------------------------------------------------------
// 2. Filters and query data

#[test]
fn or_mixes_archetype_and_row_terms() {
    let body = r#"export function rows(q: Query<[Entity], Or<[Changed<Health>, Added<Shield>]>>, p: Query<[Entity], Or<[With<Player>, Changed<Health>]>>) {
  console.log(`or ${[...q].map(([e]) => e).sort()} | ${[...p].map(([e]) => e).sort()}`);
}
app.addSystems(PostUpdate, rows);
"#;
    let (_, mut app, es) = staged(
        "ecsq-or",
        |app| {
            let a = ts(app, "game::Health", json!({}));
            let b = ts(app, "game::Health", json!({}));
            let c = ts(app, "game::Health", json!({}));
            let p = ts(app, "game::Player", json!({}));
            vec![a, b, c, p]
        },
        body,
    );
    app.update();
    // Since the last run: a's Health changed, b got a Shield; c and p are untouched.
    insert_ts(&mut app, es[0], "game::Health", json!({ "hp": 5 }));
    insert_ts(&mut app, es[1], "game::Shield", json!({}));
    app.update();
    let want = format!("or {} | {}", bits(&[es[0], es[1]]), bits(&[es[0], es[3]]));
    assert_eq!(lines(&mut app, "or").last().unwrap(), &want);
}

#[test]
fn has_is_a_bool_and_never_filters() {
    let (_, mut app, es) = staged(
        "ecsq-has",
        |app| {
            let es: Vec<Entity> = (0..3).map(|_| ts(app, "game::Plain", json!({}))).collect();
            insert_ts(app, es[1], "game::Frozen", json!({}));
            es
        },
        "export function h(q: Query<[Entity, Has<Frozen>], With<Plain>>) { const r: string[] = []; q.each((e, f) => r.push(`${e}:${f}`)); console.log(`has ${r.sort().join(' ')}`); }\napp.addSystems(Update, h);\n",
    );
    app.update();
    let mut want: Vec<String> = es.iter().enumerate().map(|(i, e)| format!("{}:{}", e.to_bits(), i == 1)).collect();
    want.sort();
    assert_eq!(lines(&mut app, "has"), vec![format!("has {}", want.join(" "))]);
}

#[test]
fn spawned_sees_new_entities_once() {
    let mut app = loaded(
        "ecsq-spawned",
        r#"let n = 0;
export function make(c: Commands) { if (n++ === 0) { c.spawn(Enemy); c.spawn(Enemy); } }
export function fresh(q: Query<[Entity], [With<Enemy>, Spawned]>) { console.log(`fresh ${q.len}`); }
app.addSystems(Update, make).addSystems(PostUpdate, fresh);
"#,
    );
    updates(&mut app, 3);
    assert_eq!(lines(&mut app, "fresh"), vec!["fresh 2", "fresh 0", "fresh 0"]);
}

#[test]
fn any_of_needs_one_part() {
    let (_, mut app, _) = staged(
        "ecsq-anyof",
        |app| {
            ts(app, "game::Health", json!({ "hp": 1 }));
            ts(app, "game::Shield", json!({ "v": 2 }));
            let both = ts(app, "game::Health", json!({ "hp": 3 }));
            insert_ts(app, both, "game::Shield", json!({ "v": 4 }));
            ts(app, "game::Plain", json!({}));
            vec![]
        },
        r#"export function any(q: Query<[AnyOf<[Health, Shield]>]>) {
  const r: string[] = [];
  for (const [[h, s]] of q) r.push(`${h ? h.hp : "-"}/${s ? s.v : "-"}`);
  console.log(`any ${r.sort().join(" ")}`);
}
app.addSystems(Update, any);
"#,
    );
    app.update();
    assert_eq!(lines(&mut app, "any"), vec!["any -/2 1/- 3/4"]);
}

#[test]
fn ref_reports_ticks_and_refuses_writes() {
    let (_, mut app, es) = staged(
        "ecsq-ref",
        |app| (0..3).map(|_| ts(app, "game::Health", json!({}))).collect(),
        r#"export function watch(q: Query<[Entity, Ref<Health>]>) {
  const changed: number[] = [];
  const added: number[] = [];
  let refused = 0;
  q.each((e, h) => {
    if (h.isChanged()) changed.push(e);
    if (h.isAdded()) added.push(e);
    if (typeof h.lastChanged() !== "number") throw new Error("lastChanged");
    try { (h as any).hp = 1; } catch { refused++; }
  });
  console.log(`ref ${changed.sort()} | ${added.sort()} | ${refused}`);
}
app.addSystems(PostUpdate, watch);
"#,
    );
    app.update();
    insert_ts(&mut app, es[1], "game::Health", json!({ "hp": 7 }));
    app.update();
    let all = bits(&es);
    assert_eq!(lines(&mut app, "ref"), vec![format!("ref {all} | {all} | 3"), format!("ref {} |  | 3", es[1].to_bits())]);
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 100.0, "the write through Ref did not land");
}

// ---------------------------------------------------------------------------------------------
// 3. Params and query methods

#[test]
fn single_skips_without_exactly_one() {
    let mut app = loaded(
        "ecsq-single",
        "export function move(player: Single<[Mut<Transform>], With<Player>>) { console.log('single'); player[0].translation.x = 5; }\napp.addSystems(Update, move);\n",
    );
    let before = calls(&app);
    updates(&mut app, 2);
    assert_eq!(calls(&app), before, "no player: no call into JS");
    assert!(lines(&mut app, "single").is_empty());
    assert!(!messages(&app).iter().any(|m| m.contains("threw")), "{:?}", messages(&app));
    let p = spawn_ts(&mut app, "game::Player", json!({}), Transform::default());
    app.update();
    assert_eq!(app.world().get::<Transform>(p).unwrap().translation.x, 5.0);
    spawn_ts(&mut app, "game::Player", json!({}), Transform::default());
    let before = calls(&app);
    app.update();
    assert_eq!(calls(&app), before, "two players: skipped");

    let mut app = loaded(
        "ecsq-single-opt",
        "export function maybe(player: Option<Single<[Entity], With<Player>>>) { console.log(`maybe ${player === undefined ? 'none' : 'one'}`); }\napp.addSystems(Update, maybe);\n",
    );
    app.update();
    ts(&mut app, "game::Player", json!({}));
    app.update();
    ts(&mut app, "game::Player", json!({}));
    app.update();
    assert_eq!(lines(&mut app, "maybe"), vec!["maybe none", "maybe one"]);
}

#[test]
fn populated_skips_when_empty() {
    let mut app = loaded("ecsq-populated", "export function horde(q: Populated<[Enemy]>) { console.log(`horde ${q.len}`); }\napp.addSystems(Update, horde);\n");
    let before = calls(&app);
    app.update();
    assert_eq!(calls(&app), before, "no enemies: no call into JS");
    ts(&mut app, "game::Enemy", json!({}));
    app.update();
    assert_eq!(lines(&mut app, "horde"), vec!["horde 1"]);
}

#[test]
fn removed_components_sees_remove_and_despawn_once() {
    let body = r#"let done = false;
export function cut(c: Commands) { if (done) return; done = true; c.entity($0).remove(Health); c.entity($1).despawn(); }
export function gone(r: RemovedComponents<Health>) { const s: number[] = []; r.each((e) => s.push(e)); console.log(`gone ${s.sort()} ${r.len} ${r.isEmpty()}`); }
app.addSystems(Update, cut).addSystems(PostUpdate, gone);
"#;
    let (dir, mut app, es) = staged("ecsq-removed", |app| (0..2).map(|_| ts(app, "game::Health", json!({}))).collect(), body);
    app.update();
    // A reload between runs keeps the cursor: the two are not seen again.
    write(&dir, "game.tsx", &src(&format!("// reloaded\n{}", body.replace("$0", &es[0].to_bits().to_string()).replace("$1", &es[1].to_bits().to_string()))));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app.update();
    assert_eq!(lines(&mut app, "gone"), vec![format!("gone {} 2 false", bits(&es)), "gone  0 true".to_string()]);
}

#[test]
fn system_change_tick_last_run_is_previous_this_run() {
    let mut app = loaded("ecsq-ticks", "export function t(ticks: SystemChangeTick) { console.log(`ticks ${ticks.thisRun} ${ticks.lastRun}`); }\napp.addSystems(Update, t);\n");
    updates(&mut app, 2);
    let l = lines(&mut app, "ticks");
    let first: Vec<&str> = l[0].split(' ').collect();
    let second: Vec<&str> = l[1].split(' ').collect();
    assert_eq!(second[2], first[1], "{l:?}");
    assert!(second[1].parse::<u32>().unwrap() > first[1].parse::<u32>().unwrap());
}

#[test]
fn contains_and_is_empty() {
    let (_, mut app, _) = staged(
        "ecsq-contains",
        |app| vec![ts(app, "game::Enemy", json!({})), ts(app, "game::Plain", json!({}))],
        "export function c(e: Query<[Entity], With<Enemy>>, p: Query<[Entity], With<Player>>) { console.log(`contains ${e.contains($0)} ${e.contains($1)} ${e.isEmpty()} ${p.isEmpty()}`); }\napp.addSystems(Update, c);\n",
    );
    app.update();
    assert_eq!(lines(&mut app, "contains"), vec!["contains true false false true"]);
}

#[test]
fn iter_many_and_get_many() {
    let (_, mut app, es) = staged(
        "ecsq-many",
        |app| vec![ts(app, "game::Health", json!({ "hp": 1 })), ts(app, "game::Plain", json!({})), ts(app, "game::Health", json!({ "hp": 3 }))],
        r#"export function m(q: Query<[Entity, Health]>) {
  console.log(`many ${[...q.iterMany([$2, $1, $0])].map(([e, h]) => `${e}:${h.hp}`).join(" ")}`);
  console.log(`got ${q.getMany([$2, $0]).map(([, h]) => h.hp).join(" ")}`);
  try { q.getMany([$2, $1, $0]); } catch (err) { console.log(`miss ${err}`); }
}
app.addSystems(Update, m);
"#,
    );
    app.update();
    assert_eq!(lines(&mut app, "many"), vec![format!("many {}:3 {}:1", es[2].to_bits(), es[0].to_bits())]);
    assert_eq!(lines(&mut app, "got"), vec!["got 3 1"]);
    let miss = lines(&mut app, "miss");
    assert!(miss.len() == 1 && miss[0].contains(&es[1].to_bits().to_string()), "{miss:?}");
}

#[test]
fn iter_combinations_yields_each_set_once() {
    let (_, mut app, _) = staged(
        "ecsq-pairs",
        |app| (0..4).map(|_| ts(app, "game::Enemy", json!({}))).collect(),
        r#"export function pairs(q: Query<[Entity], With<Enemy>>) {
  const seen = new Set<string>();
  let n = 0;
  for (const [[a], [b]] of q.iterCombinations(2)) { n++; if (a !== b) seen.add([a, b].sort().join("-")); }
  console.log(`pairs ${n} ${seen.size}`);
}
app.addSystems(Update, pairs);
"#,
    );
    app.update();
    assert_eq!(lines(&mut app, "pairs"), vec!["pairs 6 6"]);
}

/// `iterDescendants` waits on `Children` as query data (reflect-coverage stage 3; task 6.1).
#[test]
fn iter_ancestors_and_descendants() {
    let (_, mut app, es) = staged(
        "ecsq-ancestors",
        |app| {
            let a = app.world_mut().spawn_empty().id();
            let b = app.world_mut().spawn(ChildOf(a)).id();
            let c = app.world_mut().spawn(ChildOf(b)).id();
            vec![a, b, c]
        },
        "export function up(q: Query<[ChildOf]>) { console.log(`up ${[...q.iterAncestors($2)].join(' ')}`); }\napp.addSystems(Update, up);\n",
    );
    app.update();
    assert_eq!(lines(&mut app, "up"), vec![format!("up {} {}", es[1].to_bits(), es[0].to_bits())]);
}

// ---------------------------------------------------------------------------------------------
// 4. Lifecycle

#[test]
fn remove_observer_reads_the_value() {
    let mut app = loaded("ecsq-on-remove", "export function gone(on: On<Remove<Health>>) { console.log(`removed ${on.hp} ${on.entity}`); }\napp.addObserver(gone);\n");
    let e = ts(&mut app, "game::Health", json!({ "hp": 7 }));
    app.update();
    app.world_mut().despawn(e);
    app.update();
    assert_eq!(lines(&mut app, "removed"), vec![format!("removed 7 {}", e.to_bits())]);
}

#[test]
fn discard_sees_the_old_value() {
    let (_, mut app, es) = staged(
        "ecsq-discard",
        |app| vec![ts(app, "game::Health", json!({ "hp": 2 }))],
        r#"let done = false;
export function heal(c: Commands) { if (done) return; done = true; c.entity($0).insert(Health.of({ hp: 5 })); }
export function old(on: On<Discard<Health>>) { console.log(`discard ${on.hp}`); }
app.addSystems(Update, heal).addObserver(old);
"#,
    );
    updates(&mut app, 2);
    assert_eq!(lines(&mut app, "discard"), vec!["discard 2"]);
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 5.0);
}

#[test]
fn despawn_observers_run_in_bevy_order() {
    let mut app = loaded(
        "ecsq-order",
        r#"export function a(on: On<Remove<Health>>) { console.log(`order remove ${on.hp}`); }
export function b(on: On<Despawn<Health>>) { console.log(`order despawn ${on.hp}`); }
export function c(on: On<Discard<Health>>) { console.log(`order discard ${on.hp}`); }
app.addObserver(a).addObserver(b).addObserver(c);
"#,
    );
    let e = ts(&mut app, "game::Health", json!({ "hp": 4 }));
    app.update();
    app.world_mut().despawn(e);
    app.update();
    assert_eq!(lines(&mut app, "order"), vec!["order despawn 4", "order discard 4", "order remove 4"]);
}

// ---------------------------------------------------------------------------------------------
// 5. Commands

#[test]
fn a_thousand_spawns_in_one_run() {
    let mut app = loaded(
        "ecsq-thousand",
        "export function horde(c: Commands) { const ids = new Set<number>(); for (let i = 0; i < 1000; i++) ids.add(c.spawn(Enemy)); console.log(`ids ${ids.size}`); }\napp.addSystems(Startup, horde);\n",
    );
    app.update();
    assert_eq!(lines(&mut app, "ids"), vec!["ids 1000"]);
    assert_eq!(with(&mut app, "game::Enemy").len(), 1000);
    let reserves = app.world().non_send::<bevy_ts::runtime::ScriptRuntime>().reserve_calls;
    assert!((1..=4).contains(&reserves), "{reserves} refill calls");
}

#[test]
fn spawn_element_returns_its_root() {
    let mut app = loaded(
        "ecsq-spawn-el",
        r#"export function make(c: Commands) {
  const e = c.spawn(Transform.of({ translation: [1, 0, 0] }), <entity><mesh shape="cuboid" /><Health hp={3} /><entity name="tip" /></entity>);
  console.log(`root ${e}`);
  try { c.spawn(<><entity /><entity /></>); } catch (err) { console.log(`fragment ${err}`); }
  try { c.spawn(Health.of(), <entity><Health hp={1} /></entity>); } catch (err) { console.log(`twice ${err}`); }
}
app.addSystems(Startup, make);
"#,
    );
    app.update();
    let root = lines(&mut app, "root");
    let e = Entity::from_bits(root[0].trim_start_matches("root ").parse().unwrap());
    assert_eq!(app.world().get::<Transform>(e).unwrap().translation.x, 1.0);
    assert!(app.world().get::<Mesh3d>(e).is_some());
    assert_eq!(read_ts(&app, e, "game::Health").unwrap()["hp"], 3.0);
    assert!(app.world().get::<ChildOf>(e).is_none(), "no parent from the spawn");
    let children: Vec<Entity> = app.world().get::<Children>(e).unwrap().iter().collect();
    assert_eq!(children.len(), 1);
    assert_eq!(app.world().get::<Name>(children[0]).unwrap().as_str(), "tip");
    assert!(lines(&mut app, "fragment")[0].contains("<entity>"), "{:?}", messages(&app));
    assert!(lines(&mut app, "twice")[0].contains("game::Health"), "{:?}", messages(&app));
    assert_eq!(with(&mut app, "game::Health"), vec![e], "nothing else was spawned");
}

#[test]
fn insert_element_lands_on_the_entity() {
    let (_, mut app, es) = staged(
        "ecsq-insert-el",
        |app| vec![app.world_mut().spawn(Transform::default()).id()],
        "let done = false;\nexport function put(c: Commands) { if (done) return; done = true; c.entity($0).insert(<entity><Health hp={5} /><entity name=\"tip\" /></entity>); }\napp.addSystems(Update, put);\n",
    );
    app.update();
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 5.0);
    let children: Vec<Entity> = app.world().get::<Children>(es[0]).unwrap().iter().collect();
    assert_eq!(children.len(), 1);
    assert_eq!(app.world().get::<Name>(children[0]).unwrap().as_str(), "tip");
}

#[test]
fn spawn_batch_returns_ids_in_order() {
    let mut app = loaded(
        "ecsq-spawn-batch",
        "export function many(c: Commands) { console.log(`batch ${c.spawnBatch([[Enemy.of()], [Enemy.of(), Player.of()]]).join(' ')}`); }\napp.addSystems(Startup, many);\n",
    );
    app.update();
    let l = lines(&mut app, "batch");
    let ids: Vec<Entity> = l[0].trim_start_matches("batch ").split(' ').map(|s| Entity::from_bits(s.parse().unwrap())).collect();
    assert_eq!(ids.len(), 2);
    assert!(has(&app, ids[0], "game::Enemy") && !has(&app, ids[0], "game::Player"));
    assert!(has(&app, ids[1], "game::Enemy") && has(&app, ids[1], "game::Player"));
}

#[test]
fn insert_batch_inserts_each() {
    let (_, mut app, es) = staged(
        "ecsq-insert-batch",
        |app| (0..2).map(|_| app.world_mut().spawn_empty().id()).collect(),
        "export function each(c: Commands) { c.insertBatch([[$0, Health.of({ hp: 1 })], [$1, Shield.of(), Enemy]]); }\napp.addSystems(Update, each);\n",
    );
    app.update();
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 1.0);
    assert!(has(&app, es[1], "game::Shield") && has(&app, es[1], "game::Enemy"));
}

#[test]
fn insert_if_new_and_try_insert() {
    let (_, mut app, es) = staged(
        "ecsq-if-new",
        |app| vec![ts(app, "game::Health", json!({ "hp": 3 })), app.world_mut().spawn_empty().id()],
        r#"let done = false;
export function put(c: Commands) {
  if (done) return;
  done = true;
  c.entity($0).insertIfNew(Health.of({ hp: 9 }), Shield.of());
  c.entity($1).despawn();
  c.entity($1).tryInsert(Shield.of());
}
app.addSystems(Update, put);
"#,
    );
    app.update();
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 3.0);
    assert!(has(&app, es[0], "game::Shield"));
    assert!(!messages(&app).iter().any(|m| m.contains("does not exist")), "tryInsert on a gone entity is silent: {:?}", messages(&app));
}

#[test]
fn retain_and_clear() {
    let (_, mut app, es) = staged(
        "ecsq-retain",
        |app| {
            let e = spawn_ts(app, "game::Health", json!({}), Transform::default());
            insert_ts(app, e, "game::Shield", json!({}));
            let f = spawn_ts(app, "game::Health", json!({}), Transform::default());
            vec![e, f]
        },
        r#"let done = false;
export function trim(c: Commands) {
  if (done) return;
  done = true;
  c.entity($0).retain(Transform);
  c.entity($1).clear();
  const g = c.spawn(Health.of(), Transform);
  c.entity(g).clear();
  const h = c.spawn(Health.of(), Transform);
  c.entity(h).retain(Transform);
  console.log(`made ${g} ${h}`);
}
app.addSystems(Update, trim);
"#,
    );
    app.update();
    assert!(app.world().get::<Transform>(es[0]).is_some());
    assert!(!has(&app, es[0], "game::Health") && !has(&app, es[0], "game::Shield"));
    assert!(app.world().get_entity(es[1]).is_ok(), "clear keeps the entity");
    assert_eq!(app.world().entity(es[1]).archetype().component_count(), 0);
    // A TS spawn keeps bevy-ts's mark through clear and retain, so a restart still despawns it.
    let made: Vec<Entity> = lines(&mut app, "made")[0].trim_start_matches("made ").split(' ').map(|s| Entity::from_bits(s.parse().unwrap())).collect();
    for e in &made {
        assert!(app.world().get::<bevy_ts::commands::TsSpawned>(*e).is_some(), "{e} kept TsSpawned");
    }
    assert!(!has(&app, made[0], "game::Health") && app.world().get::<Transform>(made[1]).is_some());
    reload(&mut app, true);
    assert!(made.iter().all(|e| app.world().get_entity(*e).is_err()), "gone after a restart");
}

#[test]
fn remove_with_requires() {
    let (_, mut app, es) = staged(
        "ecsq-requires",
        |app| vec![app.world_mut().spawn(Knight::default()).id(), app.world_mut().spawn(Knight::default()).id()],
        "export function strip(c: Commands) { c.entity($0).removeWithRequires(Knight); c.entity($1).remove(Knight); }\napp.addSystems(Update, strip);\n",
    );
    assert!(app.world().get::<Armor>(es[0]).is_some(), "Knight requires Armor");
    app.update();
    assert!(app.world().get::<Knight>(es[0]).is_none() && app.world().get::<Armor>(es[0]).is_none());
    assert!(app.world().get::<Knight>(es[1]).is_none() && app.world().get::<Armor>(es[1]).is_some(), "remove keeps what it required");
}

#[test]
fn clone_and_spawn_copies_components() {
    let (_, mut app, es) = staged(
        "ecsq-clone",
        |app| vec![ts(app, "game::Health", json!({ "hp": 4 }))],
        r#"app.scene("s", () => <entity name="src"><Shield v={2} /></entity>);
let done = false;
export function copy(c: Commands, scene: Query<[Entity], With<Shield>>) {
  if (done) return;
  done = true;
  const [[src]] = [...scene];
  console.log(`clone ${c.entity($0).cloneAndSpawn()} ${c.entity(src).cloneAndSpawn()}`);
}
app.addSystems(Update, copy);
"#,
    );
    app.update();
    let clones: Vec<Entity> = lines(&mut app, "clone")[0].trim_start_matches("clone ").split(' ').map(|s| Entity::from_bits(s.parse().unwrap())).collect();
    let (c, t) = (clones[0], clones[1]);
    assert_ne!(c, es[0]);
    assert_eq!(read_ts(&app, c, "game::Health").unwrap()["hp"], 4.0);
    // Changing one leaves the other.
    insert_ts(&mut app, c, "game::Health", json!({ "hp": 9 }));
    assert_eq!(read_ts(&app, es[0], "game::Health").unwrap()["hp"], 4.0);
    // A clone of a tsx-made entity is not a tsx node: only the original is in the tree.
    assert_eq!(read_ts(&app, t, "game::Shield").unwrap()["v"], 2.0);
    assert!(app.world().get::<bevy_ts_tsx::TsxNode>(t).is_none());
    // A clone is a TS spawn, whatever made its source: a restart despawns it, not the source.
    assert!(app.world().get::<bevy_ts::commands::TsSpawned>(c).is_some());
    reload(&mut app, true);
    assert!(app.world().get_entity(c).is_err() && app.world().get_entity(t).is_err(), "clones gone after a restart");
    assert!(app.world().get_entity(es[0]).is_ok());
}

#[test]
fn insert_and_remove_recursive() {
    let (_, mut app, es) = staged(
        "ecsq-recursive",
        |app| {
            let a = app.world_mut().spawn_empty().id();
            let b = app.world_mut().spawn(ChildOf(a)).id();
            let c = app.world_mut().spawn(ChildOf(b)).id();
            vec![a, b, c]
        },
        "let n = 0;\nexport function tree(c: Commands) { n++; if (n === 1) c.entity($0).insertRecursive(Frozen.of()); if (n === 2) c.entity($1).removeRecursive(Frozen); }\napp.addSystems(Update, tree);\n",
    );
    app.update();
    assert!(es.iter().all(|e| has(&app, *e, "game::Frozen")));
    app.update();
    assert!(has(&app, es[0], "game::Frozen"));
    assert!(!has(&app, es[1], "game::Frozen") && !has(&app, es[2], "game::Frozen"));
}

#[test]
fn children_commands() {
    let (_, mut app, es) = staged(
        "ecsq-children",
        |app| {
            let p = app.world_mut().spawn_empty().id();
            let x = app.world_mut().spawn_empty().id();
            let q = app.world_mut().spawn_empty().id();
            let y = app.world_mut().spawn(ChildOf(q)).id();
            let r = app.world_mut().spawn_empty().id();
            let z = app.world_mut().spawn(ChildOf(r)).id();
            vec![p, x, q, y, r, z]
        },
        r#"let done = false;
export function kids(c: Commands) {
  if (done) return;
  done = true;
  c.entity($0).withChildren((parent) => { parent.spawn(Health.of()); parent.spawn(Health.of()); });
  c.entity($0).addChildren([$1]);
  c.entity($2).detachChildren();
  c.entity($4).despawnChildren();
}
app.addSystems(Update, kids);
"#,
    );
    app.update();
    let children: Vec<Entity> = app.world().get::<Children>(es[0]).unwrap().iter().collect();
    assert_eq!(children.len(), 3);
    assert_eq!(children.iter().filter(|c| has(&app, **c, "game::Health")).count(), 2);
    assert!(children.contains(&es[1]));
    assert!(app.world().get::<ChildOf>(es[3]).is_none() && app.world().get_entity(es[3]).is_ok(), "detached, alive");
    assert!(app.world().get_entity(es[5]).is_err(), "despawned with despawnChildren");
    assert!(app.world().get_entity(es[4]).is_ok());
}
