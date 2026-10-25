//! World saves (openspec world-saves): save, despawn, load and compare; entity remapping; filters;
//! migration and kept classes; the write and the file. On the fake backend: TS components and
//! resources are declared by a generation, written and read as bytes.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::world_serialization::{DynamicWorld, WorldSerializationPlugin};
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::commands::{TsSpawned, insert_bytes};
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::log::ScriptLog;
use bevy_ts::manifest::{ComponentDecl, FieldDecl, Manifest, ResourceDecl};
use bevy_ts::plugin_api;
use bevy_ts::saves::{Save, SaveDir, SaveOptions, TsSaved, WorldLoaded, WorldSaved};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A Rust resource the saves name.
#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Resource, Default)]
struct Level(u32);

/// A Rust component holding an asset handle.
#[derive(Component, Reflect, Default, Clone, Debug)]
#[reflect(Component, Default)]
struct Icon(Handle<Image>);

#[derive(Resource, Default)]
struct Got {
    saved: Vec<WorldSaved>,
    loaded: Vec<WorldLoaded>,
}

fn collect(mut s: MessageReader<WorldSaved>, mut l: MessageReader<WorldLoaded>, mut got: ResMut<Got>) {
    got.saved.extend(s.read().cloned());
    got.loaded.extend(l.read().cloned());
}

fn field(name: &str, ty: FieldType, default: Option<Value>) -> FieldDecl {
    FieldDecl { name: name.into(), ty, default, hints: Default::default() }
}

fn component(name: &str, fields: Vec<FieldDecl>) -> ComponentDecl {
    ComponentDecl { name: name.into(), fields, ..Default::default() }
}

/// `game::Health { hp: f32 }`, `game::Follow { target: Entity }`, `game::Shield { power: f32 }`
/// and the resource `game::Score { value: f32 }`.
fn declarations() -> Manifest {
    Manifest {
        components: vec![
            component("game::Health", vec![field("hp", FieldType::F32, None)]),
            component("game::Follow", vec![field("target", FieldType::Entity, None)]),
            component("game::Shield", vec![field("power", FieldType::F32, None)]),
        ],
        resources: vec![ResourceDecl {
            name: "game::Score".into(),
            fields: vec![field("value", FieldType::F32, None)],
            insert: Some(json!({ "value": 0 }).as_object().unwrap().clone()),
            location: None,
        }],
        ..Default::default()
    }
}

fn folder(name: &str) -> PathBuf {
    // Per process: another worktree's run of the same test must not share its files.
    let dir = std::env::temp_dir().join("bevy-ts-tests").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Game {
    app: App,
    fake: FakeHandle,
    dir: PathBuf,
}

impl Game {
    fn new(name: &str, manifest: Manifest) -> Game {
        let fake = FakeHandle::default();
        let h = fake.clone();
        let dir = folder(name);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin { file_path: dir.join("assets").to_string_lossy().into_owned(), ..Default::default() })
            .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
        app.init_asset::<Image>().register_type::<Level>().register_type::<Icon>();
        // What DefaultPlugins registers and MinimalPlugins leaves out.
        app.register_type::<Name>().register_type::<ChildOf>().register_type::<Children>();
        app.insert_resource(SaveDir(dir.join("saves"))).init_resource::<Got>().add_systems(Last, collect);
        let mut game = Game { app, fake, dir };
        game.declare(manifest);
        game
    }

    fn declare(&mut self, manifest: Manifest) {
        self.fake.push(FakeProgram { manifest, ..Default::default() });
        let set = ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] };
        let r = generation::apply(self.app.world_mut(), set, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    }

    fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    fn spawn(&mut self, name: &str, ts: &[(&str, Value)], extra: impl Bundle) -> Entity {
        let e = self.app.world_mut().spawn((Name::new(name.to_string()), extra));
        let id = e.id();
        for (c, v) in ts {
            self.insert_ts(id, c, v.clone());
        }
        id
    }

    fn insert_ts(&mut self, e: Entity, c: &str, v: Value) {
        let world = self.app.world_mut();
        let (id, _) = plugin_api::component_by_name(world, c).expect("live component");
        let bytes = plugin_api::encode_component(world, c, v.as_object().unwrap()).unwrap();
        // SAFETY: encoded with the component's own layout.
        unsafe { insert_bytes(&mut world.entity_mut(e), id, &bytes) };
    }

    fn read_ts(&self, e: Entity, c: &str) -> Option<Value> {
        let world = self.app.world();
        let (id, layout) = plugin_api::component_by_name(world, c)?;
        let ptr = world.get_by_id(e, id)?;
        // SAFETY: a TS component's column holds `layout.size` bytes per entity.
        Some(bevy_ts::value::decode_component(&layout, unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) }))
    }

    fn score(&self) -> Option<Value> {
        let world = self.app.world();
        let (id, layout) = plugin_api::component_by_name(world, "game::Score")?;
        let e = world.resource_entities().get(id)?;
        let ptr = world.get_by_id(e, id)?;
        // SAFETY: as above.
        Some(bevy_ts::value::decode_component(&layout, unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) }))
    }

    fn set_score(&mut self, v: f32) {
        let world = self.app.world_mut();
        let (id, _) = plugin_api::component_by_name(world, "game::Score").unwrap();
        let e = world.resource_entities().get(id).unwrap();
        let bytes = plugin_api::encode_component(world, "game::Score", json!({ "value": v }).as_object().unwrap()).unwrap();
        // SAFETY: as above.
        unsafe { insert_bytes(&mut world.entity_mut(e), id, &bytes) };
    }

    fn by_name(&mut self, name: &str) -> Vec<Entity> {
        let world = self.app.world_mut();
        world.query::<(Entity, &Name)>().iter(world).filter(|(_, n)| n.as_str() == name).map(|(e, _)| e).collect()
    }

    fn one(&mut self, name: &str) -> Entity {
        let found = self.by_name(name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    }

    fn saved_count(&mut self) -> usize {
        let world = self.app.world_mut();
        world.query_filtered::<Entity, With<Save>>().iter(world).count()
    }

    fn file(&self, slot: &str) -> PathBuf {
        self.dir.join("saves").join(format!("{slot}.scn.ron"))
    }

    /// Saves and waits for its `WorldSaved`.
    fn save(&mut self, slot: &str, options: Value) -> WorldSaved {
        let options: SaveOptions = serde_json::from_value(options).unwrap();
        bevy_ts::saves::save(self.world(), slot, options);
        self.until(|g| g.app.world().resource::<Got>().saved.iter().any(|s| s.name == slot));
        let mut got = self.world().resource_mut::<Got>();
        let i = got.saved.iter().position(|s| s.name == slot).unwrap();
        got.saved.remove(i)
    }

    fn save_ok(&mut self, slot: &str, options: Value) -> String {
        let s = self.save(slot, options);
        assert_eq!(s.error, None, "save {slot}");
        std::fs::read_to_string(self.file(slot)).unwrap()
    }

    /// Loads and waits for its `WorldLoaded`.
    fn load(&mut self, slot: &str, parent: Option<Entity>) -> WorldLoaded {
        bevy_ts::saves::load(self.world(), slot, parent);
        self.until(|g| g.app.world().resource::<Got>().loaded.iter().any(|s| s.name == slot));
        let mut got = self.world().resource_mut::<Got>();
        let i = got.loaded.iter().position(|s| s.name == slot).unwrap();
        got.loaded.remove(i)
    }

    fn load_ok(&mut self, slot: &str) -> Vec<Entity> {
        let l = self.load(slot, None);
        assert_eq!(l.error, None, "load {slot}");
        l.entities
    }

    fn until(&mut self, done: impl Fn(&Game) -> bool) {
        // A deadline that holds on a loaded machine; the IO itself takes milliseconds.
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out");
            self.app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn despawn_saved(&mut self) {
        let world = self.app.world_mut();
        let saved: Vec<Entity> = world.query_filtered::<Entity, With<Save>>().iter(world).collect();
        for e in saved {
            let _ = world.try_despawn(e);
        }
    }

    fn log(&self) -> Vec<String> {
        self.app.world().resource::<ScriptLog>().since(0).map(|e| e.message.clone()).collect()
    }
}

#[test]
fn save_despawn_all_load_compare() {
    let mut g = Game::new("saves-round-trip", declarations());
    g.world().insert_resource(Level(4));
    g.set_score(12.0);
    let a = g.spawn("a", &[("game::Health", json!({ "hp": 3 }))], (Save, Transform::from_xyz(1.0, 2.0, 3.0)));
    let b = g.spawn("b", &[("game::Health", json!({ "hp": 5 }))], (Save, Transform::from_xyz(-1.0, 0.0, 0.0)));
    let outside = g.world().spawn(Name::new("outside")).id();
    g.insert_ts(a, "game::Follow", json!({ "target": b.to_bits() }));
    g.insert_ts(b, "game::Follow", json!({ "target": outside.to_bits() }));
    let kid = g.spawn("kid", &[], (Save, ChildOf(a)));
    let unmarked = g.spawn("unmarked", &[("game::Health", json!({ "hp": 9 }))], ());
    g.app.update();

    let text = g.save_ok("slot1", json!({ "resources": ["it::saves::Level", "game::Score"] }));
    assert!(text.contains("bevy_ts::saves::TsSaved") && text.contains("game::Health"), "{text}");
    assert!(!text.contains("unmarked"), "only Save-marked entities: {text}");

    g.despawn_saved();
    assert_eq!(g.saved_count(), 0);
    g.world().insert_resource(Level(0));
    g.set_score(0.0);
    let _ = (kid, unmarked);

    let loaded = g.load_ok("slot1");
    assert_eq!(loaded.len(), 3);
    assert_eq!(g.saved_count(), 3);
    let (a, b, kid) = (g.one("a"), g.one("b"), g.one("kid"));
    for e in [a, b, kid] {
        assert!(loaded.contains(&e));
        assert!(g.app.world().get::<TsSpawned>(e).is_some(), "a loaded entity is TS-spawned");
        assert!(g.app.world().get::<TsSaved>(e).is_none(), "TsSaved leaves once written");
    }
    assert_eq!(*g.app.world().get::<Transform>(a).unwrap(), Transform::from_xyz(1.0, 2.0, 3.0));
    assert_eq!(*g.app.world().get::<Transform>(b).unwrap(), Transform::from_xyz(-1.0, 0.0, 0.0));
    assert_eq!(g.read_ts(a, "game::Health"), Some(json!({ "hp": 3.0 })));
    assert_eq!(g.read_ts(b, "game::Health"), Some(json!({ "hp": 5.0 })));
    // A TS Entity field names the loaded entity; one naming an entity outside the save is mapped
    // as Bevy maps a Rust one: to an entity that is not alive.
    assert_eq!(g.read_ts(a, "game::Follow"), Some(json!({ "target": b.to_bits() })));
    let target = Entity::from_bits(g.read_ts(b, "game::Follow").unwrap()["target"].as_u64().unwrap());
    assert_ne!(target, outside);
    assert!(g.app.world().get_entity(target).is_err());
    assert_eq!(g.app.world().get::<ChildOf>(kid).unwrap().parent(), a);
    assert_eq!(*g.app.world().resource::<Level>(), Level(4));
    assert_eq!(g.score(), Some(json!({ "value": 12.0 })));

    // Load adds: a second load doubles the set.
    g.load_ok("slot1");
    assert_eq!(g.saved_count(), 6);
    assert_eq!(g.by_name("a").len(), 2);
}

#[test]
fn the_saved_set_and_the_filters() {
    let mut g = Game::new("saves-filters", declarations());
    let a = g.spawn("a", &[("game::Health", json!({ "hp": 3 }))], (Save, Transform::default(), GlobalTransform::default()));
    g.spawn("b", &[("game::Health", json!({ "hp": 4 }))], (Save, Transform::default()));
    let c = g.spawn("c", &[], Transform::default());
    let d = g.spawn("d", &[], ());

    let all = g.save_ok("all", json!({}));
    assert!(all.contains("\"a\"") && all.contains("\"b\"") && !all.contains("\"c\""), "{all}");
    assert!(all.contains("bevy_transform::components::transform::Transform"), "{all}");
    assert!(!all.contains("GlobalTransform"), "computed, always denied: {all}");
    assert!(!all.contains("TsSpawned"), "{all}");
    assert!(!all.contains("TsSavedResources") && !all.contains("game::Score"), "no resource unless named: {all}");

    let picked = g.save_ok("picked", json!({ "entities": [c.to_bits(), d.to_bits()] }));
    assert!(picked.contains("\"c\"") && picked.contains("\"d\"") && !picked.contains("\"a\""), "{picked}");

    let denied = g.save_ok("denied", json!({ "deny": ["Transform", "game::Health"] }));
    assert!(!denied.contains("Transform") && !denied.contains("game::Health"), "{denied}");
    assert!(denied.contains("bevy_ts::saves::Save"), "{denied}");

    let allowed = g.save_ok("allowed", json!({ "allow": ["game::Health"] }));
    assert!(allowed.contains("game::Health") && allowed.contains("bevy_ts::saves::Save"), "{allowed}");
    assert!(!allowed.contains("Transform") && !allowed.contains("Name"), "only Health and Save: {allowed}");

    let rust_only = g.save_ok("rust-only", json!({ "allow": ["bevy_ecs::name::Name"], "deny": ["Name"] }));
    assert!(!rust_only.contains("Name") && !rust_only.contains("TsSaved"), "deny after allow: {rust_only}");

    let unknown = g.save("nope", json!({ "deny": ["game::Nothing"] }));
    assert!(unknown.error.unwrap().contains("game::Nothing"));
    assert!(!g.file("nope").exists());
    let _ = a;
}

#[test]
fn a_name_with_a_path_writes_nothing() {
    let mut g = Game::new("saves-names", declarations());
    g.spawn("a", &[], Save);
    for bad in ["../x", "/abs", "a/b", ""] {
        let s = g.save(bad, json!({}));
        assert!(s.error.unwrap().contains("A-Z, a-z, 0-9, _ and -"), "{bad}");
    }
    assert!(!g.dir.join("saves").exists() || std::fs::read_dir(g.dir.join("saves")).unwrap().next().is_none());
    assert!(!g.dir.join("x.scn.ron").exists());
}

#[test]
fn old_saves_migrate_and_unknown_classes_stay() {
    let mut g = Game::new("saves-migrate", declarations());
    g.spawn("a", &[("game::Health", json!({ "hp": 3 })), ("game::Shield", json!({ "power": 2 }))], Save);
    g.save_ok("old", json!({}));
    g.despawn_saved();

    // Health gains `max` and its `hp` becomes an i32; Shield is gone.
    let mut m = declarations();
    m.components = vec![
        component("game::Health", vec![field("hp", FieldType::I32, Some(json!(1))), field("max", FieldType::F32, Some(json!(10)))]),
        component("game::Follow", vec![field("target", FieldType::Entity, None)]),
    ];
    g.declare(m);
    g.load_ok("old");
    let a = g.one("a");
    assert_eq!(g.read_ts(a, "game::Health"), Some(json!({ "hp": 1, "max": 10.0 })));
    let log = g.log();
    assert!(log.iter().any(|l| l.contains("game::Health migrated on 1 entities; added max; reset hp")), "{log:?}");
    assert!(log.iter().any(|l| l.contains("game::Shield")), "{log:?}");
    let kept = g.app.world().get::<TsSaved>(a).cloned().unwrap();
    assert_eq!(kept.0.keys().collect::<Vec<_>>(), ["game::Shield"]);

    // Saved again, the kept class goes with it, beside the live ones.
    let again = g.save_ok("again", json!({}));
    assert!(again.contains("game::Shield") && again.contains("game::Health"), "{again}");
    // And comes back when its class does.
    g.despawn_saved();
    g.declare(declarations());
    g.load_ok("again");
    let a = g.one("a");
    assert_eq!(g.read_ts(a, "game::Shield"), Some(json!({ "power": 2.0 })));
    assert!(g.app.world().get::<TsSaved>(a).is_none());
}

#[test]
fn a_missing_slot_and_a_failed_write() {
    let mut g = Game::new("saves-failures", declarations());
    let before = g.saved_count();
    let l = g.load("nope", None);
    assert!(l.entities.is_empty());
    assert!(l.error.as_deref().unwrap().contains("nope.scn.ron"), "{l:?}");
    assert_eq!(g.saved_count(), before);

    g.spawn("a", &[("game::Health", json!({ "hp": 3 }))], Save);
    let first = g.save_ok("slot1", json!({}));
    g.spawn("b", &[], Save);
    // The temporary file's path is a folder: the write fails before the rename.
    std::fs::create_dir_all(g.dir.join("saves/slot1.scn.ron.tmp")).unwrap();
    let s = g.save("slot1", json!({}));
    assert!(s.error.as_deref().unwrap().contains("slot1.scn.ron.tmp"), "{s:?}");
    assert_eq!(std::fs::read_to_string(g.file("slot1")).unwrap(), first, "the old save is whole");
}

#[test]
fn bevy_reads_a_rust_only_save_and_handles_are_paths() {
    let mut g = Game::new("saves-bevy-reads", declarations());
    let icon: Handle<Image> = g.app.world().resource::<AssetServer>().load("icon.png");
    g.spawn("a", &[], (Save, Transform::from_xyz(1.0, 0.0, 0.0), Icon(icon)));
    g.spawn("b", &[], (Save, Transform::from_xyz(2.0, 0.0, 0.0)));
    let text = g.save_ok("rust", json!({}));
    assert!(text.contains("\"icon.png\""), "a handle is written as its path: {text}");

    // bevy-ts loads it back with the handle.
    g.despawn_saved();
    g.load_ok("rust");
    let a = g.one("a");
    let path = g.app.world().get::<Icon>(a).unwrap().0.path().map(|p| p.to_string());
    assert_eq!(path.as_deref(), Some("icon.png"));

    // A plain Bevy app reads the same file with its own loader.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin { file_path: g.dir.join("saves").to_string_lossy().into_owned(), ..Default::default() })
        .add_plugins(WorldSerializationPlugin);
    app.init_asset::<Image>().register_type::<Save>().register_type::<Icon>().register_type::<Name>().register_type::<Transform>();
    let handle: Handle<DynamicWorld> = app.world().resource::<AssetServer>().load("rust.scn.ron");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if let Some(bevy::asset::LoadState::Failed(e)) = server.get_load_state(&handle) {
            panic!("Bevy could not read the save: {e}");
        }
        if app.world().resource::<Assets<DynamicWorld>>().contains(&handle) {
            break;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    let world = app.world_mut();
    let dynamic = world.resource_mut::<Assets<DynamicWorld>>().remove(&handle).unwrap();
    let mut map = Default::default();
    dynamic.write_to_world(world, &mut map).unwrap();
    assert_eq!(world.query_filtered::<&Name, With<Save>>().iter(world).count(), 2);
}

/// `parent` makes each loaded root a child of the entity.
#[test]
fn a_load_under_a_parent() {
    let mut g = Game::new("saves-parent", declarations());
    let a = g.spawn("a", &[], Save);
    g.spawn("kid", &[], (Save, ChildOf(a)));
    g.save_ok("tree", json!({}));
    g.despawn_saved();
    let root = g.world().spawn(Name::new("root")).id();
    let l = g.load("tree", Some(root));
    assert_eq!(l.error, None);
    let (a, kid) = (g.one("a"), g.one("kid"));
    assert_eq!(g.app.world().get::<ChildOf>(a).unwrap().parent(), root);
    assert_eq!(g.app.world().get::<ChildOf>(kid).unwrap().parent(), a);
    assert!(g.app.world().get::<Children>(root).unwrap().contains(&a));
}
