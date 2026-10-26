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
