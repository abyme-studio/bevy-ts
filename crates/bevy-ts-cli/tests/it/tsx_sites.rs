//! Bindings on native-layout fields run per site (tsx design decision 11): Rust finds the
//! instances whose own reads changed, in table order and with no JS; one host call re-runs only
//! those rows; a derived value equal to the row's writes nothing.
//!
//! The timing test is ignored by default: run it in release,
//! `cargo static test --release -p bevy-ts-cli --test it tsx_sites:: -- --ignored --nocapture`.

use crate::common;

use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::plugin_api;
use bevy_ts_tsx::TsxTree;
use common::*;
use std::time::Instant;

fn sirens(n: usize) -> String {
    format!(
        r##"import {{ app, Component, f32, u8, ref }} from "bevy-ts";
export class Alarm extends Component {{ static path = "game::Alarm"; declare on: u8 }}
export class Light extends Component {{ static path = "game::Light"; declare intensity: f32 }}
export const Siren = () => {{
  const self = ref();
  return (
    <entity ref={{self}}>
      <Alarm initialOn={{0}} />
      <Light intensity={{() => (self.get(Alarm).on ? 8 : 2)}} />
    </entity>
  );
}};
app.scene("s", () => <>{{Array.from({{ length: {n} }}, (_, i) => <Siren key={{`s${{i}}`}} />)}}</>);
"##
    )
}

const SYSTEM: &str = r##"import { app, Component, f32, u8, PostUpdate, type Query, type Mut } from "bevy-ts";
export class Alarm extends Component { static path = "game::Alarm"; declare on: u8 }
export class Light extends Component { static path = "game::Light"; declare intensity: f32 }
export function siren(q: Query<[Alarm, Mut<Light>]>) {
  q.each((a, l) => {
    const v = a.on ? 8 : 2;
    if (l.intensity !== v) l.intensity = v;
  });
}
app.addSystems(PostUpdate, siren);
"##;

struct Ids {
    alarm: ComponentId,
    light: ComponentId,
}

fn ids(app: &App) -> Ids {
    Ids {
        alarm: plugin_api::component_by_name(app.world(), "game::Alarm").unwrap().0,
        light: plugin_api::component_by_name(app.world(), "game::Light").unwrap().0,
    }
}

fn load(name: &str, src: &str) -> App {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    app
}

fn alarms(app: &mut App, ids: &Ids) -> Vec<Entity> {
    let world = app.world_mut();
    let mut q = QueryBuilder::<Entity>::new(world).with_id(ids.alarm).build();
    let mut v: Vec<Entity> = q.iter(world).collect();
    v.sort();
    v
}

fn set_alarm(app: &mut App, ids: &Ids, e: Entity, on: u8) {
    let mut m = app.world_mut().get_mut_by_id(e, ids.alarm).unwrap();
    // SAFETY: Alarm is one u8.
    unsafe { *m.as_mut().as_ptr() = on };
}

fn intensity(app: &App, ids: &Ids, e: Entity) -> f32 {
    let p = app.world().get_by_id(e, ids.light).unwrap();
    // SAFETY: Light is one f32.
    f32::from_le_bytes(unsafe { std::slice::from_raw_parts(p.as_ptr(), 4) }.try_into().unwrap())
}

fn light_tick(app: &App, ids: &Ids, e: Entity) -> bevy::ecs::change_detection::Tick {
    plugin_api::change_ticks(app.world(), e, ids.light).unwrap().changed
}

fn calls(app: &App) -> u64 {
    app.world().resource::<TsxTree>().reactive.calls
}

#[test]
fn a_thousand_sirens_update_only_the_rows_whose_alarm_changed() {
    let mut app = load("sites-1k", &sirens(1000));
    let ids = ids(&app);
    let all = alarms(&mut app, &ids);
    assert_eq!(all.len(), 1000);
    assert!(all.iter().all(|e| intensity(&app, &ids, *e) == 2.0), "evaluated on mount");
    let ticks: Vec<_> = all.iter().map(|e| light_tick(&app, &ids, *e)).collect();

    // A quiet frame: no call, no tick.
    let before = calls(&app);
    updates(&mut app, 3);
    assert_eq!(calls(&app), before, "quiet frames make no host call");

    // 100 alarms change: one call, 100 lights written, the rest untouched.
    for e in all.iter().take(100) {
        set_alarm(&mut app, &ids, *e, 1);
    }
    app.update();
    assert_eq!(calls(&app), before + 1, "one host call for all changed rows");
    let moved = all.iter().zip(&ticks).filter(|(e, t)| light_tick(&app, &ids, **e) != **t).count();
    assert_eq!(moved, 100);
    assert!(all.iter().take(100).all(|e| intensity(&app, &ids, *e) == 8.0));

    // An alarm written with the value it had: the closure runs, the light is not written.
    let t = light_tick(&app, &ids, all[0]);
    set_alarm(&mut app, &ids, all[0], 1);
    app.update();
    assert_eq!(light_tick(&app, &ids, all[0]), t, "same derived value: no write");
}

/// Per frame, flipping 100 alarms before each: `measure` after the update, median over `frames`.
fn median_over(app: &mut App, ids: &Ids, frames: usize, measure: impl Fn(&App) -> f64) -> f64 {
    let all = alarms(app, ids);
    let mut on = 1u8;
    let mut v = Vec::with_capacity(frames);
    for f in 0..frames {
        for e in all.iter().skip((f * 100) % all.len()).take(100) {
            set_alarm(app, ids, *e, on);
        }
        on ^= 1;
        app.update();
        v.push(measure(app));
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn frame_time(app: &mut App, ids: &Ids, frames: usize) -> f64 {
    let all = alarms(app, ids);
    let mut on = 1u8;
    let mut v = Vec::with_capacity(frames);
    for f in 0..frames {
        for e in all.iter().skip((f * 100) % all.len()).take(100) {
            set_alarm(app, ids, *e, on);
        }
        on ^= 1;
        let t = Instant::now();
        app.update();
        v.push(t.elapsed().as_secs_f64() * 1e6);
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[test]
#[ignore = "timing: run in release"]
fn ten_thousand_bound_sirens_cost_within_twice_a_ts_system() {
    // One V8 app at a time: isolates cannot interleave on one thread.
    let (b, quiet, work) = {
        let mut bound = load("sites-10k", &sirens(10_000));
        let ids = ids(&bound);
        frame_time(&mut bound, &ids, 50);
        {
            let r = &mut bound.world_mut().resource_mut::<TsxTree>().reactive;
            (r.scan_us, r.call_us, r.apply_us) = (0.0, 0.0, 0.0);
        }
        let work = median_over(&mut bound, &ids, 300, |app| app.world().resource::<TsxTree>().reactive.last_frame_us);
        println!("bound sites: tsx work per frame, median {work:.1} µs");
        let b = frame_time(&mut bound, &ids, 300);
        let r = &bound.world().resource::<TsxTree>().reactive;
        println!("per frame: scan {:.1} µs, host call {:.1} µs, apply {:.1} µs", r.scan_us / 300.0, r.call_us / 300.0, r.apply_us / 300.0);
        let before = calls(&bound);
        let t = Instant::now();
        for _ in 0..300 {
            bound.update();
        }
        assert_eq!(calls(&bound), before, "quiet frames make no host call");
        (b, t.elapsed().as_secs_f64() / 300.0 * 1e6, work)
    };
    let (s, run) = {
        let mut system = load("sites-system", SYSTEM);
        let ids = ids(&system);
        for _ in 0..10_000 {
            let e = system.world_mut().spawn_empty().id();
            // SAFETY: Alarm is one u8, Light one f32.
            unsafe {
                bevy_ts::commands::insert_bytes(&mut system.world_mut().entity_mut(e), ids.alarm, &[0]);
                bevy_ts::commands::insert_bytes(&mut system.world_mut().entity_mut(e), ids.light, &2f32.to_le_bytes());
            }
        }
        frame_time(&mut system, &ids, 50);
        let run = median_over(&mut system, &ids, 300, |app| {
            app.world().resource::<bevy_ts::systems::TsSystems>().iter().next().map_or(0.0, |s| s.last_run_us() as f64)
        });
        println!("TS system: its run per frame, median {run:.1} µs");
        (frame_time(&mut system, &ids, 300), run)
    };
    let base = {
        let mut empty = load("sites-empty", "import { app } from \"bevy-ts\";\n");
        for _ in 0..50 {
            empty.update();
        }
        let t = Instant::now();
        for _ in 0..300 {
            empty.update();
        }
        t.elapsed().as_secs_f64() / 300.0 * 1e6
    };
    println!(
        "10k sirens, 100 alarms changed per frame: bound sites {b:.1} µs/frame, TS system {s:.1} µs/frame, empty frame {base:.1} µs, quiet bound frame {quiet:.1} µs; over the empty frame: sites {:.1} µs, system {:.1} µs",
        b - base,
        s - base
    );
    let _ = base;
    println!("medians: bound frame {b:.1} µs, system frame {s:.1} µs; tsx work {work:.1} µs vs the system's run {run:.1} µs");
    // The spec's measure: the frame, bound against a TS system doing the same, as medians (the
    // machine is shared, so means are noise).
    assert!(b <= 2.0 * s, "bound frame {b:.1} µs > 2 × system frame {s:.1} µs");
}

// layout-bindable: a TS system that changes a bound field warns under dev, per field and entity.

const TWO_WRITERS: &str = "a bound field has one writer";

/// A headless app for sprites, `dev` as given, with `game.tsx` loaded and two frames run.
fn sprite_app(name: &str, src: &str, dev: bool) -> App {
    use bevy_ts::generation::DevSource;
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(AssetPlugin::default());
    app.add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(dev)).add_plugins(bevy_ts_tsx::TsxPlugin);
    app.init_asset::<Image>().init_asset::<TextureAtlasLayout>().register_asset_reflect::<TextureAtlasLayout>().register_type::<Sprite>();
    app.insert_resource(DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("game.tsx"), true).unwrap())));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    app
}

/// The two-writer warnings logged so far.
fn two_writers(app: &App) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.contains(TWO_WRITERS)).collect()
}

/// Runs frames until `done` holds, or panics naming `what` after 100.
fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..100 {
        if done(app) {
            return;
        }
        app.update();
    }
    panic!("{what} within 100 frames; log: {:?}", messages(app));
}

/// A sprite with a bound colour and an atlas, and one system that writes `body` on it each frame.
fn painted(body: &str) -> String {
    format!(
        r##"import {{ app, Update }} from "bevy-ts";
import type {{ Local, Mut, Query, u32 }} from "bevy-ts";
import {{ Sprite, TextureAtlasLayout }} from "bevy-ts/bevy";

export function paint(n: Local<{{ n: u32 }}>, q: Query<[Mut<Sprite>]>) {{
  n.n += 1;
  for (const [sprite] of q) {{
    {body}
  }}
}}
app.addSystems(Update, paint);
app.scene("s", () => (
  <entity name="it">
    <Sprite color={{() => "#ff0000"}} texture_atlas={{{{ layout: TextureAtlasLayout.fromGrid({{ tileSize: [8, 8], columns: 8, rows: 1 }}), index: 0 }}}} />
  </entity>
));
"##
    )
}

/// The 1-based line of the `<Sprite>` element in `painted`'s source.
fn sprite_line(src: &str) -> usize {
    src.lines().position(|l| l.contains("<Sprite color")).unwrap() + 1
}

fn atlas_index(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&Sprite>().single(world).unwrap().texture_atlas.as_ref().expect("an atlas").index
}

#[test]
fn writing_texture_atlas_next_to_a_bound_sprite_colour_does_not_warn() {
    let src = painted("const atlas = sprite.texture_atlas;\n    if (atlas) atlas.index = n.n % 8;");
    let mut app = sprite_app("bound-atlas", &src, true);
    let first = atlas_index(&mut app);
    updates(&mut app, 10);
    assert_ne!(atlas_index(&mut app), first, "the system wrote texture_atlas");
    assert_eq!(two_writers(&app), Vec::<String>::new());
}

#[test]
fn writing_the_bound_sprite_colour_warns_once() {
    let src = painted("sprite.color = n.n % 2 ? \"#00ff00\" : \"#0000ff\";");
    let mut app = sprite_app("bound-colour", &src, true);
    until(&mut app, "a two-writer warning", |app| !two_writers(app).is_empty());
    updates(&mut app, 10);
    let w = two_writers(&app);
    assert_eq!(w.len(), 1, "one warning over 10 frames: {w:?}");
    let line = format!("game.tsx:{}", sprite_line(&src));
    for want in ["system game.tsx#paint wrote", "Sprite.color", " on it,", "<Sprite color>", line.as_str()] {
        assert!(w[0].contains(want), "{want} in {}", w[0]);
    }
}

#[test]
fn a_bound_translation_warns_only_the_system_that_writes_it() {
    let src = r##"import { app, Transform, Update } from "bevy-ts";
import type { Local, Mut, Query, u32 } from "bevy-ts";

export function grow(n: Local<{ n: u32 }>, q: Query<[Mut<Transform>]>) {
  n.n += 1;
  for (const [t] of q) t.scale = [n.n, n.n, n.n];
}
export function shift(n: Local<{ n: u32 }>, q: Query<[Mut<Transform>]>) {
  n.n += 1;
  for (const [t] of q) t.translation = [n.n, 0, 0];
}
app.addSystems(Update, grow);
app.addSystems(Update, shift);
app.scene("s", () => (
  <entity name="it">
    <Transform translation={() => [1, 2, 3]} />
  </entity>
));
"##;
    let mut app = sprite_app("bound-translation", src, true);
    until(&mut app, "a two-writer warning", |app| !two_writers(app).is_empty());
    updates(&mut app, 10);
    let w = two_writers(&app);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("system game.tsx#shift") && w[0].contains("Transform.translation"), "{}", w[0]);
}

#[test]
fn no_warning_without_dev() {
    let src = painted("sprite.color = n.n % 2 ? \"#00ff00\" : \"#0000ff\";");
    let mut app = sprite_app("bound-colour-release", &src, false);
    updates(&mut app, 10);
    let world = app.world_mut();
    let colour = world.query::<&Sprite>().single(world).unwrap().color;
    assert_ne!(colour, Color::srgb(1.0, 0.0, 0.0), "the system wrote the colour");
    assert_eq!(two_writers(&app), Vec::<String>::new());
}

/// 10,000 entities with a bound `Transform.translation` and one system writing `scale` on all of
/// them each frame: the frame's median under dev with the check against dev without it (its
/// resource removed, as `dev(false)` has none), so only the check differs.
#[test]
#[ignore = "timing: run in release"]
fn bound_field_check_costs_under_half_a_millisecond() {
    let src = r##"import { app, Transform, Update } from "bevy-ts";
import type { Local, Mut, Query, u32 } from "bevy-ts";

export function grow(n: Local<{ n: u32 }>, q: Query<[Mut<Transform>]>) {
  n.n += 1;
  const s = 1 + (n.n % 2);
  for (const [t] of q) t.scale = [s, s, s];
}
app.addSystems(Update, grow);
app.scene("s", () => <>{Array.from({ length: 10000 }, (_, i) => <entity key={`e${i}`}><Transform translation={() => [i, 0, 0]} /></entity>)}</>);
"##;
    let frame = |check: bool| {
        let mut app = sprite_app(if check { "bound-bench-on" } else { "bound-bench-off" }, src, true);
        if !check {
            app.world_mut().remove_resource::<bevy_ts::bound::BoundFields>();
        }
        updates(&mut app, 50);
        if check {
            let bound = app.world().resource::<bevy_ts::bound::BoundFields>();
            assert_eq!(bound.components.values().map(|c| c.entities.len()).sum::<usize>(), 10_000, "every translation is bound");
        }
        let mut v: Vec<f64> = (0..300)
            .map(|_| {
                let t = Instant::now();
                app.update();
                t.elapsed().as_secs_f64() * 1e6
            })
            .collect();
        v.sort_by(f64::total_cmp);
        assert!(two_writers(&app).is_empty(), "scale is not bound");
        v[v.len() / 2]
    };
    let off = frame(false);
    let on = frame(true);
    println!("10k bound translations, scale written each frame: check {on:.1} µs/frame, none {off:.1} µs/frame, check {:.1} µs", on - off);
    assert!(on - off < 500.0, "the check costs {:.1} µs a frame", on - off);
}
