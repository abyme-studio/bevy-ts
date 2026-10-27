//! TS relationships (openspec/changes/custom-relationships): sources and targets declared in TS,
//! Bevy's hooks by id, reads, clone, despawn and hot reload, one test per scenario.

use crate::common;

use bevy::ecs::component::ComponentId;
use bevy::ecs::entity::EntityCloner;
use bevy::ecs::relationship::RelationshipAccessor;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

const HEAD: &str = r#"import { app, Component, f32, PostUpdate, Relationship, RelationshipTarget, Update } from "bevy-ts";
import type { Changed, Commands, Entity, Option, Query, With } from "bevy-ts";
export class Health extends Component { static path = "game::Health"; hp: f32 = 100 }
export class Likes extends Relationship { static path = "game::Likes"; declare target: Entity }
export class LikedBy extends RelationshipTarget(Likes) { static path = "game::LikedBy" }
export class OwnedBy extends Relationship { static path = "game::OwnedBy"; declare target: Entity }
export class Owns extends RelationshipTarget(OwnedBy, { linkedSpawn: true }) { static path = "game::Owns" }
export class Mirror extends Relationship { static path = "game::Mirror"; static allowSelfReferential = true; declare target: Entity }
export class Mirrored extends RelationshipTarget(Mirror) { static path = "game::Mirrored" }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

/// Loads `HEAD` and `body`, with `$0`, `$1`, ... standing for the entities `setup` spawned after
/// `HEAD` alone loaded.
fn staged(name: &str, setup: impl FnOnce(&mut App) -> Vec<Entity>, body: &str) -> (TestDir, App, Vec<Entity>) {
    let dir = project(name, &[("game.tsx", &src(""))]);
    let mut app = app(&dir.join("game.tsx"));
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

fn loaded(name: &str) -> App {
    staged(name, |_| vec![], "").1
}

fn errors(name: &str, head: &str, body: &str) -> Vec<String> {
    let dir = project(name, &[("game.tsx", &format!("{head}{body}"))]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed, "expected a failed load");
    r.errors.into_iter().map(|e| e.message).collect()
}

fn id(app: &App, name: &str) -> ComponentId {
    bevy_ts::plugin_api::component_by_name(app.world(), name).unwrap_or_else(|| panic!("{name} is live")).0
}

/// A target's list on `e`, or `None` without one.
fn list(app: &App, e: Entity, target: &str) -> Option<Vec<Entity>> {
    let p = app.world().get_by_id(e, id(app, target))?;
    // SAFETY: a TS target's value is a `Vec<Entity>`.
    Some(unsafe { p.deref::<Vec<Entity>>() }.clone())
}

fn sorted(mut v: Vec<Entity>) -> Vec<Entity> {
    v.sort();
    v
}

/// A source's target on `e`.
fn target_of(app: &App, e: Entity, source: &str) -> Option<Entity> {
    let v = read_ts(app, e, source)?;
    Entity::try_from_bits(v["target"].as_u64().unwrap())
}

/// A Rust insert of a source by id, with the hooks' commands flushed.
fn point(app: &mut App, e: Entity, source: &str, at: Entity) {
    insert_ts(app, e, source, json!({ "target": at.to_bits() }));
    app.world_mut().flush();
}

fn unpoint(app: &mut App, e: Entity, source: &str) {
    let id = id(app, source);
    app.world_mut().entity_mut(e).remove_by_id(id);
    app.world_mut().flush();
}

fn spawn(app: &mut App) -> Entity {
    app.world_mut().spawn_empty().id()
}

fn lines(app: &mut App, prefix: &str) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|l| l.starts_with(prefix)).collect()
}

// ---------------------------------------------------------------------------------------------
// Declaring

#[test]
fn a_pair_loads_with_bevys_accessors() {
    let app = loaded("rel-loads");
    let (likes, liked_by) = (id(&app, "game::Likes"), id(&app, "game::LikedBy"));
    let info = |c| app.world().components().get_info(c).unwrap().relationship_accessor().copied();
    match info(likes) {
        Some(RelationshipAccessor::Relationship { entity_field_offset, relationship_target, linked_spawn, allow_self_referential }) => {
            assert_eq!((entity_field_offset, relationship_target, linked_spawn, allow_self_referential), (0, liked_by, false, false));
        }
        other => panic!("Likes: {other:?}"),
    }
    match info(liked_by) {
        Some(RelationshipAccessor::RelationshipTarget { relationship, linked_spawn, .. }) => assert_eq!((relationship, linked_spawn), (likes, false)),
        other => panic!("LikedBy: {other:?}"),
    }
    assert!(matches!(info(id(&app, "game::Owns")), Some(RelationshipAccessor::RelationshipTarget { linked_spawn: true, .. })));
    assert!(matches!(info(id(&app, "game::Mirror")), Some(RelationshipAccessor::Relationship { allow_self_referential: true, .. })));
    // A source is immutable, as `ChildOf` is.
    assert!(!app.world().components().get_info(likes).unwrap().mutable());
}

#[test]
fn a_rust_system_reads_a_target_through_its_accessor() {
    let mut app = loaded("rel-accessor");
    let (t, a, b) = (spawn(&mut app), spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    point(&mut app, b, "game::Likes", t);
    let liked_by = id(&app, "game::LikedBy");
    let world = app.world();
    let Some(RelationshipAccessor::RelationshipTarget { iter, .. }) = world.components().get_info(liked_by).unwrap().relationship_accessor().copied() else { panic!("a target") };
    let ptr = world.get_by_id(t, liked_by).unwrap();
    // SAFETY: `ptr` is a value of the component the accessor belongs to.
    let sources: Vec<Entity> = unsafe { iter(ptr) }.collect();
    assert_eq!(sources, vec![a, b]);
}

#[test]
fn bad_declarations_fail_the_load() {
    let head = "import { app, Relationship, RelationshipTarget, Update, f32 } from \"bevy-ts\";\nimport type { Entity, Mut, Query } from \"bevy-ts\";\n";
    let e = errors("rel-not-first", head, "export class Likes extends Relationship { static path = \"game::Likes\"; since: f32 = 0; declare target: Entity }\nexport class LikedBy extends RelationshipTarget(Likes) {}\n");
    assert!(e.iter().any(|m| m.contains("game::Likes") && m.contains("first field is target: Entity")), "{e:?}");
    let e = errors("rel-no-target", head, "export class Likes extends Relationship { static path = \"game::Likes\"; declare target: Entity }\n");
    assert!(e.iter().any(|m| m.contains("game::Likes") && m.contains("no RelationshipTarget")), "{e:?}");
    let pair = "export class Likes extends Relationship { static path = \"game::Likes\"; declare target: Entity }\n";
    let e = errors("rel-target-field", head, &format!("{pair}export class LikedBy extends RelationshipTarget(Likes) {{ static path = \"game::LikedBy\"; n: f32 = 0 }}\n"));
    assert!(e.iter().any(|m| m.contains("game::LikedBy") && m.contains("has no fields")), "{e:?}");
    let e = errors(
        "rel-two-targets",
        head,
        &format!("{pair}export class LikedBy extends RelationshipTarget(Likes) {{ static path = \"game::LikedBy\" }}\nexport class Fans extends RelationshipTarget(Likes) {{ static path = \"game::Fans\" }}\n"),
    );
    assert!(e.iter().any(|m| m.contains("game::Likes") && m.contains("two targets")), "{e:?}");
    let e = errors("rel-not-rel", head, "export class Plain { }\nexport class LikedBy extends RelationshipTarget(Plain as never) {}\n");
    assert!(e.iter().any(|m| m.contains("extends Relationship")), "{e:?}");
    let target = format!("{pair}export class LikedBy extends RelationshipTarget(Likes) {{ static path = \"game::LikedBy\" }}\n");
    let e = errors("rel-mut-source", head, &format!("{target}export function s(q: Query<[Mut<Likes>]>) {{}}\napp.addSystems(Update, s);\n"));
    assert!(e.iter().any(|m| m.contains("game::Likes") && m.contains("insert(Likes.of({ target }))")), "{e:?}");
    let e = errors("rel-mut-target", head, &format!("{target}export function s(q: Query<[Mut<LikedBy>]>) {{}}\napp.addSystems(Update, s);\n"));
    assert!(e.iter().any(|m| m.contains("game::LikedBy") && m.contains("read-only")), "{e:?}");
}

// ---------------------------------------------------------------------------------------------
// Hooks

#[test]
fn insert_and_remove_keep_the_list() {
    let mut app = loaded("rel-insert");
    let (t, a, b) = (spawn(&mut app), spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    point(&mut app, b, "game::Likes", t);
    assert_eq!(list(&app, t, "game::LikedBy"), Some(vec![a, b]), "a Rust insert by id joins the list");
    unpoint(&mut app, a, "game::Likes");
    assert_eq!(list(&app, t, "game::LikedBy"), Some(vec![b]));
    unpoint(&mut app, b, "game::Likes");
    assert_eq!(list(&app, t, "game::LikedBy"), None, "an empty list removes the target");
}

#[test]
fn removing_the_target_removes_its_sources() {
    let mut app = loaded("rel-remove-target");
    let (t, a) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    let liked_by = id(&app, "game::LikedBy");
    app.world_mut().entity_mut(t).remove_by_id(liked_by);
    app.world_mut().flush();
    assert_eq!(read_ts(&app, a, "game::Likes"), None);
}

#[test]
fn linked_despawn_takes_the_sources_and_unlinked_leaves_them() {
    let mut app = loaded("rel-despawn");
    let (t, a, b) = (spawn(&mut app), spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::OwnedBy", t);
    point(&mut app, b, "game::OwnedBy", t);
    app.world_mut().entity_mut(t).despawn();
    app.world_mut().flush();
    assert!(app.world().get_entity(a).is_err() && app.world().get_entity(b).is_err(), "a linked target despawns its sources");
    let (t, a) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    app.world_mut().entity_mut(t).despawn();
    app.world_mut().flush();
    assert!(app.world().get_entity(a).is_ok(), "an unlinked target's sources survive");
    assert_eq!(read_ts(&app, a, "game::Likes"), None, "without their source");
}

#[test]
fn self_reference_and_missing_target_are_removed() {
    let mut app = loaded("rel-self");
    let a = spawn(&mut app);
    point(&mut app, a, "game::Likes", a);
    assert_eq!(read_ts(&app, a, "game::Likes"), None, "a self-reference is removed");
    point(&mut app, a, "game::Mirror", a);
    assert_eq!(target_of(&app, a, "game::Mirror"), Some(a), "allowSelfReferential keeps it");
    assert_eq!(list(&app, a, "game::Mirrored"), Some(vec![a]));
    let gone = spawn(&mut app);
    app.world_mut().entity_mut(gone).despawn();
    let b = spawn(&mut app);
    point(&mut app, b, "game::Likes", gone);
    assert_eq!(read_ts(&app, b, "game::Likes"), None, "a missing target removes the source");
}

#[test]
fn ts_commands_insert_and_retarget() {
    let body = r#"let step = 0;
export function go(c: Commands) {
  step++;
  if (step === 1) c.entity($0).insert(Likes.of({ target: $1 }));
  if (step === 2) c.entity($0).insert(Likes.of({ target: $2 }));
}
app.addSystems(Update, go);
"#;
    let (_, mut app, es) = staged("rel-retarget", |app| (0..3).map(|_| spawn(app)).collect(), body);
    let (a, b, c) = (es[0], es[1], es[2]);
    app.update();
    app.world_mut().flush();
    assert_eq!(list(&app, b, "game::LikedBy"), Some(vec![a]));
    app.update();
    app.world_mut().flush();
    assert_eq!(list(&app, b, "game::LikedBy"), None, "the old target loses it");
    assert_eq!(list(&app, c, "game::LikedBy"), Some(vec![a]), "the new one gains it");
}

#[test]
fn a_tsx_insert_joins_the_list() {
    let body = "app.scene(\"s\", () => <entity key=\"a\"><Likes target={$0} /></entity>);\n";
    let (_, mut app, es) = staged("rel-tsx", |app| vec![spawn(app)], body);
    app.update();
    app.world_mut().flush();
    let l = list(&app, es[0], "game::LikedBy").expect("the TSX source joined the list");
    assert_eq!(l.len(), 1);
    assert_eq!(target_of(&app, l[0], "game::Likes"), Some(es[0]));
}

#[test]
fn a_required_component_and_a_source_share_one_registration() {
    let body = r#"export class Fan extends Relationship { static path = "game::Fan"; static require = [Health]; declare target: Entity }
export class Fans extends RelationshipTarget(Fan) { static path = "game::Fans" }
"#;
    let (_, mut app, _) = staged("rel-require", |_| vec![], body);
    let (t, a) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Fan", t);
    assert_eq!(list(&app, t, "game::Fans"), Some(vec![a]), "the source's hooks ran");
    assert!(read_ts(&app, a, "game::Health").is_some(), "and its required component came");
}

// ---------------------------------------------------------------------------------------------
// Clone

#[test]
fn clone_a_source_joins_the_targets_list() {
    let body = r#"let done = false;
export function cl(c: Commands) { if (done) return; done = true; console.log(`clone ${c.entity($0).cloneAndSpawn()}`); }
app.addSystems(Update, cl);
"#;
    let (_, mut app, es) = staged(
        "rel-clone-source",
        |app| {
            let (a, t) = (spawn(app), spawn(app));
            point(app, a, "game::Likes", t);
            vec![a, t]
        },
        body,
    );
    app.update();
    app.world_mut().flush();
    let clone = Entity::try_from_bits(lines(&mut app, "clone")[0].strip_prefix("clone ").unwrap().parse().unwrap()).unwrap();
    assert_eq!(target_of(&app, clone, "game::Likes"), Some(es[1]));
    assert_eq!(sorted(list(&app, es[1], "game::LikedBy").unwrap()), sorted(vec![es[0], clone]));
}

#[test]
fn linked_clone_and_move_of_a_target() {
    let mut app = loaded("rel-clone-target");
    let (t, a) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::OwnedBy", t);
    // Linked cloning clones the sources (their hooks run if not linked, so the list is built once).
    let world = app.world_mut();
    let mut builder = EntityCloner::build_opt_out(world);
    builder.linked_cloning(true);
    let t2 = builder.finish().spawn_clone(world, t);
    app.world_mut().flush();
    let owned = list(&app, t2, "game::Owns").expect("the clone owns a clone of a");
    assert_eq!(owned.len(), 1);
    let a2 = owned[0];
    assert_ne!(a2, a);
    assert_eq!(target_of(&app, a2, "game::OwnedBy"), Some(t2));
    assert_eq!(list(&app, t, "game::Owns"), Some(vec![a]), "a is still owned by t");
    // An unlinked target's clone starts empty, so it is not inserted.
    let (u, b) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, b, "game::Likes", u);
    let world = app.world_mut();
    let u2 = EntityCloner::build_opt_out(world).finish().spawn_clone(world, u);
    app.world_mut().flush();
    assert_eq!(list(&app, u2, "game::LikedBy").unwrap_or_default(), vec![]);
    assert_eq!(list(&app, u, "game::LikedBy"), Some(vec![b]));
    // Moving a target retargets its sources, with no hook.
    let world = app.world_mut();
    let moved = world.spawn_empty().id();
    let mut builder = EntityCloner::build_opt_out(world);
    builder.move_components(true);
    builder.finish().clone_entity(world, u, moved);
    app.world_mut().flush();
    assert_eq!(list(&app, moved, "game::LikedBy"), Some(vec![b]));
    assert_eq!(target_of(&app, b, "game::Likes"), Some(moved));
    assert_eq!(list(&app, u, "game::LikedBy"), None);
}

// ---------------------------------------------------------------------------------------------
// Reads

#[test]
fn a_query_reads_target_lists() {
    let body = r#"export function read(q: Query<[Entity, LikedBy]>, o: Query<[Entity, Option<LikedBy>], With<Health>>, ch: Query<[Entity], Changed<LikedBy>>) {
  const rows: string[] = [];
  q.each((e, l) => rows.push(`${e}=${l.length}:${l.toArray().join("/")}:${l.at(-1)}:${l.includes(l.at(0)!)}`));
  const opt: string[] = [];
  o.each((e, l) => opt.push(`${e}=${l === undefined ? "none" : [...l].length}`));
  console.log(`read ${rows.sort().join(" ")} | ${opt.sort().join(" ")} | ${ch.len}`);
}
app.addSystems(PostUpdate, read);
"#;
    let (_, mut app, es) = staged(
        "rel-read",
        |app| {
            // t0 lists one, t1 lists three, in one table; h has Health and no list.
            let es: Vec<Entity> = (0..6).map(|_| spawn(app)).collect();
            point(app, es[2], "game::Likes", es[0]);
            for s in &es[3..6] {
                point(app, *s, "game::Likes", es[1]);
            }
            let h = spawn_ts(app, "game::Health", json!({}), ());
            let mut es = es;
            es.push(h);
            es
        },
        body,
    );
    app.update();
    let b = |e: Entity| e.to_bits().to_string();
    let row = |t: Entity, ss: &[Entity]| format!("{}={}:{}:{}:true", b(t), ss.len(), ss.iter().map(|s| b(*s)).collect::<Vec<_>>().join("/"), b(*ss.last().unwrap()));
    let mut rows = vec![row(es[0], &es[2..3]), row(es[1], &es[3..6])];
    rows.sort();
    let first = format!("read {} | {}=none | 2", rows.join(" "), b(es[6]));
    assert_eq!(lines(&mut app, "read"), vec![first.clone()]);
    // A flush between runs changes a list: the next run reads it, and `Changed` sees that row.
    unpoint(&mut app, es[4], "game::Likes");
    point(&mut app, es[2], "game::Likes", es[6]);
    app.update();
    let mut rows = vec![row(es[1], &[es[3], es[5]]), row(es[6], &es[2..3])];
    rows.sort();
    let got = lines(&mut app, "read");
    assert_eq!(got[1], format!("read {} | {}=1 | 2", rows.join(" "), b(es[6])));
}

// ---------------------------------------------------------------------------------------------
// Hot reload

/// Every target list as (target, sorted sources).
fn lists(app: &mut App, target: &str) -> BTreeMap<Entity, Vec<Entity>> {
    let tid = id(app, target);
    let world = app.world_mut();
    let es: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(tid).build().iter(world).collect();
    es.into_iter().map(|e| (e, sorted(list(app, e, target).unwrap()))).collect()
}

fn reload_with(app: &mut App, dir: &std::path::Path, head: &str) {
    write(dir, "game.tsx", head);
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.world_mut().flush();
}

#[test]
fn a_reload_rebuilds_lists_when_a_pair_changes() {
    let (dir, mut app, _) = staged("rel-reload", |_| vec![], "");
    let (t, a, b) = (spawn(&mut app), spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    point(&mut app, b, "game::Likes", t);
    let (o, c) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, c, "game::OwnedBy", o);
    let before = lists(&mut app, "game::LikedBy");
    let old = (id(&app, "game::Likes"), id(&app, "game::LikedBy"));
    // No pair change: the ids, every list and its tick stay.
    let tick = app.world().entity(t).get_change_ticks_by_id(old.1).unwrap().changed;
    reload_with(&mut app, &dir, &src("export const unrelated = 1;\n"));
    assert_eq!((id(&app, "game::Likes"), id(&app, "game::LikedBy")), old);
    assert_eq!(lists(&mut app, "game::LikedBy"), before);
    assert_eq!(app.world().entity(t).get_change_ticks_by_id(old.1).unwrap().changed, tick, "the list was not touched");
    // A source gains a field: both get new ids, targets kept, lists rebuilt.
    let gained = src("").replace("declare target: Entity }\nexport class LikedBy", "declare target: Entity; since: f32 = 0 }\nexport class LikedBy");
    reload_with(&mut app, &dir, &gained);
    assert_ne!(id(&app, "game::Likes"), old.0);
    assert_ne!(id(&app, "game::LikedBy"), old.1);
    assert_eq!(target_of(&app, a, "game::Likes"), Some(t));
    assert_eq!(lists(&mut app, "game::LikedBy"), before);
    // A target becomes linked: same lists, and despawning it now takes its sources.
    let linked = gained.replace("RelationshipTarget(Likes)", "RelationshipTarget(Likes, { linkedSpawn: true })");
    reload_with(&mut app, &dir, &linked);
    assert_eq!(lists(&mut app, "game::LikedBy"), before);
    assert_eq!(lists(&mut app, "game::Owns"), BTreeMap::from([(o, vec![c])]), "an unchanged pair keeps its lists");
    app.world_mut().entity_mut(t).despawn();
    app.world_mut().flush();
    assert!(app.world().get_entity(a).is_err() && app.world().get_entity(b).is_err());
}

#[test]
fn a_plain_component_becomes_a_source() {
    let plain = r#"import { app, Component, Relationship, RelationshipTarget } from "bevy-ts";
import type { Entity } from "bevy-ts";
export class Likes extends Component { static path = "game::Likes"; declare target: Entity }
"#;
    let dir = project("rel-plain-to-source", &[("game.tsx", plain)]);
    let mut app = app(&dir.join("game.tsx"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let (t, a, b) = (spawn(&mut app), spawn(&mut app), spawn(&mut app));
    insert_ts(&mut app, a, "game::Likes", json!({ "target": t.to_bits() }));
    insert_ts(&mut app, b, "game::Likes", json!({ "target": t.to_bits() }));
    assert_eq!(list_opt(&app, t), None);
    let source = plain.replace("extends Component", "extends Relationship") + "export class LikedBy extends RelationshipTarget(Likes) { static path = \"game::LikedBy\" }\n";
    reload_with(&mut app, &dir, &source);
    assert_eq!(target_of(&app, a, "game::Likes"), Some(t));
    assert_eq!(sorted(list(&app, t, "game::LikedBy").unwrap()), sorted(vec![a, b]));
}

#[test]
fn a_source_turned_plain_drops_its_deleted_linked_target() {
    let (dir, mut app, _) = staged("rel-source-to-plain", |_| vec![], "");
    let (o, c) = (spawn(&mut app), spawn(&mut app));
    point(&mut app, c, "game::OwnedBy", o);
    let owns = id(&app, "game::Owns");
    let plain = src("")
        .replace("export class OwnedBy extends Relationship", "export class OwnedBy extends Component")
        .replace("export class Owns extends RelationshipTarget(OwnedBy, { linkedSpawn: true }) { static path = \"game::Owns\" }\n", "");
    reload_with(&mut app, &dir, &plain);
    assert!(!app.world().entity(o).contains_id(owns), "the stale list is gone");
    app.world_mut().entity_mut(o).despawn();
    app.world_mut().flush();
    assert!(app.world().get_entity(c).is_ok(), "no longer a source, so not despawned with o");
}

fn list_opt(app: &App, e: Entity) -> Option<Vec<Entity>> {
    bevy_ts::plugin_api::component_by_name(app.world(), "game::LikedBy").and_then(|_| list(app, e, "game::LikedBy"))
}

// ---------------------------------------------------------------------------------------------
// Protocol

fn rpc(app: &mut App, method: &str, params: serde_json::Value) -> Result<serde_json::Value, bevy::remote::BrpError> {
    use bevy::remote::{RemoteMethodSystemId, RemoteMethods};
    let id = *app.world().resource::<RemoteMethods>().get(method).unwrap_or_else(|| panic!("{method} not served"));
    match id {
        RemoteMethodSystemId::Instant(id) => app.world_mut().run_system_with(id, Some(params)).unwrap(),
        RemoteMethodSystemId::Watching(id) => app.world_mut().run_system_with(id, Some(params)).unwrap().map(|v| v.unwrap_or_default()),
    }
}

#[test]
fn the_protocol_reads_targets_and_retargets_sources() {
    let dir = project("rel-protocol", &[("game.tsx", &src(""))]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(bevy::remote::RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let (t, a, b, c) = (spawn(&mut app), spawn(&mut app), spawn(&mut app), spawn(&mut app));
    point(&mut app, a, "game::Likes", t);
    point(&mut app, b, "game::Likes", t);
    let got = rpc(&mut app, "bevy-ts/get_components", json!({ "entity": t.to_bits(), "components": ["game::LikedBy"] })).unwrap();
    assert_eq!(got["components"]["game::LikedBy"], json!([a.to_bits(), b.to_bits()]));
    let schema = rpc(&mut app, "bevy-ts/schema", json!({ "names": ["game::LikedBy", "game::Likes"] })).unwrap();
    assert_eq!(schema["game::LikedBy"]["x-bevy-ts"]["relationship"], json!({ "role": "target", "counterpart": "game::Likes" }));
    assert_eq!(schema["game::Likes"]["x-bevy-ts"]["relationship"], json!({ "role": "source", "counterpart": "game::LikedBy" }));
    assert_eq!(schema["game::LikedBy"]["x-bevy-ts"]["layout"]["fields"], json!([]));
    // Retargeting: the write re-inserts the source, so its hooks move `a`.
    rpc(&mut app, "bevy-ts/mutate_components", json!({ "entity": a.to_bits(), "component": "game::Likes", "path": ".target", "value": c.to_bits() })).unwrap();
    app.world_mut().flush();
    assert_eq!(list(&app, t, "game::LikedBy"), Some(vec![b]));
    assert_eq!(list(&app, c, "game::LikedBy"), Some(vec![a]));
    let refused = rpc(&mut app, "bevy-ts/mutate_components", json!({ "entity": t.to_bits(), "component": "game::LikedBy", "path": "", "value": [] })).unwrap_err();
    assert!(refused.message.contains("game::LikedBy") && refused.message.contains("read-only"), "{refused:?}");
}

// ---------------------------------------------------------------------------------------------
// Saves

#[test]
fn a_world_save_skips_targets() {
    let dir = project("rel-save", &[("game.tsx", &src(""))]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default());
    app.insert_resource(bevy_ts::saves::SaveDir(dir.join("saves")));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let t = app.world_mut().spawn(bevy_ts::saves::Save).id();
    let a = app.world_mut().spawn(bevy_ts::saves::Save).id();
    point(&mut app, a, "game::Likes", t);
    assert!(list(&app, t, "game::LikedBy").is_some());
    bevy_ts::saves::save(app.world_mut(), "rel", Default::default());
    let file = dir.join("saves/rel.scn.ron");
    for _ in 0..500 {
        app.update();
        if std::fs::read_to_string(&file).is_ok_and(|t| !t.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert!(text.contains("game::Likes"), "the source is saved: {text}");
    assert!(!text.contains("game::LikedBy"), "the target is derived, never saved: {text}");
}

// ---------------------------------------------------------------------------------------------
// Property: random structural changes keep every list right and never panic in a hook

/// A small deterministic generator: the test needs no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Each live source points at a live entity whose target lists it exactly once, and every list
/// holds exactly the sources pointing there, none empty.
fn check_pair(app: &mut App, source: &str, target: &str, step: &str) {
    let sid = id(app, source);
    let world = app.world_mut();
    let sources: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(sid).build().iter(world).collect();
    let mut want: BTreeMap<Entity, Vec<Entity>> = BTreeMap::new();
    for s in sources {
        let t = target_of(app, s, source).unwrap();
        assert!(app.world().get_entity(t).is_ok(), "{step}: {s} points at a despawned {t}");
        want.entry(t).or_default().push(s);
    }
    want.values_mut().for_each(|v| v.sort());
    assert_eq!(lists(app, target), want, "{step}: {target} lists differ from {source} sources");
}

#[test]
fn random_structural_changes_keep_lists_right() {
    let (dir, mut app, _) = staged("rel-property", |_| vec![], "");
    let heads = [src(""), src("").replace("declare target: Entity }\nexport class LikedBy", "declare target: Entity; since: f32 = 0 }\nexport class LikedBy")];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut pool: Vec<Entity> = (0..8).map(|_| spawn(&mut app)).collect();
    let mut head = 0;
    for step in 0..400 {
        let pick = |rng: &mut Rng, pool: &[Entity]| pool[rng.below(pool.len())];
        let what = match rng.below(20) {
            0..=2 => {
                pool.push(spawn(&mut app));
                "spawn"
            }
            3..=4 if pool.len() > 4 => {
                let e = pool.swap_remove(rng.below(pool.len()));
                app.world_mut().entity_mut(e).despawn();
                "despawn"
            }
            5..=11 => {
                let (s, t) = (pick(&mut rng, &pool), pick(&mut rng, &pool));
                let pair = if rng.below(3) == 0 { "game::OwnedBy" } else { "game::Likes" };
                insert_ts(&mut app, s, pair, json!({ "target": t.to_bits() }));
                "retarget"
            }
            12..=14 => {
                let s = pick(&mut rng, &pool);
                let sid = id(&app, "game::Likes");
                app.world_mut().entity_mut(s).remove_by_id(sid);
                "remove source"
            }
            15..=16 => {
                let t = pick(&mut rng, &pool);
                let tid = id(&app, "game::LikedBy");
                app.world_mut().entity_mut(t).remove_by_id(tid);
                "remove target"
            }
            17 if step % 40 < 2 => {
                head = 1 - head;
                reload_with(&mut app, &dir, &heads[head]);
                "hot reload"
            }
            _ => {
                app.update();
                "frame"
            }
        };
        app.world_mut().flush();
        // A linked despawn may have taken pool entities with it.
        pool.retain(|e| app.world().get_entity(*e).is_ok());
        while pool.len() < 4 {
            pool.push(spawn(&mut app));
        }
        let at = format!("step {step} ({what})");
        check_pair(&mut app, "game::Likes", "game::LikedBy", &at);
        check_pair(&mut app, "game::OwnedBy", "game::Owns", &at);
    }
}

// ---------------------------------------------------------------------------------------------
// Bench (task 5.4): 10,000 rows of a target listing 1, 4 or 16 entities, read whole in a TS query,
// against the same entities as an inline array of `Entity` fields on a plain component. Each arm's
// time is its system's run time less an empty system's over the same rows, per row, median of
// the measured frames. Ignored by default; run it in release:
// `BENCH_RUNS=3 cargo static test --release -p bevy-ts-cli --test it relationships::bench -- --ignored --nocapture`.

const ROWS: usize = 10_000;
const FRAMES: usize = 200;

/// Median run time in microseconds of each named system over `FRAMES` frames.
fn medians(app: &mut App, names: &[&str]) -> Vec<f64> {
    let mut us: Vec<Vec<f64>> = vec![Vec::new(); names.len()];
    for _ in 0..FRAMES {
        app.update();
        let systems = app.world().resource::<bevy_ts::systems::TsSystems>();
        for (k, n) in names.iter().enumerate() {
            let s = systems.iter().find(|s| s.decl.name.rsplit('#').next() == Some(*n)).expect("system");
            assert!(s.last_error().is_none(), "{n}: {:?}", s.last_error());
            us[k].push(s.last_run_us() as f64);
        }
    }
    us.into_iter()
        .map(|mut v| {
            v.sort_by(f64::total_cmp);
            v[FRAMES / 2]
        })
        .collect()
}

#[test]
#[ignore]
fn bench_packed_target_reads() {
    let runs: usize = std::env::var("BENCH_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let mut table = Vec::new();
    for per in [1usize, 4, 16] {
        let fields: String = (0..per).map(|i| format!("declare e{i}: Entity; ")).collect();
        let sum: String = (0..per).map(|i| format!("n += v.e{i};")).collect();
        let body = format!(
            r#"export class Inline extends Component {{ static path = "game::Inline"; {fields}}}
export class Row extends Component {{ static path = "game::Row" }}
let sink = 0;
export function bare(q: Query<[Row]>) {{ q.each(() => {{ sink++; }}); }}
export function empty(q: Query<[LikedBy]>) {{ q.each(() => {{ sink++; }}); }}
export function packed(q: Query<[LikedBy]>) {{ let n = 0; q.each((l) => {{ for (let i = 0; i < l.length; i++) n += l.at(i)!; }}); sink += n; }}
export function emptyInline(q: Query<[Inline]>) {{ q.each(() => {{ sink++; }}); }}
export function inline(q: Query<[Inline]>) {{ let n = 0; q.each((v) => {{ {sum} }}); sink += n; }}
app.addSystems(Update, bare, empty, packed, emptyInline, inline);
"#
        );
        let (_, mut app, _) = staged(&format!("rel-bench-{per}"), |_| vec![], &body);
        for _ in 0..ROWS {
            let t = spawn(&mut app);
            for _ in 0..per {
                let s = spawn(&mut app);
                insert_ts(&mut app, s, "game::Likes", json!({ "target": t.to_bits() }));
            }
            insert_ts(&mut app, t, "game::Inline", json!({}));
            insert_ts(&mut app, t, "game::Row", json!({}));
        }
        app.world_mut().flush();
        updates(&mut app, 20);
        let ns = |us: f64| us * 1000.0 / ROWS as f64;
        let (mut pack, mut column, mut packed, mut inline) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for _ in 0..runs {
            let m = medians(&mut app, &["empty", "packed", "emptyInline", "inline", "bare"]);
            // What naming a target costs over naming a field-less marker on the same rows: the
            // packing and its frame.
            pack.push(ns(m[0] - m[4]));
            column.push(ns(m[2] - m[4]));
            packed.push(ns(m[1] - m[0]));
            inline.push(ns(m[3] - m[2]));
        }
        table.push((per, pack, column, packed, inline));
    }
    println!("| entities per row | target over a marker (pack), ns/row | inline column over a marker, ns/row | packed JS read, ns/row | inline JS read, ns/row |\n|---|---|---|---|---|");
    for (per, k, c, p, i) in table {
        let f = |v: &[f64]| v.iter().map(|x| format!("{x:.1}")).collect::<Vec<_>>().join(", ");
        println!("| {per} | {} | {} | {} | {} |", f(&k), f(&c), f(&p), f(&i));
    }
}
