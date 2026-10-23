//! Rust messages and events from TS, and `commands.entity(e).observe` (openspec rust-events).

mod common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::observers::TsEntityObserver;
use common::*;

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
