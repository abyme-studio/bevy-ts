//! Systems on the ruled surface: queries, change detection, iteration, ordering, commands and the
//! load errors the compile step and runtime report.

use crate::common;

use bevy::prelude::*;
use common::*;
use serde_json::json;

const HEAD: &str = r#"import { app, chain, Component, f32, PostUpdate, Startup, systems, systemSet, SystemSet, Transform, Update, vec3 } from "bevy-ts";
import type { Added, Changed, Commands, Entity, Local, Mut, Option, Query, Res, ResMut, With, Without } from "bevy-ts";
export class Mover extends Component { static path = "game::Mover"; speed: f32 = 1; amplitude: f32 = 2 }
export class Marked extends Component { static path = "game::Marked" }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

fn movers(app: &mut App, n: usize) -> Vec<Entity> {
    (0..n).map(|i| spawn_ts(app, "game::Mover", json!({}), Transform::from_xyz(i as f32, 0.0, 0.0))).collect()
}

/// Loads `HEAD` (to register the components), spawns `n` movers, then reloads with `body`.
fn with_movers(name: &str, n: usize, body: &str) -> (App, Vec<Entity>) {
    let (dir, mut app) = load(name, &src(""));
    let es = movers(&mut app, n);
    write(&dir, "game.ts", &src(body));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    (app, es)
}

#[test]
fn ts_writes_reach_rust_and_only_written_rows_are_changed() {
    let (dir, mut app) = load("changed", &src(""));
    let es = movers(&mut app, 10_000);
    let picks = [es[5], es[500], es[9999]].map(|e| e.to_bits());
    write(
        &dir,
        "game.ts",
        &src(&format!(
            "const picks = new Set([{}, {}, {}]);\nexport function pick(q: Query<[Entity, Mut<Mover>]>) {{ q.each((e, m) => {{ if (picks.has(e)) m.speed = 3; }}); }}\napp.addSystems(Update, pick);\n",
            picks[0], picks[1], picks[2]
        )),
    );
    reload(&mut app, false);
    let since = tick(&mut app);
    app.update();
    let mut want = vec![es[5], es[500], es[9999]];
    want.sort();
    assert_eq!(changed_since(&mut app, "game::Mover", since), want);
    assert_eq!(read_ts(&app, es[500], "game::Mover").unwrap()["speed"], 3.0);
    assert_eq!(read_ts(&app, es[501], "game::Mover").unwrap()["speed"], 1.0);
}

#[test]
fn read_data_throws_on_write_and_is_never_marked() {
    let (mut app, _) = with_movers("readonly", 3, "export function ro(q: Query<[Mover]>) { q.each((m) => { (m as any).speed = 9; }); }\napp.addSystems(Update, ro);\n");
    let since = tick(&mut app);
    updates(&mut app, 2);
    assert!(changed_since(&mut app, "game::Mover", since).is_empty());
    assert!(messages(&app).iter().any(|l| l.contains("read-only")), "{:?}", messages(&app));
}

#[test]
fn writing_a_read_column_through_raw_arrays_warns_once() {
    let (mut app, _) = with_movers(
        "sneaky",
        3,
        "export function sneaky(q: Query<[Mover]>) { for (const t of q.tables) (t.read(Mover) as any).f32[0] += 1; }\napp.addSystems(Update, sneaky);\n",
    );
    updates(&mut app, 5);
    let n = messages(&app).iter().filter(|m| m.contains("wrote to game::Mover, which no query of it declares Mut")).count();
    assert_eq!(n, 1, "{:?}", messages(&app));
}

#[test]
fn split_marks_only_ticked_rows_and_write_marks_the_table() {
    let (mut app, es) = with_movers(
        "split",
        10,
        r#"export function viaSplit(q: Query<[Mut<Mover>]>) {
  for (const t of q.tables) {
    const s = t.split(Mover);
    const stride = s.size / 4, off = s.offsets.speed / 4;
    s.f32![3 * stride + off] = 30;
    s.f32![7 * stride + off] = 70;
    s.ticks[3] = q.tick;
    s.ticks[7] = q.tick;
  }
}
app.addSystems(Update, viaSplit);
"#,
    );
    let since = tick(&mut app);
    app.update();
    let mut want = vec![es[3], es[7]];
    want.sort();
    assert_eq!(changed_since(&mut app, "game::Mover", since), want);

    let (mut app, es) = with_movers(
        "write",
        10,
        r#"export function a(q: Query<[Mut<Mover>]>) { q.each((m) => { m.amplitude = m.speed * 2; }); }
export function b(q: Query<[Mut<Mover>]>) {
  for (const t of q.tables) {
    const w = t.write(Mover), st = w.size / 4;
    for (let r = 0; r < t.len; r++) w.f32![r * st + w.offsets.speed / 4] = w.f32![r * st + w.offsets.amplitude / 4] / 2 + 1;
  }
}
app.addSystems(Update, chain(a, b));
"#,
    );
    let since = tick(&mut app);
    app.update();
    assert_eq!(changed_since(&mut app, "game::Mover", since).len(), 10);
    assert_eq!(read_ts(&app, es[0], "game::Mover").unwrap()["speed"], 2.0);
}

#[test]
fn one_call_into_js_per_run_over_four_tables() {
    let body = r#"export class A extends Component { static path = "t::A" }
export class B extends Component { static path = "t::B" }
export function count(q: Query<[Mover, Mut<Transform>]>) {
  let n = 0;
  q.each((m, t) => { n++; t.translation.y = m.speed; });
  if (q.tables.length !== 4 || n !== 10000 || q.len !== 10000) throw new Error(`${q.tables.length} ${n}`);
}
app.addSystems(Update, count);
"#;
    let (dir, mut app) = load("calls", &src(body.replace("app.addSystems(Update, count);", "").as_str()));
    let es = movers(&mut app, 10_000);
    for (i, e) in es.iter().enumerate() {
        if i % 4 == 1 || i % 4 == 3 {
            insert_ts(&mut app, *e, "t::A", json!({}));
        }
        if i % 4 >= 2 {
            insert_ts(&mut app, *e, "t::B", json!({}));
        }
    }
    write(&dir, "game.ts", &src(body));
    reload(&mut app, false);
    let before = calls(&app);
    app.update();
    assert_eq!(calls(&app) - before, 1);
    app.update();
    assert!(!messages(&app).iter().any(|m| m.contains("threw")), "{:?}", messages(&app));
    assert_eq!(app.world().get::<Transform>(es[1234]).unwrap().translation.y, 1.0);
}

#[test]
fn chain_after_before_sets_and_stages_order_systems() {
    let (mut app, es) = with_movers(
        "order",
        1,
        r#"export const Late = systemSet("game::Late");
export function second(q: Query<[Mover, Mut<Transform>]>) { q.each((m, t) => { t.translation.x = m.speed; }); }
export function first(q: Query<[Mut<Mover>]>) { q.each((m) => { m.speed = 5; }); }
export function third(q: Query<[Mut<Mover>]>) { q.each((m) => { m.amplitude = m.speed + 1; }); }
export function fourth(q: Query<[Mover, Mut<Transform>]>) { q.each((m, t) => { t.translation.z = m.amplitude; }); }
export function last(q: Query<[Mover, Mut<Transform>]>) { q.each((m, t) => { t.translation.y = m.speed * 10; }); }
app.addSystems(Update, systems(fourth).inSet(Late), chain(first, second, third).before(Late));
app.addSystems(PostUpdate, last);
"#,
    );
    app.update();
    let t = app.world().get::<Transform>(es[0]).unwrap();
    assert_eq!((t.translation.x, t.translation.z, t.translation.y), (5.0, 6.0, 50.0));
}

#[test]
fn overlapping_queries_alias_one_column() {
    let (mut app, _) = with_movers(
        "overlap",
        3,
        r#"export function separate(a: Query<[Entity, Mut<Transform>], With<Mover>>, b: Query<[Entity, Transform], With<Mover>>) {
  a.each((e, t) => { t.translation.y = 7; });
  b.each((e, t) => { if (t.translation.y !== 7) throw new Error("not aliased: " + t.translation.y); });
  console.log(`seen ${b.len}`);
}
app.addSystems(Update, separate);
"#,
    );
    updates(&mut app, 2);
    assert!(messages(&app).contains(&"seen 3".to_string()), "{:?}", messages(&app));
}

#[test]
fn option_entity_single_get_and_iteration() {
    let (dir, mut app) = load("rows", &src(""));
    let es = movers(&mut app, 3);
    insert_ts(&mut app, es[1], "game::Marked", json!({}));
    write(
        &dir,
        "game.ts",
        &src(&format!(
            r#"export function look(all: Query<[Entity, Mover, Option<Marked>]>, one: Query<[Entity], With<Marked>>, t: Query<[Mut<Transform>]>) {{
  const marked: number[] = [];
  for (const [e, m, mark] of all) if (mark) marked.push(e);
  const [only] = one.single();
  const row = t.get({target} as any);
  if (row) row[0].translation.set(1, 2, 3).add(vec3(1, 1, 1)).scale(2);
  console.log(`marked ${{marked.length}} ${{marked[0] === only}} ${{t.get(1e15 as any) === undefined}}`);
}}
app.addSystems(Update, look);
"#,
            target = es[2].to_bits()
        )),
    );
    reload(&mut app, false);
    updates(&mut app, 2);
    assert!(messages(&app).contains(&"marked 1 true true".to_string()), "{:?}", messages(&app));
    assert_eq!(app.world().get::<Transform>(es[2]).unwrap().translation, Vec3::new(4.0, 6.0, 8.0));
}

#[test]
fn changed_and_added_filters_visit_only_matching_rows() {
    let (mut app, es) = with_movers(
        "rows-changed",
        1000,
        r#"export function seen(q: Query<[Entity, Mover], Changed<Mover>>, fresh: Query<[Entity], Added<Marked>>) {
  console.log(`changed ${q.len} added ${fresh.len}`);
}
app.addSystems(PostUpdate, seen);
"#,
    );
    app.update();
    app.update();
    // Rust changes two movers and marks one entity between runs.
    let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Mover").unwrap().0;
    for e in [es[10], es[20]] {
        let mut em = app.world_mut().entity_mut(e);
        em.get_mut_by_id(id).unwrap().set_changed();
    }
    insert_ts(&mut app, es[30], "game::Marked", json!({}));
    // Console output is pumped in the next frame's `First`.
    updates(&mut app, 3);
    let m = messages(&app);
    let lines: Vec<&String> = m.iter().filter(|l| l.starts_with("changed")).collect();
    assert_eq!(lines[0], "changed 1000 added 0", "first run: every row is newer than a system that never ran");
    assert_eq!(lines[1], "changed 0 added 0");
    assert_eq!(lines[2], "changed 2 added 1", "{lines:?}");
    assert_eq!(lines[3], "changed 0 added 0");
}

#[test]
fn param_bundles_are_rebuilt_in_js() {
    let (mut app, es) = with_movers(
        "bundle",
        2,
        r#"interface Moving { movers: Query<[Mut<Mover>]>; places: Query<[Mut<Transform>], With<Mover>> }
export function bundled({ movers, places }: Moving) {
  movers.each((m) => { m.speed = 4; });
  places.each((t) => { t.translation.x = 9; });
}
app.addSystems(Update, bundled);
"#,
    );
    app.update();
    assert_eq!(read_ts(&app, es[1], "game::Mover").unwrap()["speed"], 4.0);
    assert_eq!(app.world().get::<Transform>(es[1]).unwrap().translation.x, 9.0);
}

#[test]
fn a_retained_view_reads_length_zero_after_the_run() {
    let (mut app, _) = with_movers(
        "retained",
        1,
        r#"let kept: Float32Array | null = null;
let ticks: Uint32Array | null = null;
export function keep(q: Query<[Mut<Transform>]>) {
  if (kept) console.log(`after: ${kept.length} ${ticks!.length}`);
  const t = q.tables[0].split(Transform);
  kept = t.f32!;
  ticks = t.ticks;
  console.log(`during: ${kept.length > 0}`);
}
app.addSystems(Update, keep);
"#,
    );
    updates(&mut app, 3);
    let m = messages(&app);
    assert!(m.contains(&"during: true".to_string()), "{m:?}");
    assert!(m.contains(&"after: 0 0".to_string()), "{m:?}");
}

#[test]
fn commands_spawn_parent_overflow_and_discard() {
    let (_, mut app) = load(
        "commands",
        &src(
            r#"export class Big extends Component { static path = "t::Big"; __FIELDS__ }
export function spawn(commands: Commands) {
  const parent = commands.spawn(Mover.of({ speed: 4 }), Transform);
  const child = commands.spawn(Mover);
  commands.entity(child).setParent(parent);
  for (let i = 0; i < 100; i++) commands.spawn(Big);
}
export function throws(commands: Commands) {
  commands.spawn(Mover);
  throw new Error("no");
}
app.addSystems(Startup, spawn, throws);
"#,
        )
        // 64 Vec4 fields: 1 KB a row, so 100 spawns overflow the 64 KB arena.
        .replace("__FIELDS__", &(0..64).map(|i| format!("declare v{i}: Vec4;")).collect::<String>()),
    );
    let id = bevy_ts::plugin_api::component_by_name(app.world(), "game::Mover").unwrap().0;
    let world = app.world_mut();
    let movers: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).collect();
    assert_eq!(movers.len(), 2, "the throwing startup's spawn is discarded");
    let parent = *movers.iter().find(|e| read_ts(&app, **e, "game::Mover").unwrap()["speed"] == 4.0).unwrap();
    let child = *movers.iter().find(|e| **e != parent).unwrap();
    assert_eq!(app.world().get::<ChildOf>(child).unwrap().parent(), parent);
    assert_eq!(app.world().get::<Transform>(parent), Some(&Transform::IDENTITY));
    let big = bevy_ts::plugin_api::component_by_name(app.world(), "t::Big").unwrap().0;
    let world = app.world_mut();
    assert_eq!(QueryBuilder::<Entity>::new(world).with_id(big).build().iter(world).count(), 100, "100 KB of spawns overflow the arena");
}

#[test]
fn load_errors_name_what_is_wrong() {
    let cases: &[(&str, &str, &str)] = &[
        ("string", "import { Resource } from \"bevy-ts\";\nexport class Title extends Resource { text: string = \"x\" }\n", "allowed on components only"),
        // A reflect-tier Rust component (`GlobalTransform`) is query data since reflect-coverage stage 3.
        (
            "cycle",
            "export function a(q: Query<[Mover]>) {}\nexport function b(q: Query<[Mover]>) {}\napp.addSystems(Update, systems(a).after(b), systems(b).after(a));\n",
            "cycle",
        ),
        ("unexported", "function hidden(q: Query<[Mover]>) {}\napp.addSystems(Update, hidden);\n", "not exported"),
        ("inline", "app.addSystems(Update, (() => {}) as any);\n", "compiled without bevy-ts"),
        ("computed", "declare function mk(): Query<[Mover]>;\nexport function c(q: ReturnType<typeof mk>) {}\napp.addSystems(Update, c);\n", "ReturnType"),
        ("time-mut", "import { Time } from \"bevy-ts\";\nexport function t(time: ResMut<Time>) {}\napp.addSystems(Update, t);\n", "ResMut<Time>"),
    ];
    for (name, body, want) in cases {
        let errors = load_errors(name, &src(body));
        assert!(errors.iter().any(|e| e.message.contains(want) || e.to_string().contains(want)), "{name}: {errors:?}");
        assert!(errors.iter().all(|e| e.location.is_some()), "{name}: located: {errors:?}");
    }
}

#[test]
fn a_type_only_import_a_param_needs_fails_the_compile() {
    let dir = project(
        "type-only",
        &[
            ("enemy.ts", "import { Component, f32 } from \"bevy-ts\";\nexport class Enemy extends Component(\"game::Enemy\", { speed: f32 }) {}\n"),
            (
                "game.ts",
                "import { app, Update } from \"bevy-ts\";\nimport type { Query } from \"bevy-ts\";\nimport type { Enemy } from \"./enemy\";\nexport function chase(q: Query<[Enemy]>) {}\napp.addSystems(Update, chase);\n",
            ),
        ],
    );
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("Enemy is imported as a type"), "{e:?}");
    assert_eq!(e.location.as_ref().unwrap().line, 3, "{e:?}");
}

#[test]
fn an_alias_from_another_file_declares_its_query() {
    let dir = project(
        "alias",
        &[
            (
                "enemies.ts",
                "import { Component, Transform } from \"bevy-ts\";\nimport type { f32, Mut, Query, With } from \"bevy-ts\";\nexport class Alive extends Component { static path = \"game::Alive\" }\nexport class Enemy extends Component { static path = \"game::Enemy\"; speed: f32 = 2 }\nexport type Movers<T> = Query<[T, Mut<Transform>], With<Alive>>;\n",
            ),
            (
                "game.ts",
                "import { app, Startup, Transform, Update } from \"bevy-ts\";\nimport type { Commands } from \"bevy-ts\";\nimport { Alive, Enemy, type Movers } from \"./enemies\";\nexport function spawn(c: Commands) { c.spawn(Enemy.of(), Transform, Alive.of()); c.spawn(Enemy.of(), Transform); }\nexport function move(q: Movers<Enemy>) { q.each((e, t) => { t.translation.y += e.speed; }); }\napp.addSystems(Startup, spawn).addSystems(Update, move);\n",
            ),
        ],
    );
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let mut ys: Vec<f32> = app.world_mut().query::<&Transform>().iter(app.world()).map(|t| t.translation.y).collect();
    ys.sort_by(f32::total_cmp);
    assert_eq!(ys, vec![0.0, 2.0], "only the Alive enemy moved");
}

#[test]
fn a_vector_method_stores_the_tick_once() {
    let (mut app, es) = with_movers(
        "marks-once",
        1,
        r#"export function push(q: Query<[Mut<Transform>]>) {
  q.each((t) => {
    const view = t.translation as any;
    const p = view.p, T = Object.getOwnPropertySymbols(p).find((s) => s.description === "ticks")!, real = p[T];
    let stores = 0;
    p[T] = new Proxy(real, { set(o: any, k: any, v: any) { stores++; o[k] = v; return true; } });
    view.addScaled(vec3(1, 2, 3), 2);
    const after = stores;
    view.x = 1;
    p[T] = real;
    console.log(`stores ${after} ${stores}`);
  });
}
app.addSystems(Update, push);
"#,
    );
    let since = tick(&mut app);
    updates(&mut app, 2);
    assert!(messages(&app).contains(&"stores 1 2".to_string()), "one store per method, one per setter: {:?}", messages(&app));
    assert_eq!(app.world().get::<Transform>(es[0]).unwrap().translation, Vec3::new(1.0, 8.0, 12.0), "two runs");
    let id = app.world_mut().register_component::<Transform>();
    let ticks = app.world().entity(es[0]).get_change_ticks_by_id(id).unwrap();
    assert!(ticks.is_changed(bevy::ecs::change_detection::Tick::new(since), app.world().read_change_tick()));
}

#[test]
fn fields_named_like_the_accessors_state_read_and_write() {
    // Every name the runtime keeps state under, or once did: the row, the run's tick, the tick
    // column, the typed views, a nested view's slot, a query row's presence and `On`'s own.
    let names = ["r", "k", "f32", "f64", "i8", "u8", "i16", "u16", "i32", "u32", "_t", "present"];
    let decl = |ty: &str| names.map(|n| format!("{n}: {}", if n == "f64" { "f64" } else { ty })).join("; ");
    let fill = names.iter().enumerate().map(|(i, n)| format!("o.{n} = {}", i + 1)).collect::<Vec<_>>().join("; ");
    let dump = names.map(|n| format!("${{o.{n}}}")).join(",");
    let body = format!(
        r#"import {{ app, Component, EntityEvent, Message, Resource, chain, f32, f64, u32, vec3, PostUpdate, Startup }} from "bevy-ts";
import type {{ Commands, Entity, Local, MessageReader, MessageWriter, Mut, On, Query, ResMut }} from "bevy-ts";
export class Slots extends Component {{ static path = "game::Slots"; t: vec3 = vec3(0, 0, 0); {c} }}
export class SlotsRes extends Resource {{ static path = "game::SlotsRes"; declare t: vec3; {r} }}
export class SlotsMsg extends Message {{ static path = "game::SlotsMsg"; declare t: f32; {r} }}
export class SlotsHit extends EntityEvent {{ static path = "game::SlotsHit"; static propagate = true; declare t: f32; declare stopped: f32; declare bind: f32 }}
const dump = (tag: string, o: any) => console.log(`${{tag}} ${{o.t.x ?? o.t}},${{o.t.y ?? ""}},${{o.t.z ?? ""}} {dump}`);
let frame = 0;
export function spawn(c: Commands) {{ c.spawn(Slots.of()); }}
export function writeAll(q: Query<[Mut<Slots>]>, res: ResMut<SlotsRes>, l: Local<{{ t: vec3; {lt} }}>, w: MessageWriter<SlotsMsg>) {{
  const f = frame++;
  if (f === 2) dump("local", l);
  if (f !== 1) return;
  q.each((o) => {{ o.t.set(7, 8, 9); {fill}; }});
  for (const o of [res, l] as any[]) {{ o.t = [7, 8, 9]; {fill}; }}
  const m = w.next() as any; m.t = 7; {fill_m};
}}
export function readAll(q: Query<[Entity, Slots]>, res: ResMut<SlotsRes>, r: MessageReader<SlotsMsg>, c: Commands) {{
  if (frame !== 2) return;
  q.each((e, o) => {{ dump("component", o); c.trigger(SlotsHit.of({{ entity: e, t: 1, stopped: 2, bind: 3 }})); }});
  dump("resource", res);
  r.each((m) => dump("message", m));
}}
export function onHit(h: On<SlotsHit>) {{ console.log(`event ${{h.t}},${{h.stopped}},${{h.bind}}`); h.propagate(false); }}
app.insertResource(SlotsRes.of()).addMessage(SlotsMsg).addSystems(Startup, spawn).addSystems(PostUpdate, chain(writeAll, readAll)).addObserver(onHit);
"#,
        c = decl("f32").split("; ").map(|f| format!("{f} = 0")).collect::<Vec<_>>().join("; "),
        r = decl("f32").split("; ").map(|f| format!("declare {f}")).collect::<Vec<_>>().join("; "),
        lt = decl("f32"),
        fill = fill,
        fill_m = fill.replace("o.", "m."),
        dump = dump,
    );
    let (_, mut app) = load("slot-names", &body);
    let since = tick(&mut app);
    updates(&mut app, 4);
    let m = messages(&app);
    let want = format!("7,8,9 {}", (1..=names.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
    for tag in ["component", "resource", "local"] {
        assert!(m.contains(&format!("{tag} {want}")), "{tag}: {m:?}");
    }
    let want_msg = format!("7,, {}", (1..=names.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
    assert!(m.contains(&format!("message {want_msg}")), "message: {m:?}");
    assert!(m.contains(&"event 1,2,3".to_string()), "event: {m:?}");
    let e = changed_since(&mut app, "game::Slots", since);
    assert_eq!(e.len(), 1, "the written row is changed");
    let v = read_ts(&app, e[0], "game::Slots").unwrap();
    assert_eq!(v["t"], json!([7.0, 8.0, 9.0]));
    for (i, n) in names.iter().enumerate() {
        assert_eq!(v[*n], json!((i + 1) as f64), "{n} in Rust");
    }
}
