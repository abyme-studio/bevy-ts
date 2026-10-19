mod common;

use bevy::ecs::world::FilteredEntityRef;
use bevy::prelude::*;
use bevy_ts::components::TsComponents;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::runtime::ScriptRuntime;
use common::*;

const MOVER: &str = r#"import { app, component, f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export const Mover = component("game::Mover", { speed: f32.default(1), amplitude: f32.default(2) });
"#;

fn spawn_movers(app: &mut App, n: usize) -> Vec<Entity> {
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let bytes = [1f32.to_le_bytes(), 2f32.to_le_bytes()].concat();
    (0..n)
        .map(|i| {
            let mut e = app.world_mut().spawn(Transform::from_xyz(i as f32, 0.0, 0.0));
            unsafe { bevy_ts::commands::insert_bytes(&mut e, id, &bytes) };
            e.id()
        })
        .collect()
}

fn speed_of(app: &App, e: Entity) -> f32 {
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let p = app.world().get_by_id(e, id).unwrap();
    f32::from_le_bytes(unsafe { std::slice::from_raw_parts(p.as_ptr(), 4) }.try_into().unwrap())
}

/// Entities whose `game::Mover` changed since `since`, through a Rust `QueryBuilder` query.
fn changed_movers(app: &mut App, since: u32) -> Vec<Entity> {
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let world = app.world_mut();
    let this_run = world.read_change_tick();
    let mut q = QueryBuilder::<(Entity, FilteredEntityRef)>::new(world).ref_id(id).build();
    q.iter(world)
        .filter(|(_, r)| r.get_change_ticks_by_id(id).unwrap().is_changed(bevy::ecs::change_detection::Tick::new(since), this_run))
        .map(|(e, _)| e)
        .collect()
}

fn calls(app: &App) -> u64 {
    app.world().non_send::<ScriptRuntime>().backend.calls_into_js()
}

#[test]
fn ts_writes_reach_rust_and_only_written_rows_are_changed() {
    let mut app = app();
    load(&mut app, &[("decl.js", MOVER)]);
    let es = spawn_movers(&mut app, 10_000);
    app.update();
    let src = format!(
        "{MOVER}app.system(\"pick\", {{ write: [Mover], run(q, ctx) {{ q.each((e, m) => {{ if (e === {a} || e === {b} || e === {c}) m.speed = 3; }}); }} }});\n",
        a = es[5].to_bits(),
        b = es[500].to_bits(),
        c = es[9999].to_bits()
    );
    load(&mut app, &[("decl.js", &src)]);
    let since = app.world_mut().read_change_tick().get();
    app.update();
    let mut changed = changed_movers(&mut app, since);
    changed.sort();
    let mut want = vec![es[5], es[500], es[9999]];
    want.sort();
    assert_eq!(changed, want);
    assert_eq!(speed_of(&app, es[500]), 3.0);
    assert_eq!(speed_of(&app, es[501]), 1.0);
}

#[test]
fn read_rows_throw_on_write_and_are_never_marked() {
    let mut app = app();
    load(&mut app, &[("decl.js", MOVER)]);
    let es = spawn_movers(&mut app, 3);
    let src = format!("{MOVER}app.system(\"ro\", {{ read: [Mover], run(q) {{ q.each((e, m) => {{ m.speed = 9; }}); }} }});\n");
    load(&mut app, &[("decl.js", &src)]);
    let since = app.world_mut().read_change_tick().get();
    updates(&mut app, 2);
    assert!(changed_movers(&mut app, since).is_empty());
    assert_eq!(speed_of(&app, es[0]), 9.0, "the data write lands before the setter throws");
    let m = messages(&app);
    assert!(m.iter().any(|l| l.contains("declared in `read`")), "{m:?}");
}

#[test]
fn writing_a_read_column_through_raw_arrays_warns_once() {
    let mut app = app();
    load(&mut app, &[("decl.js", MOVER)]);
    spawn_movers(&mut app, 3);
    let src = format!(
        "{MOVER}app.system(\"sneaky\", {{ read: [Mover], run(q) {{ for (const t of q.tables) t.read(Mover).f32[0] += 1; }} }});\n"
    );
    load(&mut app, &[("decl.js", &src)]);
    updates(&mut app, 5);
    let n = messages(&app).iter().filter(|m| m.contains("sneaky wrote to game::Mover")).count();
    assert_eq!(n, 1, "{:?}", messages(&app));
}

#[test]
fn each_write_and_split_agree_and_split_marks_only_ticked_rows() {
    let mut app = app();
    load(&mut app, &[("decl.js", MOVER)]);
    let es = spawn_movers(&mut app, 10);
    let src = format!(
        r#"{MOVER}
        app.system("viaTables", {{
          write: [Mover],
          run(q, ctx) {{
            for (const t of q.tables) {{
              const s = t.split(Mover);
              const stride = s.size / 4, off = s.offsets.speed / 4;
              s.f32[3 * stride + off] = 30;
              s.f32[7 * stride + off] = 70;
              s.ticks[3] = ctx.tick;
              s.ticks[7] = ctx.tick;
            }}
          }},
        }});
        "#
    );
    load(&mut app, &[("decl.js", &src)]);
    let since = app.world_mut().read_change_tick().get();
    app.update();
    let mut changed = changed_movers(&mut app, since);
    changed.sort();
    let mut want = vec![es[3], es[7]];
    want.sort();
    assert_eq!(changed, want);

    // `write(T)` marks the whole table; `each` and the raw arrays give the same values.
    let src = format!(
        r#"{MOVER}
        app.system("a", {{ write: [Mover], run(q, ctx) {{ q.each((e, m) => {{ m.amplitude = m.speed * 2; }}); }} }});
        app.system("b", {{ write: [Mover], run(q, ctx) {{
          for (const t of q.tables) {{
            const w = t.write(Mover), st = w.size / 4;
            for (let r = 0; r < t.len; r++) w.f32[r * st + w.offsets.speed / 4] = w.f32[r * st + w.offsets.amplitude / 4] / 2 + 1;
          }}
        }} }});
        "#
    );
    load(&mut app, &[("decl.js", &src)]);
    let since = app.world_mut().read_change_tick().get();
    app.update();
    assert_eq!(changed_movers(&mut app, since).len(), 10);
    assert_eq!(speed_of(&app, es[3]), 31.0);
    assert_eq!(speed_of(&app, es[0]), 2.0);
}

#[test]
fn one_call_into_js_per_run_over_four_tables() {
    let mut app = app();
    let decl = format!("{MOVER}export const A = component(\"t::A\", {{ x: f32 }});\nexport const B = component(\"t::B\", {{ x: f32 }});\n");
    load(&mut app, &[("decl.js", &decl)]);
    let es = spawn_movers(&mut app, 10_000);
    let (a, b) = {
        let ts = app.world().resource::<TsComponents>();
        (ts.live("t::A").unwrap().id, ts.live("t::B").unwrap().id)
    };
    for (i, e) in es.iter().enumerate() {
        let mut em = app.world_mut().entity_mut(*e);
        if i % 4 == 1 || i % 4 == 3 {
            unsafe { bevy_ts::commands::insert_bytes(&mut em, a, &0f32.to_le_bytes()) };
        }
        if i % 4 >= 2 {
            unsafe { bevy_ts::commands::insert_bytes(&mut em, b, &0f32.to_le_bytes()) };
        }
    }
    let src = format!(
        "{decl}app.system(\"count\", {{ write: [Mover, Transform], run(q) {{ let n = 0; q.each((e, m, t) => {{ n++; t.translation.y = m.speed; }}); if (q.tables.length !== 4 || n !== 10000) throw new Error(`${{q.tables.length}} ${{n}}`); }} }});\n"
    );
    load(&mut app, &[("decl.js", &src)]);
    let before = calls(&app);
    app.update();
    assert_eq!(calls(&app) - before, 1);
    app.update();
    assert!(!messages(&app).iter().any(|m| m.contains("threw")), "{:?}", messages(&app));
    assert_eq!(app.world().get::<Transform>(es[1234]).unwrap().translation.y, 1.0);
}

#[test]
fn systems_in_a_stage_see_earlier_writes_and_after_reorders() {
    let mut app = app();
    load(&mut app, &[("decl.js", MOVER)]);
    let es = spawn_movers(&mut app, 1);
    let src = format!(
        r#"{MOVER}
        app.system("second", {{ read: [Mover], write: [Transform], after: "first", run(q) {{ q.each((e, m, t) => {{ t.translation.x = m.speed; }}); }} }});
        app.system("first", {{ write: [Mover], run(q) {{ q.each((e, m) => {{ m.speed = 5; }}); }} }});
        app.system("fixed", {{ stage: "postUpdate", read: [Mover], write: [Transform], run(q) {{ q.each((e, m, t) => {{ t.translation.z = m.speed; }}); }} }});
        "#
    );
    load(&mut app, &[("decl.js", &src)]);
    app.update();
    let t = app.world().get::<Transform>(es[0]).unwrap();
    assert_eq!((t.translation.x, t.translation.z), (5.0, 5.0));
}

#[test]
fn declaration_errors_name_what_is_wrong() {
    let mut app = app();
    let cases = [
        ("import { component, string } from \"bevy-ts\";\ncomponent(\"game::Tag\", { label: string });\n", "game::Tag field label", 2),
        (
            "import { app } from \"bevy-ts\";\nimport { GlobalTransform } from \"bevy-ts/bevy\";\napp.system(\"g\", { read: [GlobalTransform], run() {} });\n",
            "GlobalTransform",
            3,
        ),
        (
            "import { app } from \"bevy-ts\";\napp.system(\"a\", { after: \"b\", run() {} });\napp.system(\"b\", { after: \"a\", run() {} });\n",
            "cycle",
            2,
        ),
        ("import { app } from \"bevy-ts\";\napp.system(\"a\", { after: \"nope\", run() {} });\n", "nope", 2),
        ("import { app } from \"bevy-ts\";\napp.system(\"a\", { run() {} });\napp.system(\"a\", { run() {} });\n", "declared twice", 3),
        (
            "import { component, f32 } from \"bevy-ts\";\ncomponent(\"game::X\", { a: f32 });\ncomponent(\"game::X\", { a: f32 });\n",
            "declared twice",
            3,
        ),
        ("import { app } from \"bevy-ts\";\nnull.boom;\n", "TypeError", 2),
    ];
    for (src, want, line) in cases {
        let r = bevy_ts::generation::apply(app.world_mut(), set(&[("bad.js", src)]), false);
        assert_eq!(r.status, ReloadStatus::Failed, "{src}");
        let e = &r.errors[0];
        assert!(e.message.contains(want), "{src}: {e:?}");
        assert_eq!(e.location.as_ref().map(|l| l.line), Some(line), "{src}: {e:?}");
    }
}

#[test]
fn commands_spawn_parent_overflow_and_discard() {
    let mut app = app();
    let src = format!(
        r#"{MOVER}
        export const Big = component("t::Big", {{ data: (await import("bevy-ts")).array((await import("bevy-ts")).f32, 1000) }});
        app.startup("spawn", (ctx) => {{
          const parent = ctx.commands.spawn(Mover({{ speed: 4 }}), Transform);
          const child = ctx.commands.spawn(Mover);
          ctx.commands.setParent(child, parent);
          for (let i = 0; i < 40; i++) ctx.commands.spawn(Big);
        }});
        app.startup("throws", (ctx) => {{
          ctx.commands.spawn(Mover);
          throw new Error("no");
        }});
        "#
    );
    load(&mut app, &[("decl.js", &src)]);
    let movers: Vec<(Entity, f32)> = {
        let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
        let world = app.world_mut();
        let mut q = QueryBuilder::<(Entity, FilteredEntityRef)>::new(world).ref_id(id).build();
        q.iter(world)
            .map(|(e, r)| (e, f32::from_le_bytes(unsafe { std::slice::from_raw_parts(r.get_by_id(id).unwrap().as_ptr(), 4) }.try_into().unwrap())))
            .collect()
    };
    assert_eq!(movers.len(), 2, "the throwing startup's spawn is discarded");
    let parent = movers.iter().find(|(_, s)| *s == 4.0).unwrap().0;
    let child = movers.iter().find(|(_, s)| *s == 1.0).unwrap().0;
    assert_eq!(app.world().get::<ChildOf>(child).unwrap().parent(), parent);
    assert_eq!(app.world().get::<Transform>(parent), Some(&Transform::IDENTITY));
    let big = app.world().resource::<TsComponents>().live("t::Big").unwrap().id;
    let world = app.world_mut();
    let n = QueryBuilder::<Entity>::new(world).with_id(big).build().iter(world).count();
    assert_eq!(n, 40, "160 KB of spawns overflow the 64 KB arena and all apply");
}
