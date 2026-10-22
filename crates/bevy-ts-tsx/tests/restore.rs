//! What a Stop's restore uses: applying an external root at key paths only, and the scan of what
//! the game changed since it started.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{ComponentDecl, FieldDecl, Manifest};
use bevy_ts::plugin_api;
use bevy_ts_tsx::desc::NodeDesc;
use bevy_ts_tsx::reconcile::{AtMode, RNode, apply_at, find_node, resolve_tree};
use bevy_ts_tsx::restore;
use bevy_ts_tsx::{TsxPlugin, TsxTree};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

const ROOT: &str = "level:arena.bsn";

fn app() -> App {
    let fake = FakeHandle::default();
    let h = fake.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    let health = ComponentDecl {
        name: "game::Health".into(),
        fields: vec![
            FieldDecl { name: "hp".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
            FieldDecl { name: "max".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
        ],
        ..Default::default()
    };
    fake.push(FakeProgram { manifest: Manifest { components: vec![health], ..Default::default() }, ..Default::default() });
    let r = generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app
}

fn enemy(key: &str, x: f32) -> Value {
    json!({
        "key": key, "type": "entity", "name": key, "source": [],
        "components": [
            { "type": "Transform", "kind": "rust", "props": { "translation": [x, 0, -4] } },
            { "type": "game::Health", "kind": "ts", "props": { "initialHp": 50 } }
        ],
        "children": []
    })
}

/// `arena` holding enemies `a` to `d`.
fn level(x_of_b: f32) -> Vec<NodeDesc> {
    let arena = json!({
        "key": "arena", "type": "entity", "name": "arena", "source": [],
        "components": [{ "type": "Transform", "kind": "rust", "props": {} }],
        "children": [enemy("a", -3.0), enemy("b", x_of_b), enemy("c", 3.0), enemy("d", 6.0)]
    });
    serde_json::from_value(json!([arena])).unwrap()
}

fn p(s: &str) -> Vec<String> {
    s.split('/').map(String::from).collect()
}

fn entity(app: &App, path: &str) -> Option<Entity> {
    app.world().resource::<TsxTree>().entity(bevy_ts_tsx::desc::node_id(ROOT, &p(path))).filter(|e| app.world().get_entity(*e).is_ok())
}

fn kept(app: &App) -> Arc<[RNode]> {
    app.world().resource::<TsxTree>().roots[ROOT].resolved.clone().expect("kept")
}

fn mounted() -> App {
    let mut app = app();
    bevy_ts_tsx::reconcile_external(app.world_mut(), ROOT, None, &level(0.0)).unwrap();
    app
}

fn changed_ticks(app: &App, e: Entity) -> Vec<bevy::ecs::change_detection::Tick> {
    let er = app.world().entity(e);
    er.archetype().iter_components().filter_map(|c| er.get_change_ticks_by_id(c)).map(|t| t.changed).collect()
}

#[test]
fn respawning_one_enemy_keeps_its_siblings() {
    let mut app = mounted();
    let (a, b, c) = (entity(&app, "arena/a").unwrap(), entity(&app, "arena/b").unwrap(), entity(&app, "arena/c").unwrap());
    app.world_mut().get_mut::<Transform>(b).unwrap().translation.x = 9.0;
    let (ta, tc) = (changed_ticks(&app, a), changed_ticks(&app, c));
    app.world_mut().increment_change_tick();

    let k = kept(&app);
    let stats = apply_at(app.world_mut(), ROOT, &[(p("arena/b"), find_node(&k, &p("arena/b")))], AtMode::Respawn);
    assert_eq!((stats.despawned, stats.spawned), (1, 1));
    let b2 = entity(&app, "arena/b").unwrap();
    assert_ne!(b2, b, "b is a new entity");
    assert_eq!(app.world().get::<Transform>(b2).unwrap().translation.x, 0.0, "with its described value");
    assert_eq!((entity(&app, "arena/a"), entity(&app, "arena/c")), (Some(a), Some(c)));
    assert_eq!((changed_ticks(&app, a), changed_ticks(&app, c)), (ta, tc), "a and c are not written");
    let arena = entity(&app, "arena").unwrap();
    let order: Vec<Entity> = app.world().get::<Children>(arena).unwrap().iter().collect();
    assert_eq!(order[..3], [a, b2, c], "in b's place among its siblings");
    let tree = app.world().resource::<TsxTree>();
    let (_, change) = tree.journal.messages.back().unwrap();
    let b_id = bevy_ts_tsx::desc::node_id(ROOT, &p("arena/b"));
    assert!(change.removed.contains(&b_id) && change.added.contains(&b_id), "the watch sees b go and come");
    assert!(Arc::ptr_eq(&tree.roots[ROOT].resolved.clone().unwrap(), &k), "the kept tree is still the one applied");
}

#[test]
fn reconciling_one_path_writes_one_field() {
    let mut app = mounted();
    let b = entity(&app, "arena/b").unwrap();
    let nodes = level(5.0);
    let b_desc = nodes[0].children.iter().find(|n| n.key == "b").cloned().unwrap();
    let resolved = resolve_tree(app.world_mut(), ROOT, &[b_desc], (&p("arena"), &["entity".to_string()])).unwrap();
    let stats = apply_at(app.world_mut(), ROOT, &[(p("arena/b"), Some(&resolved[0]))], AtMode::Reconcile);
    assert_eq!(stats, bevy_ts_tsx::tree::ReconcileStats { written: 1, ..Default::default() });
    assert_eq!(entity(&app, "arena/b"), Some(b));
    assert_eq!(app.world().get::<Transform>(b).unwrap().translation.x, 5.0);
    let k = kept(&app);
    let kb = find_node(&k, &p("arena/b")).unwrap();
    assert!(Arc::ptr_eq(&kb.comps, &resolved[0].comps), "the kept tree takes the new subtree");
}

#[test]
fn respawning_a_tombstoned_node_brings_it_back() {
    let mut app = mounted();
    let a = entity(&app, "arena/a").unwrap();
    app.world_mut().despawn(a);
    bevy_ts_tsx::reconcile_external(app.world_mut(), ROOT, None, &level(0.0)).unwrap();
    let a_id = bevy_ts_tsx::desc::node_id(ROOT, &p("arena/a"));
    assert!(app.world().resource::<TsxTree>().tombstones.contains_key(&a_id));
    assert_eq!(restore::tombstoned_paths(app.world(), ROOT), vec![p("arena/a")]);

    let k = kept(&app);
    apply_at(app.world_mut(), ROOT, &[(p("arena/a"), find_node(&k, &p("arena/a")))], AtMode::Respawn);
    assert!(entity(&app, "arena/a").is_some(), "a exists again");
    assert!(!app.world().resource::<TsxTree>().tombstones.contains_key(&a_id), "its tombstone is gone");
}

#[test]
fn the_scan_finds_what_the_game_changed() {
    let mut app = mounted();
    restore::take_baselines(app.world_mut());
    assert!(restore::has_baseline(app.world(), ROOT));
    assert!(!restore::has_callbacks(app.world(), ROOT));
    app.update();
    app.update();
    assert_eq!(restore::changed_paths(app.world(), ROOT), Some(vec![]), "nothing changed");

    // A TS write (a native-layout field), a Rust write, an added and a removed component.
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    let a = entity(&app, "arena/a").unwrap();
    let mut m = app.world_mut().get_mut_by_id(a, id).unwrap();
    // SAFETY: Health is two f32s.
    unsafe { std::ptr::copy_nonoverlapping(1f32.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
    let b = entity(&app, "arena/b").unwrap();
    app.world_mut().get_mut::<Transform>(b).unwrap().translation.y = 1.0;
    let c = entity(&app, "arena/c").unwrap();
    app.world_mut().entity_mut(c).insert(Visibility::Hidden);
    let d = entity(&app, "arena/d").unwrap();
    app.world_mut().entity_mut(d).remove_by_id(id);
    let mut got = restore::changed_paths(app.world(), ROOT).unwrap();
    got.sort();
    assert_eq!(got, [p("arena/a"), p("arena/b"), p("arena/c"), p("arena/d")]);

    // Applied again since the start: no baseline.
    bevy_ts_tsx::reconcile_external(app.world_mut(), ROOT, None, &level(0.0)).unwrap();
    assert_eq!(restore::changed_paths(app.world(), ROOT), None);
}

#[test]
fn a_despawned_child_marks_its_parent() {
    let mut app = mounted();
    restore::take_baselines(app.world_mut());
    let c = entity(&app, "arena/c").unwrap();
    // A raw despawn, as a game's: the parent's `Children` is written, so it counts as changed too.
    app.world_mut().despawn(c);
    let got = restore::changed_paths(app.world(), ROOT).unwrap();
    assert_eq!(got, [p("arena")], "the parent's Children changed: the whole arena respawns");
    assert_eq!(restore::highest(vec![p("arena/c"), p("arena"), p("arena/c/x")]), [p("arena")]);
}

/// A derived component written after the start, as Bevy writes it each frame, is no change.
fn derived_is_no_change<C: Component<Mutability = bevy::ecs::component::Mutable>>(value: C) {
    let mut app = mounted();
    let b = entity(&app, "arena/b").unwrap();
    app.world_mut().entity_mut(b).insert(value);
    restore::take_baselines(app.world_mut());
    app.world_mut().get_mut::<C>(b).unwrap().set_changed();
    assert_eq!(restore::changed_paths(app.world(), ROOT), Some(vec![]), "{}", std::any::type_name::<C>());
}

#[test]
fn what_bevy_derives_each_frame_is_not_a_change() {
    use bevy::camera::visibility::VisibleEntities;
    use bevy::core_pipeline::prepass::PreviousViewData;
    use bevy::light::cluster::{ClusterableObjects, Clusters};
    derived_is_no_change(ViewVisibility::HIDDEN);
    derived_is_no_change(VisibleEntities::default());
    derived_is_no_change(Clusters {
        tile_size: UVec2::ONE,
        dimensions: UVec3::ONE,
        near: 0.1,
        far: 10.0,
        last_frame_farthest_z: None,
        last_frame_total_cluster_index_count: None,
        clusterable_objects: ClusterableObjects::Gpu,
    });
    let m = Mat4::IDENTITY;
    derived_is_no_change(PreviousViewData { view_from_world: m, clip_from_world: m, unjittered_clip_from_world: m, clip_from_view: m, world_from_clip: m, view_from_clip: m });
    // What the game set on a camera still counts.
    let mut app = mounted();
    let b = entity(&app, "arena/b").unwrap();
    app.world_mut().entity_mut(b).insert(Visibility::Visible);
    restore::take_baselines(app.world_mut());
    *app.world_mut().get_mut::<Visibility>(b).unwrap() = Visibility::Hidden;
    assert_eq!(restore::changed_paths(app.world(), ROOT), Some(vec![p("arena/b")]));
}
