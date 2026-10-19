//! The reconciler against the fake backend: scene descriptions arrive in the `tsx` section the
//! way JS writes them, generations apply, and the world changes only where descriptions did.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::{ModuleSet, ScriptError, SourceLocation};
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, Generations, ReloadReport, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{ComponentDecl, FieldDecl, Manifest, Section};
use bevy_ts::plugin_api;
use bevy_ts_tsx::tree::ReconcileStats;
use bevy_ts_tsx::{Provenance, TsxNode, TsxPlugin, TsxTree};
use serde_json::{Value, json};
use std::path::PathBuf;

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    (app, handle)
}

fn health() -> ComponentDecl {
    ComponentDecl {
        name: "game::Health".into(),
        fields: vec![
            FieldDecl { name: "hp".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
            FieldDecl { name: "max".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
        ],
        location: None,
        ..Default::default()
    }
}

fn at(file: &str, line: u32) -> Value {
    json!({ "file": file, "line": line, "column": 3 })
}

/// What `<Enemy key at hp />` describes, as the JSX runtime writes it.
fn enemy(key: &str, x: f32, hp: f32) -> Value {
    enemy_with(key, x, hp, json!({ "initialHp": hp, "max": hp }))
}

fn enemy_with(key: &str, x: f32, _hp: f32, health_props: Value) -> Value {
    json!({
        "key": key,
        "type": "Enemy",
        "name": "enemy",
        "source": [at("enemy.tsx", 4), at("game.tsx", 12)],
        "components": [
            { "type": "Transform", "kind": "rust", "props": { "translation": [x, 0, -4] }, "source": at("enemy.tsx", 5) },
            { "type": "game::Health", "kind": "ts", "props": health_props, "source": at("enemy.tsx", 6) },
            { "type": "mesh", "kind": "intrinsic", "props": { "shape": "cuboid", "size": [1, 1, 1], "color": "#c33" }, "source": at("enemy.tsx", 7) }
        ],
        "children": []
    })
}

fn scene(nodes: Vec<Value>) -> Value {
    json!({ "roots": [{ "name": "arena", "location": at("game.tsx", 10), "nodes": nodes }] })
}

fn program(section: &Value) -> FakeProgram {
    FakeProgram {
        manifest: Manifest { components: vec![health()], ..Default::default() },
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(section).unwrap(), location: None }],
        ..Default::default()
    }
}

fn load(app: &mut App, fake: &FakeHandle, section: &Value) -> ReloadReport {
    fake.push(program(section));
    load_pushed(app, false)
}

fn load_pushed(app: &mut App, restart: bool) -> ReloadReport {
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, restart)
}

fn by_key(app: &mut App, key: &str) -> Option<Entity> {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path.last().map(String::as_str) == Some(key)).map(|(e, _)| e)
}

fn hp(app: &App, e: Entity) -> (f32, f32) {
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let ptr = app.world().get_by_id(e, id).unwrap();
    // SAFETY: Health is two f32s.
    let b = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 8) };
    (f32::from_le_bytes(b[0..4].try_into().unwrap()), f32::from_le_bytes(b[4..8].try_into().unwrap()))
}

fn set_hp(app: &mut App, e: Entity, v: f32) {
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let mut m = app.world_mut().get_mut_by_id(e, id).unwrap();
    // SAFETY: Health is two f32s.
    unsafe { std::ptr::copy_nonoverlapping(v.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
}

fn last(app: &App) -> ReconcileStats {
    app.world().resource::<TsxTree>().last.clone()
}

fn applied(r: &ReloadReport) {
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

#[test]
fn two_enemies_mount_from_a_description() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0), enemy("b", 3.0, 250.0)])));
    let (a, b) = (by_key(&mut app, "a").unwrap(), by_key(&mut app, "b").unwrap());
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation, Vec3::new(-3.0, 0.0, -4.0));
    assert_eq!(app.world().get::<Transform>(b).unwrap().translation, Vec3::new(3.0, 0.0, -4.0));
    assert_eq!(hp(&app, b), (250.0, 250.0));
    assert_eq!(app.world().get::<Name>(a).unwrap().as_str(), "enemy");
    assert!(app.world().get::<Mesh3d>(a).is_some());
    assert!(app.world().get::<MeshMaterial3d<StandardMaterial>>(a).is_some());
    let node = app.world().get::<TsxNode>(b).unwrap();
    assert_eq!(node.key_path, vec!["b"]);
    assert_eq!(node.source[1], SourceLocation { file: "game.tsx".into(), line: 12, column: 3 });
    assert_eq!(last(&app), ReconcileStats { spawned: 2, ..Default::default() });
}

#[test]
fn a_second_description_writes_only_what_changed() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0), enemy("b", 3.0, 250.0)])));
    let b = by_key(&mut app, "b").unwrap();
    let a = by_key(&mut app, "a").unwrap();
    applied(&load(&mut app, &fake, &scene(vec![enemy("b", 5.0, 250.0), enemy("c", 0.0, 100.0)])));
    assert_eq!(last(&app), ReconcileStats { spawned: 1, despawned: 1, written: 1, ..Default::default() });
    assert_eq!(by_key(&mut app, "b"), Some(b), "b keeps its entity");
    assert!(app.world().get_entity(a).is_err(), "a is despawned");
    assert_eq!(app.world().get::<Transform>(b).unwrap().translation.x, 5.0);
    assert!(by_key(&mut app, "c").is_some());
}

#[test]
fn an_identical_generation_writes_nothing_and_moves_no_tick() {
    let (mut app, fake) = app();
    let s = scene(vec![enemy("a", -3.0, 100.0)]);
    applied(&load(&mut app, &fake, &s));
    let a = by_key(&mut app, "a").unwrap();
    app.update();
    let (hid, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let tid = app.world().components().component_id::<Transform>().unwrap();
    let nid = app.world().components().component_id::<TsxNode>().unwrap();
    let ticks = |app: &App| [hid, tid, nid].map(|id| plugin_api::change_ticks(app.world(), a, id).unwrap().changed);
    let before = ticks(&app);
    applied(&load(&mut app, &fake, &s));
    assert!(last(&app).is_empty(), "{:?}", last(&app));
    assert_eq!(ticks(&app), before);
}

#[test]
fn a_system_moved_controlled_field_stays_until_the_declaration_changes() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0)])));
    let a = by_key(&mut app, "a").unwrap();
    app.world_mut().get_mut::<Transform>(a).unwrap().translation.y = 7.0;
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0)])));
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.y, 7.0, "unchanged declaration: not written");
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 1.0, 100.0)])));
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation, Vec3::new(1.0, 0.0, -4.0));
}

#[test]
fn max_follows_the_source_and_hp_stays_where_damage_left_it() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("b", 3.0, 250.0)])));
    let b = by_key(&mut app, "b").unwrap();
    set_hp(&mut app, b, 180.0);
    applied(&load(&mut app, &fake, &scene(vec![enemy("b", 3.0, 400.0)])));
    assert_eq!(hp(&app, b), (180.0, 400.0));
    assert_eq!(last(&app), ReconcileStats { written: 1, ..Default::default() });
    let node = app.world().get::<TsxNode>(b).unwrap();
    let h = &node.components["game::Health"];
    assert_eq!(h.fields["hp"].provenance, Provenance::Initial);
    assert_eq!(h.fields["hp"].applied, json!(250.0), "the value written on insert");
    assert_eq!(h.fields["max"].provenance, Provenance::Controlled);
    assert_eq!(h.fields["max"].applied, json!(400.0));
}

#[test]
fn components_systems_insert_survive_and_removed_elements_go() {
    let (mut app, fake) = app();
    #[derive(Component)]
    struct Stunned;
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 100.0)])));
    let a = by_key(&mut app, "a").unwrap();
    app.world_mut().entity_mut(a).insert(Stunned);
    // Health dropped from the description, a new Name-less entity otherwise identical.
    let mut e = enemy("a", 0.0, 100.0);
    e["components"].as_array_mut().unwrap().remove(1);
    applied(&load(&mut app, &fake, &scene(vec![e])));
    let (hid, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    assert!(app.world().get::<Stunned>(a).is_some());
    assert!(app.world().get_by_id(a, hid).is_none(), "tsx removed what it inserted");
    assert_eq!(last(&app), ReconcileStats { removed: 1, ..Default::default() });
    // And back: inserted with its initial values.
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 60.0)])));
    assert_eq!(hp(&app, a), (60.0, 60.0));
    assert_eq!(last(&app).inserted, 1);
}

#[test]
fn a_system_despawned_node_stays_dead_until_restart() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0), enemy("b", 3.0, 100.0)])));
    let a = by_key(&mut app, "a").unwrap();
    set_hp(&mut app, a, 10.0);
    app.world_mut().despawn(a);
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0), enemy("b", 3.0, 100.0)])));
    assert!(by_key(&mut app, "a").is_none(), "not respawned");
    let tree = app.world().resource::<TsxTree>();
    assert_eq!(tree.tombstones.len(), 1);
    assert_eq!(tree.tombstones.values().next().unwrap().key_path, vec!["a"]);
    // A key change is a new node.
    applied(&load(&mut app, &fake, &scene(vec![enemy("a2", -3.0, 100.0), enemy("b", 3.0, 100.0)])));
    assert!(by_key(&mut app, "a2").is_some());
    assert!(app.world().resource::<TsxTree>().tombstones.is_empty(), "the old key path is gone");
    // Restart mounts everything as if first.
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0)])));
    let a = by_key(&mut app, "a");
    assert!(a.is_some());
    let a = a.unwrap();
    app.world_mut().despawn(a);
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", -3.0, 100.0)])));
    assert!(by_key(&mut app, "a").is_none());
    fake.push(program(&scene(vec![enemy("a", -3.0, 100.0)])));
    applied(&load_pushed(&mut app, true));
    let a = by_key(&mut app, "a").expect("back after restart");
    assert_eq!(hp(&app, a), (100.0, 100.0));
    assert!(app.world().resource::<TsxTree>().tombstones.is_empty());
}

#[test]
fn moved_lines_and_a_renamed_prefab_keep_entities_and_write_nothing() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("b", 3.0, 100.0)])));
    let b = by_key(&mut app, "b").unwrap();
    let mut moved = enemy("b", 3.0, 100.0);
    moved["type"] = json!("Grunt");
    moved["source"] = json!([at("grunt.tsx", 9), at("game.tsx", 14)]);
    applied(&load(&mut app, &fake, &scene(vec![moved])));
    assert_eq!(by_key(&mut app, "b"), Some(b));
    assert!(last(&app).is_empty(), "{:?}", last(&app));
    let node = app.world().get::<TsxNode>(b).unwrap();
    assert_eq!(node.types, vec!["Grunt"]);
    assert_eq!(node.source[1].line, 14, "locations follow the applied generation");
}

#[test]
fn a_bad_field_fails_the_generation_naming_it_and_its_line() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 100.0)])));
    let a = by_key(&mut app, "a").unwrap();
    let r = load(&mut app, &fake, &scene(vec![enemy_with("a", 9.0, 100.0, json!({ "mx": 100 }))]));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("no field mx"), "{}", r.errors[0].message);
    assert_eq!(r.errors[0].location.as_ref().unwrap().file, "enemy.tsx");
    assert_eq!(r.errors[0].location.as_ref().unwrap().line, 6);
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 0.0, "nothing applied");
    assert_eq!(app.world().resource::<Generations>().live, 1);
    // The next good save applies normally.
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 9.0, 100.0)])));
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation.x, 9.0);
}

#[test]
fn a_field_given_both_ways_fails_naming_the_field() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &scene(vec![enemy_with("a", 0.0, 10.0, json!({ "hp": 10, "initialHp": 10 }))]));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("game::Health.hp"), "{}", r.errors[0].message);
}

#[test]
fn unknown_components_wrong_types_and_duplicates_fail() {
    let (mut app, fake) = app();
    let mut e = enemy("a", 0.0, 1.0);
    e["components"][1]["type"] = json!("game::Nope");
    let r = load(&mut app, &fake, &scene(vec![e]));
    assert!(r.errors[0].message.contains("unknown component game::Nope"), "{:?}", r.errors);
    let r = load(&mut app, &fake, &scene(vec![enemy_with("a", 0.0, 1.0, json!({ "max": "lots" }))]));
    assert!(r.errors[0].message.contains("max"), "{:?}", r.errors);
    let r = load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 1.0), enemy("a", 1.0, 1.0)]));
    assert!(r.errors[0].message.contains("key \"a\""), "{:?}", r.errors);
    let mut e = enemy("a", 0.0, 1.0);
    let t = e["components"][0].clone();
    e["components"].as_array_mut().unwrap().push(t);
    let r = load(&mut app, &fake, &scene(vec![e]));
    assert!(r.errors[0].message.contains("Transform is given twice"), "{:?}", r.errors);
    assert_eq!(app.world_mut().query::<&TsxNode>().iter(app.world()).count(), 0);
}

#[test]
fn a_throwing_scene_fails_the_generation_and_leaves_the_world() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 100.0)])));
    let mut p = program(&scene(vec![]));
    p.load_error = Some(vec![ScriptError::new("Error: boom")]);
    fake.push(p);
    assert_eq!(load_pushed(&mut app, false).status, ReloadStatus::Failed);
    assert!(by_key(&mut app, "a").is_some());
}

#[test]
fn roots_missing_from_a_generation_unmount() {
    let (mut app, fake) = app();
    let two = json!({ "roots": [
        { "name": "arena", "nodes": [enemy("a", 0.0, 1.0)] },
        { "name": "extra", "nodes": [enemy("x", 0.0, 1.0)] }
    ]});
    applied(&load(&mut app, &fake, &two));
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 1.0)])));
    assert!(by_key(&mut app, "x").is_none());
    assert!(by_key(&mut app, "a").is_some());
    // A generation with no tsx section at all declares no scenes.
    fake.push(FakeProgram { manifest: Manifest { components: vec![health()], ..Default::default() }, ..Default::default() });
    applied(&load_pushed(&mut app, false));
    assert!(by_key(&mut app, "a").is_none());
    let r = load(&mut app, &fake, &json!({ "roots": [{ "name": "arena", "nodes": [] }, { "name": "arena", "nodes": [] }] }));
    assert!(r.errors[0].message.contains("declared twice"), "{:?}", r.errors);
}

#[test]
fn child_entities_spawn_under_their_parent_and_go_with_it() {
    let (mut app, fake) = app();
    let mut e = enemy("a", 0.0, 1.0);
    e["children"] = json!([{ "key": "eye", "type": "entity", "source": [at("enemy.tsx", 8)],
        "components": [{ "type": "Transform", "kind": "rust", "props": { "translation": [0, 0.6, 0.5] } }], "children": [] }]);
    applied(&load(&mut app, &fake, &scene(vec![e])));
    let a = by_key(&mut app, "a").unwrap();
    let eye = by_key(&mut app, "eye").unwrap();
    assert_eq!(app.world().get::<ChildOf>(eye).unwrap().parent(), a);
    assert_eq!(app.world().get::<TsxNode>(eye).unwrap().key_path, vec!["a", "eye"]);
    // A system parents its own entity under a; it goes when a's key path does.
    let extra = app.world_mut().spawn(ChildOf(a)).id();
    applied(&load(&mut app, &fake, &scene(vec![])));
    assert!(app.world().get_entity(eye).is_err());
    assert!(app.world().get_entity(extra).is_err());
}

#[test]
fn identical_meshes_share_one_asset() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 1.0), enemy("b", 1.0, 1.0)])));
    let (a, b) = (by_key(&mut app, "a").unwrap(), by_key(&mut app, "b").unwrap());
    assert_eq!(app.world().get::<Mesh3d>(a).unwrap().0, app.world().get::<Mesh3d>(b).unwrap().0);
    assert_eq!(app.world().resource::<bevy_ts_tsx::intrinsics::IntrinsicAssets>().built, 2, "one mesh, one material");
    // Recolouring writes the material on every enemy and keeps their state.
    set_hp(&mut app, a, 0.5);
    let mut ea = enemy("a", 0.0, 1.0);
    let mut eb = enemy("b", 1.0, 1.0);
    for e in [&mut ea, &mut eb] {
        e["components"][2]["props"]["color"] = json!("#3c3");
    }
    applied(&load(&mut app, &fake, &scene(vec![ea, eb])));
    assert_eq!(last(&app), ReconcileStats { written: 2, ..Default::default() });
    assert_eq!(hp(&app, a).0, 0.5);
}

fn with_entry(mut p: FakeProgram, name: &str, f: bevy_ts::fake::FakeFn) -> FakeProgram {
    p.manifest.entries.push(bevy_ts::manifest::EntryDecl { name: name.into(), location: None });
    p.entries.insert(name.into(), f);
    p
}

fn op(app: &App, kind: &str) -> u16 {
    app.world().resource::<bevy_ts::commands::CommandKinds>().opcode(kind).unwrap()
}

#[test]
fn a_handler_mounts_under_the_runtime_root_and_it_survives_reloads() {
    let (mut app, fake) = app();
    let (mount, unmount) = (op(&app, "tsx/mount"), op(&app, "tsx/unmount"));
    let payload = serde_json::to_vec(&json!({ "nodes": [enemy("e9", 0.0, 50.0)] })).unwrap();
    let bad = serde_json::to_vec(&json!({ "nodes": [enemy_with("e10", 0.0, 1.0, json!({ "mx": 1 }))] })).unwrap();
    let gone = serde_json::to_vec(&json!({ "key": "e9" })).unwrap();
    let p = with_entry(
        program(&scene(vec![enemy("a", 0.0, 100.0)])),
        "test/click",
        Box::new(move |call| {
            let which = call.payload.to_vec();
            let bytes = match which.as_slice() {
                b"mount" => &payload,
                b"bad" => &bad,
                _ => &gone,
            };
            call.commands.encoder.plugin(if which == b"unmount" { unmount } else { mount }, bytes);
            Ok(())
        }),
    );
    fake.push(p);
    applied(&load_pushed(&mut app, false));
    plugin_api::call_entry(app.world_mut(), "test/click", b"mount").unwrap();
    let e9 = by_key(&mut app, "e9").expect("mounted");
    let node = app.world().get::<TsxNode>(e9).unwrap();
    assert_eq!(node.root, "$runtime");
    assert_eq!(node.key_path, vec!["e9"]);
    assert_eq!(hp(&app, e9), (50.0, 50.0));

    // A reload reconciles the scene roots and leaves runtime mounts alone.
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 1.0, 100.0)])));
    assert_eq!(by_key(&mut app, "e9"), Some(e9));

    // A bad mount writes nothing and is logged.
    fake.push(with_entry(
        program(&scene(vec![enemy("a", 1.0, 100.0)])),
        "test/click",
        Box::new({
            let bad = serde_json::to_vec(&json!({ "nodes": [enemy_with("e10", 0.0, 1.0, json!({ "mx": 1 }))] })).unwrap();
            let gone = serde_json::to_vec(&json!({ "key": "e9" })).unwrap();
            move |call| {
                if call.payload == b"bad" {
                    call.commands.encoder.plugin(mount, &bad);
                } else {
                    call.commands.encoder.plugin(unmount, &gone);
                }
                Ok(())
            }
        }),
    ));
    applied(&load_pushed(&mut app, false));
    plugin_api::call_entry(app.world_mut(), "test/click", b"bad").unwrap();
    assert!(by_key(&mut app, "e10").is_none());
    let log: Vec<String> = app.world().resource::<bevy_ts::log::ScriptLog>().since(0).map(|e| e.message.clone()).collect();
    assert!(log.iter().any(|m| m.contains("no field mx")), "{log:?}");

    plugin_api::call_entry(app.world_mut(), "test/click", b"unmount").unwrap();
    assert!(app.world().get_entity(e9).is_err(), "unmounted");
    assert!(by_key(&mut app, "a").is_some());
}

#[test]
fn a_second_identical_enemy_reuses_the_resolved_template() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 100.0), enemy("b", 0.0, 100.0), enemy("c", 2.0, 100.0)])));
    let cache = app.world().resource::<bevy_ts_tsx::reconcile::TemplateCache>();
    assert_eq!((cache.hits, cache.misses), (1, 2), "b reuses a's; c differs in translation");
    let (a, b) = (by_key(&mut app, "a").unwrap(), by_key(&mut app, "b").unwrap());
    assert_ne!(a, b);
    assert_eq!(hp(&app, b), (100.0, 100.0));
}

#[test]
fn scenes_reconcile_after_migrations() {
    let (mut app, fake) = app();
    applied(&load(&mut app, &fake, &scene(vec![enemy("a", 0.0, 100.0)])));
    let a = by_key(&mut app, "a").unwrap();
    set_hp(&mut app, a, 42.0);
    // Health gains `armor`; the same save sets it on the element.
    let mut p = program(&scene(vec![enemy_with("a", 0.0, 100.0, json!({ "initialHp": 100, "max": 100, "armor": 5 }))]));
    p.manifest.components[0].fields.push(FieldDecl { name: "armor".into(), ty: FieldType::F32, default: Some(json!(0)), hints: Default::default() });
    fake.push(p);
    let r = load_pushed(&mut app, false);
    applied(&r);
    assert_eq!(r.migrations.len(), 1);
    let (id, layout) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let ptr = app.world().get_by_id(a, id).unwrap();
    // SAFETY: Health is now three f32s.
    let b = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) };
    let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let off = |n: &str| layout.field(n).unwrap().offset;
    assert_eq!((f(off("hp")), f(off("max")), f(off("armor"))), (42.0, 100.0, 5.0), "migrated hp kept, armor written");
}
