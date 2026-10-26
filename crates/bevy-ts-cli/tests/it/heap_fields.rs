//! Heap fields in TS systems (openspecs ts-component-heap-fields, ts-heap-fields-2), through V8: a
//! `string` or `T[]` field of a component or resource read through the host at its place and
//! cached by its changed tick, assigned whole on a `Mut` row or `ResMut`, read from a lifecycle
//! value's copy, kept by a `Local` across runs and reloads, and refused where heap values are not
//! held yet.

use crate::common;

use bevy::prelude::*;
use bevy_ts::reflect_codec::CALLS;
use common::*;
use serde_json::json;
use std::sync::atomic::Ordering;

const HEAD: &str = r#"import { app, chain, Component, Update } from "bevy-ts";
import type { Changed, f32, Mut, Query, u32 } from "bevy-ts";
export class Tag extends Component { label: string = "crate"; hp: f32 = 0; scores: u32[] = [] }
"#;

/// What a Rust system after the TS ones read of each `Tag`'s label, this frame.
#[derive(Resource, Default)]
struct RustSaw(Vec<String>);

fn rust_reads(world: &mut World) {
    let Some(c) = world.resource::<bevy_ts::components::TsComponents>().live("game::Tag").cloned() else { return };
    let mut q = QueryBuilder::<bevy::ecs::world::FilteredEntityRef>::new(world).ref_id(c.id).build();
    let labels: Vec<String> = q
        .iter(world)
        .map(|r| {
            let p = r.get_by_id(c.id).unwrap();
            // SAFETY: the column holds a live value of `c.layout`.
            let v = unsafe { bevy_ts::value::decode_component(&c.layout, std::slice::from_raw_parts(p.as_ptr(), c.layout.size)) };
            v["label"].as_str().unwrap().to_string()
        })
        .collect();
    world.resource_mut::<RustSaw>().0 = labels;
}

#[test]
fn a_write_is_seen_at_once_by_the_system_rust_and_changed() {
    let body = r#"
export function rename(q: Query<[Mut<Tag>]>) {
  q.each((t) => {
    if (t.label !== "door") return;
    t.label = "open";
    console.log("read " + t.label + " " + JSON.stringify(t));
  });
}
export function seen(q: Query<[Tag], Changed<Tag>>) {
  q.each((t) => console.log("changed " + t.label));
}
app.addSystems(Update, chain(rename, seen));
"#;
    let (_, mut app) = load("heap-write", &format!("{HEAD}{body}"));
    app.init_resource::<RustSaw>().add_systems(PostUpdate, rust_reads);
    let e = spawn_ts(&mut app, "game::Tag", json!({ "label": "door", "hp": 1 }), ());
    app.update();
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap(), json!({ "label": "open", "hp": 1.0, "scores": [] }));
    assert_eq!(app.world().resource::<RustSaw>().0, vec!["open"], "a Rust system later in the frame sees the write");
    updates(&mut app, 2);
    let m = messages(&app);
    assert!(m.iter().any(|l| l == r#"read open {"label":"open","hp":1,"scores":[]}"#), "{m:?}");
    assert!(m.iter().any(|l| l == "changed open"), "{m:?}");
}

#[test]
fn a_list_reads_frozen_and_is_written_whole() {
    let body = r#"
export function grow(q: Query<[Mut<Tag>]>) {
  q.each((t) => {
    if (t.scores.length >= 3) return;
    try {
      (t.scores as u32[]).push(9);
    } catch (e) {
      console.log("push " + (e instanceof TypeError));
    }
    t.scores = [...t.scores, t.scores.length + 1];
    console.log("now " + JSON.stringify(t.scores));
  });
}
app.addSystems(Update, grow);
"#;
    let (_, mut app) = load("heap-list", &format!("{HEAD}{body}"));
    let e = spawn_ts(&mut app, "game::Tag", json!({ "scores": [1] }), ());
    updates(&mut app, 3);
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["scores"], json!([1, 2, 3]));
    updates(&mut app, 2);
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "push true"), "{m:?}");
    assert!(m.iter().any(|l| l == "now [1,2]") && m.iter().any(|l| l == "now [1,2,3]"), "{m:?}");
}

#[test]
fn an_untouched_row_makes_no_host_call_on_the_second_frame() {
    let body = r#"
export function labels(q: Query<[Tag]>) {
  let n = 0;
  q.each((t) => { if (t.label.startsWith("l")) n++; });
  if (n !== 1000) throw new Error("read " + n);
}
app.addSystems(Update, labels);
"#;
    let (_, mut app) = load("heap-cache", &format!("{HEAD}{body}"));
    for i in 0..1000 {
        spawn_ts(&mut app, "game::Tag", json!({ "label": format!("l{i}") }), ());
    }
    let before = CALLS.load(Ordering::Relaxed);
    app.update();
    // Tests in this binary run at once and share the counter: at least our 1000 misses.
    assert!(CALLS.load(Ordering::Relaxed) - before >= 1000);
    assert!(messages(&app).iter().all(|m| !m.contains("read ")), "{:?}", messages(&app));
    let quiet = (0..5).any(|_| {
        let before = CALLS.load(Ordering::Relaxed);
        app.update();
        CALLS.load(Ordering::Relaxed) == before
    });
    assert!(quiet, "every frame made host calls for rows nothing wrote");
}

#[test]
fn a_spawn_from_a_script_carries_its_string() {
    let body = r#"
import type { Commands } from "bevy-ts";
import { Startup } from "bevy-ts";
export function make(commands: Commands) {
  commands.spawn(Tag.of({ label: "lamp", hp: 2, scores: [4, 5] }));
  commands.spawn(Tag);
}
app.addSystems(Startup, make);
"#;
    let (_, mut app) = load("heap-spawn", &format!("{HEAD}{body}"));
    updates(&mut app, 2);
    let c = app.world().resource::<bevy_ts::components::TsComponents>().live("game::Tag").unwrap().id;
    let world = app.world_mut();
    let es: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(c).build().iter(world).collect();
    let mut labels: Vec<_> = es.iter().map(|e| read_ts(&app, *e, "game::Tag").unwrap()).collect();
    labels.sort_by_key(|v| v["label"].as_str().unwrap().to_string());
    assert_eq!(labels, vec![json!({ "label": "crate", "hp": 0.0, "scores": [] }), json!({ "label": "lamp", "hp": 2.0, "scores": [4, 5] })]);
}

#[test]
fn a_resmut_write_is_seen_by_res_and_an_unchanged_res_makes_no_host_call() {
    let body = r#"
import { Resource } from "bevy-ts";
import type { Res, ResMut } from "bevy-ts";
export class Title extends Resource { text: string = "x"; n: u32 = 0 }
export function rename(t: ResMut<Title>) { if (t.text === "x") t.text = "hello"; }
export function show(t: Res<Title>) { console.log("title " + t.text + " " + t.text.length); }
app.insertResource(Title.of({ text: "x" })).addSystems(Update, chain(rename, show));
"#;
    let (_, mut app) = load("heap-resource", &format!("{HEAD}{body}"));
    updates(&mut app, 2);
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "title hello 5"), "{m:?}");
    // `show` reads twice a frame and `rename` once; once nothing writes it, none is a host call.
    let quiet = (0..5).any(|_| {
        let before = CALLS.load(Ordering::Relaxed);
        app.update();
        CALLS.load(Ordering::Relaxed) == before
    });
    assert!(quiet, "an unchanged resource's string was fetched again");
}

#[test]
fn a_despawn_observer_reads_the_label_as_it_was() {
    let body = r#"
import type { Commands, Despawn, Entity, On, Remove } from "bevy-ts";
export function gone(on: On<Despawn<Tag>>) { console.log("gone " + on.label + " " + JSON.stringify(on.scores) + " " + on.event.label); }
export function removed(on: On<Remove<Tag>>) { console.log("removed " + on.label); }
export function kill(q: Query<[Entity, Tag]>, commands: Commands) { q.each((e, t) => { if (t.label === "door") commands.entity(e).despawn(); }); }
app.addObserver(gone).addObserver(removed).addSystems(Update, kill);
"#;
    let (_, mut app) = load("heap-despawn", &format!("{HEAD}{body}"));
    spawn_ts(&mut app, "game::Tag", json!({ "label": "door", "scores": [7, 8] }), ());
    updates(&mut app, 3);
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "gone door [7,8] door"), "{m:?}");
    assert!(m.iter().any(|l| l == "removed door"), "{m:?}");
}

#[test]
fn a_local_keeps_its_string_and_list_across_runs_and_a_reload_that_adds_a_field() {
    let src = |extra: &str| {
        format!(
            r#"import {{ app, Update }} from "bevy-ts";
import type {{ Local, u32 }} from "bevy-ts";
export function tally(l: Local<{{ log: string; hits: u32[]{extra} }}>) {{
  try {{
    (l.hits as u32[]).push(9);
  }} catch (e) {{
    if (!(e instanceof TypeError)) throw e;
  }}
  l.log = l.log + "x";
  l.hits = [...l.hits, l.hits.length];
  console.log("tally " + l.log + " " + JSON.stringify(l.hits));
}}
app.addSystems(Update, tally);
"#
        )
    };
    let (dir, mut app) = load("heap-local", &src(""));
    updates(&mut app, 3);
    assert!(messages(&app).iter().any(|l| l == "tally xx [0,1]"), "{:?}", messages(&app));
    write(&dir, "game.ts", &src("; n: u32"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    assert!(messages(&app).iter().any(|l| l == "tally xxx [0,1,2]"), "the reload kept the values: {:?}", messages(&app));
}

#[test]
fn heap_fields_outside_components_and_unsupported_types_fail_with_file_and_line() {
    let errors = load_errors(
        "heap-message",
        r#"import { Message } from "bevy-ts";
export class Said extends Message { text: string = "x" }
"#,
    );
    let e = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(e.contains("game.ts:2:") && e.contains("Said field text") && e.contains("allowed on components, resources and Local only"), "{e}");

    let errors = load_errors(
        "heap-list-of",
        r#"import { Component, type Vec3 } from "bevy-ts";
export class Path extends Component { declare points: Vec3[] }
"#,
    );
    let e = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(e.contains("game.ts:2:") && e.contains("Path field points"), "{e}");

    let errors = load_errors(
        "heap-object",
        r#"import { Component, type f32 } from "bevy-ts";
export class Meta extends Component { declare meta: { a: f32 } }
"#,
    );
    let e = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(e.contains("game.ts:2:") && e.contains("Meta field meta"), "{e}");
}
