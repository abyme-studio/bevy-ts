//! Task 8.3: mount and reload of 1,000 and 10,000 enemies, and the HUD update cost. Ignored by
//! default; run in release and record the output in `docs/research/tsx-scale.md`:
//! `cargo static test --release -p bevy-ts-cli --test it tsx_scale:: -- --ignored --nocapture --test-threads 1`.

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::plugin_api;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;
use std::path::Path;
use std::time::Instant;

fn game(n: usize, color: &str) -> String {
    format!(
        r##"import {{ app, Component, f32, ref, px }} from "bevy-ts";
import {{ Transform }} from "bevy-ts/bevy";
export class Health extends Component {{ static path = "game::Health"; hp: f32 = 100; max: f32 = 100 }}
const Enemy = ({{ at, hp = 100 }}: {{ at: [number, number, number]; hp?: number }}) => (
  <entity name="enemy">
    <Transform translation={{at}} />
    <Health initialHp={{hp}} max={{hp}} />
    <mesh shape="cuboid" size={{[1, 1, 1]}} color="{color}" />
  </entity>
);
const player = ref();
app.scene("arena", () => (
  <>
    <entity key="player" ref={{player}}><Health initialHp={{100}} /></entity>
    {{Array.from({{ length: {n} }}, (_, i) => <Enemy key={{`e${{i}}`}} at={{[i % 100, 0, Math.floor(i / 100)]}} />)}}
    <node key="hud" style={{{{ position: "absolute", top: px(12), left: px(12) }}}} background="#0008">
      <text key="hp" size={{20}}>{{() => `HP ${{Math.ceil(player.get(Health).hp)}} / ${{player.get(Health).max}}`}}</text>
    </node>
  </>
));
"##
    )
}

fn tsx_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    app
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn timed_reload(app: &mut App, dir: &Path, src: &str) -> (f64, f64) {
    write(dir, "game.tsx", src);
    let t = Instant::now();
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let apply = t.elapsed().as_secs_f64() * 1e3;
    let tm = app.world().resource::<TsxTree>().timings;
    println!("  reload: total {apply:.1} ms; tsx check {:.1} (parse {:.1}), apply {:.1} (resolve {:.1}, reconcile {:.1}) ms", tm.check_ms, tm.parse_ms, tm.apply_ms, tm.resolve_ms, tm.reconcile_ms);
    let t = Instant::now();
    app.update();
    (apply, t.elapsed().as_secs_f64() * 1e3)
}

fn measure(n: usize) {
    let dir = project(&format!("scale-{n}"), &[("game.tsx", &game(n, "#c33"))]);
    let mut app = tsx_app(&dir.join("game.tsx"));
    let t = Instant::now();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let mount = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    app.update();
    let first = t.elapsed().as_secs_f64() * 1e3;
    let count = {
        let world = app.world_mut();
        world.query::<&TsxNode>().iter(world).count()
    };
    assert_eq!(count, n + 3);

    let (same, same_frame) = timed_reload(&mut app, &dir, &format!("{}\n", game(n, "#c33")));
    assert!(app.world().resource::<TsxTree>().last.is_empty());
    let (recolour, recolour_frame) = timed_reload(&mut app, &dir, &game(n, "#3c3"));
    assert_eq!(app.world().resource::<TsxTree>().last.written, n);

    // The HUD: the player's hp changes every frame; the tsx work per frame, median.
    let player = {
        let world = app.world_mut();
        world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["player"]).map(|(e, _)| e).unwrap()
    };
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let mut hud = Vec::new();
    let mut quiet = Vec::new();
    for f in 0..200 {
        let changed = f % 2 == 0;
        if changed {
            let mut m = app.world_mut().get_mut_by_id(player, id).unwrap();
            // SAFETY: Health is two f32s.
            unsafe { std::ptr::copy_nonoverlapping((50.0 + f as f32).to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
        }
        app.update();
        let us = app.world().resource::<TsxTree>().reactive.last_frame_us;
        if changed { hud.push(us) } else { quiet.push(us) }
    }
    println!(
        "| {n} | {mount:.1} | {first:.1} | {same:.1} + {same_frame:.1} | {recolour:.1} + {recolour_frame:.1} | {:.1} | {:.1} |",
        median(hud),
        median(quiet)
    );
}

#[test]
#[ignore = "timing: run in release"]
fn mount_reload_and_hud_at_scale() {
    println!("| enemies | mount: load + reconcile, ms | first frame, ms | identical reload, ms (apply + frame) | recolour every enemy, ms | HUD hit: tsx work, µs | quiet frame: tsx work, µs |");
    println!("|---|---|---|---|---|---|---|");
    for n in [1_000, 10_000] {
        measure(n);
    }
}

/// 1,000 bindings reading `ref.has(Pressed)` (presence pairs) or `ref.get(Health)` (tick pairs),
/// nothing changing: the per-frame check's cost, median of 200 quiet frames, and host calls made.
fn quiet_reads(read: &str) -> (f64, u64) {
    let src = format!(
        r##"import {{ app, Component, f32, ref }} from "bevy-ts";
import {{ Pressed }} from "bevy-ts/bevy";
export class Health extends Component {{ static path = "game::Health"; hp: f32 = 100 }}
const Row = () => {{
  const r = ref();
  return <button ref={{r}} background={{() => ({read}) ? "#224466" : "#3366cc"}}><Health initialHp={{100}} /></button>;
}};
app.scene("ui", () => <node key="list">{{Array.from({{ length: 1000 }}, (_, i) => <Row key={{`r${{i}}`}} />)}}</node>);
"##
    );
    let dir = project(&format!("scale-reads-{}", if read.contains("has") { "has" } else { "get" }), &[("game.tsx", &src)]);
    let mut app = tsx_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 5);
    assert_eq!(app.world().resource::<TsxTree>().reactive.subs.len(), 1000, "one pair per binding");
    let calls = app.world().resource::<TsxTree>().reactive.calls;
    let mut quiet = Vec::new();
    for _ in 0..200 {
        app.update();
        quiet.push(app.world().resource::<TsxTree>().reactive.last_frame_us);
    }
    (median(quiet), app.world().resource::<TsxTree>().reactive.calls - calls)
}

#[test]
#[ignore = "timing: run in release"]
fn quiet_frames_with_a_thousand_has_or_get_reads() {
    println!("| 1,000 bindings reading | quiet frame: tsx work, µs | host calls in 200 quiet frames |");
    println!("|---|---|---|");
    for read in ["r.has(Pressed)", "r.get(Health).hp > 50"] {
        let (us, calls) = quiet_reads(read);
        println!("| `{read}` | {us:.1} | {calls} |");
        assert_eq!(calls, 0, "a quiet frame makes no host call");
    }
}
