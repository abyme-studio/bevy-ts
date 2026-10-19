//! Exported Rust system params (native decision 8) through the V8 backend: one host call per
//! method call, batches in place, errors and panics thrown in JS.

mod common;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_ts::host::HostParams;
use bevy_ts_native::prelude::*;
use common::*;

#[derive(Resource)]
pub struct Ground(pub f32);

#[derive(SystemParam)]
pub struct Probe<'w> {
    ground: Option<Res<'w, Ground>>,
}

#[derive(HostRecord)]
pub struct Hit {
    pub toi: f32,
    pub normal: Vec3,
    pub entity: Entity,
}

#[export(ts = "Probe", module = "bevy-ts/test")]
impl Probe<'_> {
    fn ground(&self) -> Result<f32, String> {
        self.ground.as_ref().map(|g| g.0).ok_or_else(|| "no ground yet".to_string())
    }

    /// The distance down to the ground from `origin`, when below `max`.
    pub fn cast_ray(&self, origin: Vec3, max: f32) -> Option<Hit> {
        let g = self.ground().ok()?;
        let toi = origin.y - g;
        (toi >= 0.0 && toi <= max).then_some(Hit { toi, normal: Vec3::Y, entity: Entity::from_bits(7) })
    }

    /// One call for many rays: `ys` in, distances out; returns how many hit.
    pub fn cast_rays(&self, ys: &[f32], toi: &mut [f32]) -> u32 {
        let g = self.ground().unwrap_or(f32::NAN);
        let mut hits = 0;
        for (y, t) in ys.iter().zip(toi.iter_mut()) {
            *t = y - g;
            hits += (*t >= 0.0) as u32;
        }
        hits
    }

    pub fn boom(&self) -> f32 {
        panic!("boom")
    }

    /// Fails when there is no ground, as a param whose context is missing does.
    pub fn height(&self) -> Result<f32, String> {
        self.ground()
    }
}

const SRC: &str = r#"import { app, Update } from "bevy-ts";
import type { Probe } from "bevy-ts/test";

export function cast(probe: Probe) {
  const hit = probe.castRay([0, 5, 0], 100);
  const ys = new Float32Array(1000);
  for (let i = 0; i < 1000; i++) ys[i] = i;
  const toi = new Float32Array(1000);
  const n = probe.castRays(ys, toi);
  if (hit) console.log(`hit ${hit.toi} ${hit.normal.y} ${hit.entity} rays ${n} ${toi[999]}`);
  let caught = "";
  try {
    probe.boom();
  } catch (e) {
    caught = e.message;
  }
  console.log(`caught ${caught}`);
  console.log(`height ${probe.height()}`);
}

app.addSystems(Update, cast);
"#;

fn with_probe(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.ts"));
    app.ts_param::<Probe<'static>>();
    app.insert_resource(Ground(2.0));
    app
}

#[test]
fn a_batch_is_one_crossing_and_results_arrive_in_the_same_run() {
    let dir = project("exported", &[("game.ts", SRC)]);
    let mut app = with_probe(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let before = app.world().resource::<HostParams>().calls;
    app.update();
    let calls = app.world().resource::<HostParams>().calls - before;
    // castRay, castRays (1,000 rays in one crossing), boom and height.
    assert_eq!(calls, 4);
    // console output is collected at the next frame's pump.
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "hit 3 1 7 rays 998 997"), "{m:?}");
    assert!(m.iter().any(|l| l.starts_with("caught Probe.boom panicked: boom")), "{m:?}");
    assert!(m.iter().any(|l| l == "height 2"), "{m:?}");

    // Without the resource the method throws, naming param and method; the run's commands go.
    app.world_mut().remove_resource::<Ground>();
    app.update();
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|l| l.contains("Probe.height: no ground yet")), "{m:?}");
}

#[test]
fn an_unknown_param_kind_fails_the_load() {
    let errors = load_errors(
        "exported-unknown",
        "import { app, Update } from \"bevy-ts\";\nimport type { Prob } from \"bevy-ts/test\";\nexport function cast(p: Prob) {}\napp.addSystems(Update, cast);\n",
    );
    assert!(errors.iter().any(|e| e.message.contains("Prob")), "{errors:?}");
}

const MODULE_JS: &str = r#"import { Message } from "bevy-ts";
export class Bump extends Message {
  static {
    __bevyts.data(this, { kind: "message", name: "test::Bump", fields: [{ name: "kind", type: { kind: "enum", variants: ["Soft", "Hard"] } }, { name: "who", type: { kind: "entity" } }] });
  }
}
export function twice(x) { return x * 2; }
"#;

/// A Rust system forwarding into the module's message, as a port does with Rapier's events.
fn bump(world: &mut World) {
    let values = serde_json::json!({ "kind": "Hard", "who": Entity::from_bits(9).to_bits() });
    let _ = bevy_ts::plugin_api::write_message(world, "test::Bump", values.as_object().unwrap());
}

#[test]
fn a_port_module_serves_classes_and_rust_writes_its_messages() {
    let src = r#"import { app, Update } from "bevy-ts";
import type { MessageReader } from "bevy-ts";
import { Bump, twice } from "bevy-ts/test-module";

export function read(r: MessageReader<Bump>) {
  r.each((m) => console.log(`bump ${m.kind} ${m.who} ${twice(21)}`));
}

app.addMessage(Bump).addSystems(Update, read);
"#;
    let dir = project("exported-module", &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    app.ts_module("bevy-ts/test-module", MODULE_JS, "export declare class Bump {}\n");
    app.add_systems(PreUpdate, bump);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "bump Hard 9 42"), "{m:?}");
}
