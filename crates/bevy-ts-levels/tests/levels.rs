//! Levels in an app against the fake backend: `app.level` arrives in the `levels` section, the
//! file loads through the asset server, and its entities reconcile through tsx.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::log::ScriptLog;
use bevy_ts::manifest::{ComponentDecl, FieldDecl, Manifest, Section};
use bevy_ts::plugin_api;
use bevy_ts_levels::engine::{LevelInstance, Levels, LevelsPlugin, reconcile_level};
use bevy_ts_tsx::{Provenance, TsxNode, TsxPlugin, TsxTree};
use serde_json::json;
use std::path::{Path, PathBuf};

struct Fixture {
    app: App,
    fake: FakeHandle,
    dir: PathBuf,
}

fn fixture(name: &str, files: &[(&str, &str)]) -> Fixture {
    let dir = std::env::temp_dir().join(format!("bevy-ts-levels-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (p, src) in files {
        write(&dir, p, src);
    }
    let fake = FakeHandle::default();
    let h = fake.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..Default::default() }))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin)
        .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = dir.clone();
    Fixture { app, fake, dir }
}

fn write(dir: &Path, p: &str, src: &str) {
    let path = dir.join(p);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, src).unwrap();
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

impl Fixture {
    fn load(&mut self, levels: &[&str]) {
        self.fake.push(FakeProgram {
            manifest: Manifest { components: vec![health()], ..Default::default() },
            sections: vec![Section { name: "levels".into(), bytes: serde_json::to_vec(&json!({ "levels": levels })).unwrap(), location: None }],
            ..Default::default()
        });
        let r = generation::apply(self.app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    }

    /// Frames until every open level has loaded and reconciled.
    fn settle(&mut self) {
        for _ in 0..200 {
            self.app.update();
            let levels = self.app.world().resource::<Levels>();
            if levels.roots.values().all(|r| !r.dirty) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("levels never loaded");
    }

    fn by_name(&mut self, name: &str) -> Entity {
        let world = self.app.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no {name}"))
    }

    fn translation(&self, e: Entity) -> Vec3 {
        self.app.world().get::<Transform>(e).unwrap().translation
    }

    fn hp(&self, e: Entity) -> (f32, f32) {
        let (id, _) = plugin_api::component_by_name(self.app.world(), "game::Health").unwrap();
        let ptr = self.app.world().get_by_id(e, id).unwrap();
        // SAFETY: Health is two f32s.
        let b = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 8) };
        (f32::from_le_bytes(b[0..4].try_into().unwrap()), f32::from_le_bytes(b[4..8].try_into().unwrap()))
    }

    fn errors(&self) -> Vec<String> {
        self.app.world().resource::<ScriptLog>().since(0).filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| format!("{} {}", e.location.as_ref().map(|l| l.to_string()).unwrap_or_default(), e.message)).collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const ARENA: &str = "// The arena.
#EnemyA
Transform { translation: Vec3 { x: -3.0, z: -4.0 } }
game::Health { hp: 100.0, max: 100.0 }
--
#EnemyB
Transform { translation: Vec3 { x: 3.0, z: -4.0 } }
Health { max: 250.0 }
--
#RoomA
:\"levels/room.bsn\"
--
#RoomB
:\"./room.bsn\"
Transform { translation: Vec3 { x: 20.0 } }
";

const ROOM: &str = "#Room
Transform
Children [
    #Floor
    Transform { scale: Vec3 { x: 10.0, y: 1.0, z: 10.0 } }
]
";

#[test]
fn a_level_loads_as_a_reconciled_root() {
    let mut f = fixture("load", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
    f.load(&["levels/arena.bsn"]);
    f.settle();
    assert!(f.errors().is_empty(), "{:?}", f.errors());
    let a = f.by_name("EnemyA");
    let b = f.by_name("EnemyB");
    assert_eq!(f.translation(a), Vec3::new(-3.0, 0.0, -4.0));
    assert_eq!(f.hp(b), (100.0, 250.0));
    let room_b = f.by_name("RoomB");
    assert_eq!(f.translation(room_b), Vec3::new(20.0, 0.0, 0.0));
    let floors: Vec<Entity> = {
        let world = f.app.world_mut();
        world.query::<(Entity, &Name)>().iter(world).filter(|(_, n)| n.as_str() == "Floor").map(|(e, _)| e).collect()
    };
    assert_eq!(floors.len(), 2, "room.bsn is included twice");
    let floor = f.app.world().get::<Transform>(floors[0]).unwrap();
    assert_eq!(floor.scale, Vec3::new(10.0, 1.0, 10.0));
    assert!(f.app.world().get::<ChildOf>(floors[0]).is_some());

    let node = f.app.world().get::<TsxNode>(b).unwrap();
    assert_eq!(node.root, "level:levels/arena.bsn");
    assert_eq!(node.key_path, ["EnemyB"]);
    assert_eq!(node.components["game::Health"].fields["max"].provenance, Provenance::Override);
    assert_eq!(node.components["game::Health"].overrides["max"].to_string(), "levels/arena.bsn:8:10");
    let inst = f.app.world().get::<LevelInstance>(floors[0]).unwrap();
    let files: Vec<&str> = inst.owners.iter().map(|o| o.file.as_str()).collect();
    assert_eq!(files, ["levels/arena.bsn", "levels/room.bsn"]);
}

#[test]
fn a_generation_leaves_level_entities_alone() {
    let mut f = fixture("generation", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
    f.load(&["levels/arena.bsn"]);
    f.settle();
    let b = f.by_name("EnemyB");
    // A system moves enemy b; an unchanged override is not written again (rule 3).
    f.app.world_mut().get_mut::<Transform>(b).unwrap().translation.x = 7.0;
    f.load(&["levels/arena.bsn"]);
    f.settle();
    assert_eq!(f.by_name("EnemyB"), b, "same entity");
    assert_eq!(f.translation(b).x, 7.0);
    assert_eq!(f.app.world().resource::<TsxTree>().last.written, 0);
}

#[test]
fn a_bad_level_changes_nothing() {
    let mut f = fixture("bad", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
    f.load(&["levels/arena.bsn"]);
    f.settle();
    let b = f.by_name("EnemyB");
    write(&f.dir, "levels/arena.bsn", &ARENA.replace("Health { max: 250.0 }", "Health { max: 250.0 }\nTransfrom { translation: Vec3 { x: 9.0 } }"));
    // Reload the asset by hand: the file watcher is a feature of its own.
    let handle = f.app.world().resource::<Levels>().roots["levels/arena.bsn"].handle.clone();
    f.app.world().resource::<AssetServer>().reload(handle.path().unwrap().clone());
    for _ in 0..100 {
        f.app.update();
        if !f.errors().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    reconcile_level(f.app.world_mut(), "levels/arena.bsn");
    let errors = f.errors();
    assert!(errors.iter().any(|e| e.starts_with("levels/arena.bsn:9:1 levels: unknown component Transfrom")), "{errors:?}");
    assert_eq!(f.translation(b).x, 3.0);
}

#[test]
fn a_generation_without_the_level_closes_it() {
    let mut f = fixture("close", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
    f.load(&["levels/arena.bsn"]);
    f.settle();
    let b = f.by_name("EnemyB");
    f.load(&[]);
    f.app.update();
    assert!(f.app.world().get_entity(b).is_err());
    assert!(f.app.world().resource::<Levels>().roots.is_empty());
}

#[derive(Asset, Reflect, Debug)]
struct Note(String);

#[derive(Default, TypePath)]
struct NoteLoader;

impl bevy::asset::AssetLoader for NoteLoader {
    type Asset = Note;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn bevy::asset::io::Reader, _: &(), _: &mut bevy::asset::LoadContext<'_>) -> Result<Note, std::io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(Note(String::from_utf8_lossy(&bytes).into_owned()))
    }

    fn extensions(&self) -> &[&str] {
        &["note"]
    }
}

/// A reflect-tier component: a handle field and a plain one.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Sign {
    note: Handle<Note>,
    size: f32,
}

fn with_signs(f: &mut Fixture) {
    f.app.init_asset::<Note>().register_asset_reflect::<Note>().register_asset_loader(NoteLoader).register_type::<Sign>();
}

#[test]
fn handle_fields_take_asset_paths() {
    let level = "#Sign\nSign { note: \"./hello.note\", size: 2.0 }\n--\n#Gone\nSign { note: \"notes/gone.note\" }\n";
    let mut f = fixture("assets", &[("levels/signs.bsn", level), ("levels/hello.note", "hello")]);
    with_signs(&mut f);
    f.load(&["levels/signs.bsn"]);
    f.settle();
    assert!(f.errors().is_empty(), "{:?}", f.errors());
    let sign = f.by_name("Sign");
    let s = f.app.world().get::<Sign>(sign).unwrap();
    assert_eq!(s.size, 2.0);
    assert_eq!(s.note.path().map(|p| p.to_string()).as_deref(), Some("levels/hello.note"));
    let id = s.note.id();
    let mut warned = Vec::new();
    for _ in 0..200 {
        f.app.update();
        warned = f.app.world().resource::<ScriptLog>().since(0).filter(|e| e.level == bevy_ts::log::LogLevel::Warn).map(|e| format!("{} {}", e.location.as_ref().map(|l| l.to_string()).unwrap_or_default(), e.message)).collect();
        if f.app.world().resource::<Assets<Note>>().get(id).is_some() && !warned.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(f.app.world().resource::<Assets<Note>>().get(id).map(|n| n.0.as_str()), Some("hello"));
    assert!(warned.iter().any(|w| w.starts_with("levels/signs.bsn:5:8 levels: missing asset notes/gone.note")), "{warned:?}");
    // The level still loaded the entity, with the handle unresolved.
    let gone = f.by_name("Gone");
    assert!(f.app.world().get::<Sign>(gone).is_some());
}

mod writes {
    use super::*;
    use bevy_ts_levels::edit::{WriteRequest, write_level};
    use bevy_ts_levels::engine::content_hash;
    use bevy_ts_levels::writer::{Base, NewPatch, Op, WValue};

    fn p(s: &str) -> Vec<String> {
        s.split('/').map(String::from).collect()
    }

    fn set_x(path: &str, x: f64) -> Op {
        Op::Set { path: p(path), component: "Transform".into(), field: p("translation/x"), value: WValue::Float { v: x, f32: true }, containers: vec!["Vec3".into()] }
    }

    fn file(f: &Fixture) -> String {
        std::fs::read_to_string(f.dir.join("levels/arena.bsn")).unwrap()
    }

    fn write(f: &mut Fixture, base: &str, ops: Vec<Op>) -> bevy_ts_levels::edit::WriteResult {
        write_level(f.app.world_mut(), WriteRequest { id: 7, file: "levels/arena.bsn".into(), base: content_hash(base.as_bytes()), ops })
    }

    fn arena() -> Fixture {
        let mut f = fixture(&format!("write-{:?}", std::thread::current().id()).replace(['(', ')'], ""), &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        f
    }

    #[test]
    fn a_write_changes_one_line_and_keeps_the_entity() {
        let mut f = arena();
        let b = f.by_name("EnemyB");
        let r = write(&mut f, ARENA, vec![set_x("EnemyB", 5.0)]);
        assert_eq!((r.status, r.lines.clone()), ("written", vec![(7, 7)]), "{r:?}");
        assert_eq!(file(&f), ARENA.replace("x: 3.0, z: -4.0", "x: 5.0, z: -4.0"));
        assert_eq!(f.by_name("EnemyB"), b);
        assert_eq!(f.translation(b).x, 5.0);
        // The watcher's reload of the writer's own write reconciles nothing again.
        f.app.update();
        assert_eq!(f.app.world().resource::<bevy_ts_levels::edit::WriteResults>().0.len(), 1);
    }

    #[test]
    fn a_rename_keeps_the_entity() {
        let mut f = arena();
        let a = f.by_name("EnemyA");
        let r = write(&mut f, ARENA, vec![Op::Rename { path: p("EnemyA"), to: "Grunt".into() }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.by_name("Grunt"), a);
        assert_eq!(f.app.world().get::<TsxNode>(a).unwrap().key_path, ["Grunt"]);
    }

    #[test]
    fn another_entity_changed_on_disk_rebases() {
        let mut f = arena();
        let a = f.by_name("EnemyA");
        std::fs::write(f.dir.join("levels/arena.bsn"), ARENA.replace("x: -3.0", "x: -6.0")).unwrap();
        let r = write(&mut f, ARENA, vec![set_x("EnemyB", 5.0)]);
        assert_eq!(r.status, "rebased", "{r:?}");
        assert_eq!(f.translation(a).x, -6.0);
        let b = f.by_name("EnemyB");
        assert_eq!(f.translation(b).x, 5.0);
        assert!(file(&f).contains("x: -6.0") && file(&f).contains("x: 5.0"));
    }

    #[test]
    fn a_target_deleted_on_disk_is_a_conflict() {
        let mut f = arena();
        let b = f.by_name("EnemyB");
        let without_b = ARENA.replace("#EnemyB\nTransform { translation: Vec3 { x: 3.0, z: -4.0 } }\nHealth { max: 250.0 }\n--\n", "");
        assert_ne!(without_b, ARENA);
        std::fs::write(f.dir.join("levels/arena.bsn"), &without_b).unwrap();
        let r = write(&mut f, ARENA, vec![set_x("EnemyB", 5.0)]);
        assert_eq!((r.status, r.reason.as_deref()), ("conflict", Some("target removed")), "{r:?}");
        assert!(f.app.world().get_entity(b).is_err(), "the world follows the disk");
        assert_eq!(file(&f), without_b);
    }

    #[test]
    fn the_same_field_changed_on_disk_is_a_conflict() {
        let mut f = arena();
        std::fs::write(f.dir.join("levels/arena.bsn"), ARENA.replace("x: 3.0", "x: 9.0")).unwrap();
        let r = write(&mut f, ARENA, vec![set_x("EnemyB", 5.0)]);
        assert_eq!((r.status, r.reason.as_deref(), r.disk.as_deref()), ("conflict", Some("changed on disk"), Some("9.0")), "{r:?}");
        let b = f.by_name("EnemyB");
        assert_eq!(f.translation(b).x, 9.0);
    }

    #[test]
    fn naming_an_unnamed_entity_keeps_it() {
        let src = "Transform { translation: Vec3 { x: 1.0 } }\n--\nTransform { translation: Vec3 { x: 2.0 } }\n";
        let mut f = fixture("unnamed", &[("levels/arena.bsn", src)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let find = |f: &mut Fixture, x: f32| {
            let world = f.app.world_mut();
            world.query::<(Entity, &Transform)>().iter(world).find(|(_, t)| t.translation.x == x).map(|(e, _)| e).unwrap()
        };
        let (first, second) = (find(&mut f, 1.0), find(&mut f, 2.0));
        let r = write(&mut f, src, vec![set_x("#0", 1.5)]);
        assert_eq!(r.status, "written", "{r:?}");
        assert!(file(&f).starts_with("#Transform1 Transform { translation: Vec3 { x: 1.5 } }"), "{}", file(&f));
        assert_eq!(find(&mut f, 1.5), first);
        assert_eq!(find(&mut f, 2.0), second, "the positional id after it moved, not the entity");
    }
    fn set_hp(f: &mut Fixture, e: Entity, hp: f32) {
        let (id, _) = plugin_api::component_by_name(f.app.world(), "game::Health").unwrap();
        let mut ptr = f.app.world_mut().get_mut_by_id(e, id).unwrap();
        // SAFETY: Health is two f32s, hp first.
        unsafe { ptr.as_mut().as_ptr().cast::<f32>().write_unaligned(hp) };
    }

    #[test]
    fn a_reparent_keeps_the_entity_and_its_state() {
        let mut f = arena();
        let (a, b) = (f.by_name("EnemyA"), f.by_name("EnemyB"));
        set_hp(&mut f, b, 42.0);
        let r = write(&mut f, ARENA, vec![Op::Reparent { path: p("EnemyB"), parent: p("EnemyA"), index: None }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.by_name("EnemyB"), b);
        assert_eq!(f.app.world().get::<ChildOf>(b).map(ChildOf::parent), Some(a));
        assert_eq!(f.app.world().get::<TsxNode>(b).unwrap().key_path, ["EnemyA", "EnemyB"]);
        assert_eq!(f.hp(b), (42.0, 250.0), "the state stays");
        let moved = file(&f);
        let back = write(&mut f, &moved, r.inverse.clone());
        assert_eq!(back.status, "written", "{back:?}");
        assert_eq!(file(&f), ARENA);
        assert_eq!(f.by_name("EnemyB"), b);
        assert_eq!(f.app.world().get::<TsxNode>(b).unwrap().key_path, ["EnemyB"]);
        assert_eq!(f.hp(b), (42.0, 250.0));
    }

    #[test]
    fn an_add_returns_the_entity_it_spawned() {
        let mut f = arena();
        let transform = NewPatch { component: "Transform".into(), fields: vec![("translation".into(), WValue::Vector { ty: "Vec3".into(), lanes: vec![("x".into(), 7.0)] })] };
        let r = write(&mut f, ARENA, vec![Op::Add { parent: Vec::new(), name: None, base: Base::None, patches: vec![transform], asset: None }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(r.created.len(), 1);
        let (path, e) = &r.created[0];
        assert_eq!(path, &p("Transform"));
        let e = e.expect("spawned");
        assert_eq!(f.by_name("Transform"), e);
        assert_eq!(f.translation(e).x, 7.0);
    }

    #[test]
    fn components_are_checked_against_the_world() {
        let mut f = arena();
        let room = f.by_name("RoomB");
        // The included room gives RoomB its Transform: the file's patch overrides it, and cannot remove it.
        let r = write(&mut f, ARENA, vec![Op::RemoveComponent { path: p("RoomB"), component: "Transform".into() }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("prefab-internal")), "{r:?}");
        // A new patch takes the shortest name that resolves.
        let add = Op::AddComponent { path: p("RoomB"), component: "game::Health".into(), fields: vec![("max".into(), WValue::Float { v: 9.0, f32: true })], text: None };
        let r = write(&mut f, ARENA, vec![add.clone()]);
        assert_eq!(r.status, "written", "{r:?}");
        assert!(file(&f).contains("Transform { translation: Vec3 { x: 20.0 } }\nHealth { max: 9.0 }\n"), "{}", file(&f));
        assert_eq!(f.hp(room).1, 9.0);
        let now = file(&f);
        let again = write(&mut f, &now, vec![Op::AddComponent { path: p("RoomA"), component: "Transform".into(), fields: vec![], text: None }]);
        assert_eq!((again.status, again.reason.as_deref()), ("refused", Some("exists")), "the room gives RoomA a Transform: {again:?}");
        let back = write(&mut f, &now, r.inverse.clone());
        assert_eq!(back.status, "written", "{back:?}");
        assert_eq!(file(&f), ARENA);
        let (id, _) = plugin_api::component_by_name(f.app.world(), "game::Health").unwrap();
        assert!(!f.app.world().entity(room).contains_id(id), "the component went with its patch");
    }
    #[test]
    fn a_reorder_orders_the_worlds_children() {
        let src = "#Room\nTransform\nChildren [\n    #A Transform\n    --\n    #B Transform\n    --\n    #C Transform\n]\n";
        let mut f = fixture("reorder", &[("levels/arena.bsn", src)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let (room, a, b, c) = (f.by_name("Room"), f.by_name("A"), f.by_name("B"), f.by_name("C"));
        let order = |f: &Fixture| f.app.world().get::<Children>(room).unwrap().to_vec();
        assert_eq!(order(&f), [a, b, c]);
        let r = write(&mut f, src, vec![Op::Reparent { path: p("Room/A"), parent: p("Room"), index: Some(1) }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(order(&f), [b, a, c], "the world follows the file's order");
        assert_eq!(f.by_name("A"), a);
        let now = file(&f);
        assert_eq!(write(&mut f, &now, r.inverse.clone()).status, "written");
        assert_eq!(order(&f), [a, b, c]);
        assert_eq!(file(&f), src);
    }

    #[test]
    fn a_parent_game_code_gave_survives_a_reconcile() {
        let mut f = arena();
        let (a, b) = (f.by_name("EnemyA"), f.by_name("EnemyB"));
        f.app.world_mut().entity_mut(a).insert(ChildOf(b));
        let r = write(&mut f, ARENA, vec![set_x("EnemyB", 5.0)]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.app.world().get::<ChildOf>(a).map(ChildOf::parent), Some(b), "only a node a write moved is re-parented");
    }

    #[test]
    fn a_batch_is_checked_through_its_own_changes() {
        let mut f = arena();
        let r = write(&mut f, ARENA, vec![Op::Rename { path: p("RoomA"), to: "Hall".into() }, Op::AddComponent { path: p("Hall"), component: "Transform".into(), fields: vec![], text: None }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("invalid")), "a batch does not reuse a path it changed: {r:?}");
        // What the batch added, it can remove.
        let add = Op::AddComponent { path: p("RoomB"), component: "game::Health".into(), fields: vec![], text: None };
        let r = write(&mut f, ARENA, vec![add, Op::RemoveComponent { path: p("RoomB"), component: "Health".into() }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(file(&f), ARENA);
    }

    #[test]
    fn an_included_level_is_checked_through_the_level_that_opens_it() {
        let arena = "#Hall\n:\"levels/hut.bsn\"\n";
        let hut = "#Hut\n:\"levels/post.bsn\"\n";
        let post = "#Post\ngame::Health { max: 3.0 }\n";
        let mut f = fixture("included", &[("levels/arena.bsn", arena), ("levels/hut.bsn", hut), ("levels/post.bsn", post), ("levels/loose.bsn", "#Loose Transform\n")]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let to = |f: &mut Fixture, file: &str, base: &str, ops: Vec<Op>| write_level(f.app.world_mut(), WriteRequest { id: 8, file: file.into(), base: content_hash(base.as_bytes()), ops });
        let r = to(&mut f, "levels/hut.bsn", hut, vec![Op::AddComponent { path: p("Hut"), component: "game::Health".into(), fields: vec![], text: None }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("exists")), "post.bsn gives it: {r:?}");
        let r = to(&mut f, "levels/hut.bsn", hut, vec![Op::RemoveComponent { path: p("Hut"), component: "game::Health".into() }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("prefab-internal")), "{r:?}");
        // A level no open level shows has no world to check against.
        let r = to(&mut f, "levels/loose.bsn", "#Loose Transform\n", vec![Op::AddComponent { path: p("Loose"), component: "game::Health".into(), fields: vec![], text: None }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("invalid")), "{r:?}");
    }
    #[test]
    fn an_empty_path_is_refused_not_a_panic() {
        let mut f = arena();
        for op in [Op::Rename { path: vec![], to: "X".into() }, Op::Delete { path: vec![] }, Op::Reparent { path: vec![], parent: p("EnemyA"), index: None }] {
            let r = write(&mut f, ARENA, vec![op]);
            assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("not-found")), "{r:?}");
        }
    }

    #[test]
    fn a_reorder_in_an_included_level_orders_the_world() {
        let arena = "#Hall\n:\"levels/hut.bsn\"\n";
        let hut = "#Hut\nTransform\nChildren [\n    #A Transform\n    --\n    #B Transform\n]\n";
        let mut f = fixture("included-order", &[("levels/arena.bsn", arena), ("levels/hut.bsn", hut)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let (hall, a, b) = (f.by_name("Hall"), f.by_name("A"), f.by_name("B"));
        let r = write_level(f.app.world_mut(), WriteRequest { id: 1, file: "levels/hut.bsn".into(), base: content_hash(hut.as_bytes()), ops: vec![Op::Reparent { path: p("Hut/A"), parent: p("Hut"), index: Some(1) }] });
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.app.world().get::<Children>(hall).unwrap().to_vec(), [b, a]);
        assert_eq!(f.by_name("A"), a, "the entity stays");
    }

    #[test]
    fn a_move_leaves_no_stale_mark() {
        let mut f = arena();
        assert_eq!(bevy_ts_tsx::rekey(f.app.world_mut(), "level:levels/arena.bsn", &p("Nobody"), &p("Somebody")), 0);
        assert!(f.app.world().resource::<TsxTree>().moved.is_empty(), "nothing moved, nothing marked");
        let r = write(&mut f, ARENA, vec![Op::Rename { path: p("EnemyA"), to: "Grunt".into() }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert!(f.app.world().resource::<TsxTree>().moved.is_empty(), "the reconcile used the mark");
    }

    #[test]
    fn an_including_levels_override_is_not_the_files() {
        let arena = "#Hall\n:\"levels/hut.bsn\"\ngame::Health { max: 9.0 }\n--\n#Hall2\n:\"levels/hut.bsn\"\n";
        let hut = "#Hut\nTransform\n";
        let mut f = fixture("included-twice", &[("levels/arena.bsn", arena), ("levels/hut.bsn", hut)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let add = Op::AddComponent { path: p("Hut"), component: "game::Health".into(), fields: vec![], text: None };
        let r = write_level(f.app.world_mut(), WriteRequest { id: 1, file: "levels/hut.bsn".into(), base: content_hash(hut.as_bytes()), ops: vec![add] });
        assert_eq!(r.status, "written", "arena.bsn's Health on Hall is not hut.bsn's: {r:?}");
        let now = std::fs::read_to_string(f.dir.join("levels/hut.bsn")).unwrap();
        let r = write_level(f.app.world_mut(), WriteRequest { id: 2, file: "levels/hut.bsn".into(), base: content_hash(now.as_bytes()), ops: r.inverse.clone() });
        assert_eq!(r.status, "written", "hut.bsn removes its own patch: {r:?}");
        assert_eq!(std::fs::read_to_string(f.dir.join("levels/hut.bsn")).unwrap(), hut);
    }
    #[test]
    fn the_writers_refusal_of_an_earlier_op_stands() {
        let mut f = arena();
        let r = write(&mut f, ARENA, vec![Op::Delete { path: p("Nobody") }, Op::AddComponent { path: p("RoomA"), component: "Transform".into(), fields: vec![], text: None }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("not-found")), "op 0 is the batch's first problem: {r:?}");
    }

    const MULTI: &str = "#Hut\nTransform\nChildren [\n    #A Transform\n]\n--\n#Shed Transform\n";

    fn multi() -> Fixture {
        let mut f = fixture(&format!("multi-{:?}", std::thread::current().id()).replace(['(', ')'], ""), &[("levels/arena.bsn", "#Hall\n:\"levels/hut.bsn\"\n"), ("levels/hut.bsn", MULTI)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        f
    }

    fn to_hut(f: &mut Fixture, base: &str, ops: Vec<Op>) -> bevy_ts_levels::edit::WriteResult {
        write_level(f.app.world_mut(), WriteRequest { id: 3, file: "levels/hut.bsn".into(), base: content_hash(base.as_bytes()), ops })
    }

    #[test]
    fn a_reparent_across_tops_of_an_included_file_keeps_the_entity() {
        let mut f = multi();
        let (a, shed) = (f.by_name("A"), f.by_name("Shed"));
        let r = to_hut(&mut f, MULTI, vec![Op::Reparent { path: p("Hut/A"), parent: p("Shed"), index: None }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.by_name("A"), a, "the entity stays");
        assert_eq!(f.app.world().get::<ChildOf>(a).map(ChildOf::parent), Some(shed));
    }

    #[test]
    fn created_resolves_where_the_entity_is() {
        // A second top of an included file, and a path a later op renamed.
        let mut f = multi();
        let add = |parent: &str, name: &str| Op::Add { parent: p(parent), name: Some(name.into()), base: Base::None, patches: vec![], asset: None };
        let r = to_hut(&mut f, MULTI, vec![add("Shed", "Lamp"), add("Hut", "Bulb"), Op::Rename { path: p("Hut"), to: "Hall2".into() }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(r.created.iter().map(|(p, _)| p.join("/")).collect::<Vec<_>>(), ["Shed/Lamp", "Hall2/Bulb"]);
        assert_eq!((r.created[0].1, r.created[1].1), (Some(f.by_name("Lamp")), Some(f.by_name("Bulb"))), "{r:?}");
        // A file open by itself and included elsewhere: its own root.
        let mut g = fixture("created-own", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        g.load(&["levels/arena.bsn", "levels/room.bsn"]);
        g.settle();
        let r = write_level(g.app.world_mut(), WriteRequest { id: 4, file: "levels/room.bsn".into(), base: content_hash(ROOM.as_bytes()), ops: vec![Op::Add { parent: p("Room"), name: Some("Lamp".into()), base: Base::None, patches: vec![], asset: None }] });
        assert_eq!(r.status, "written", "{r:?}");
        let lamp = r.created[0].1.expect("spawned");
        assert_eq!(g.app.world().get::<TsxNode>(lamp).unwrap().root, "level:levels/room.bsn");
    }
    #[test]
    fn the_earliest_refusal_wins() {
        let mut f = arena();
        let r = write(&mut f, ARENA, vec![Op::AddComponent { path: p("RoomA"), component: "Transform".into(), fields: vec![], text: None }, Op::Delete { path: p("Nobody") }]);
        assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("exists")), "op 0's refusal: {r:?}");
    }

    #[test]
    fn an_add_that_unmerges_an_included_file_reports_itself() {
        let hut = "#Hut Transform\n";
        let mut f = fixture("unmerge", &[("levels/arena.bsn", "#Hall\n:\"levels/hut.bsn\"\n"), ("levels/hut.bsn", hut)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let r = to_hut(&mut f, hut, vec![Op::Add { parent: vec![], name: Some("Lamp".into()), base: Base::None, patches: vec![], asset: None }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(r.created[0].1, Some(f.by_name("Lamp")), "the lamp, not the including Hall: {r:?}");
    }

    #[test]
    fn a_move_that_merges_an_included_file_keeps_the_entity() {
        let mut f = multi();
        let a = f.by_name("A");
        let r = to_hut(&mut f, MULTI, vec![Op::Delete { path: p("Shed") }, Op::Rename { path: p("Hut/A"), to: "B".into() }]);
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(f.by_name("B"), a, "hut.bsn now merges into Hall, and B is still A's entity");
    }
}

mod hot_reload {
    use super::*;
    use bevy_ts_levels::engine::content_hash;

    /// Saves `file` and waits until every level including it has applied the new text.
    fn save(f: &mut Fixture, file: &str, text: &str) {
        write(&f.dir, file, text);
        let hash = content_hash(text.as_bytes());
        let errors = f.errors().len();
        f.app.world().resource::<AssetServer>().reload(file.to_string());
        for _ in 0..300 {
            f.app.update();
            if f.errors().len() > errors {
                return;
            }
            let levels = f.app.world().resource::<Levels>();
            if levels.affected(file).iter().all(|l| levels.roots[l].applied.get(file) == Some(&hash)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("{file} never reloaded");
    }

    fn arena() -> Fixture {
        let mut f = fixture(&format!("reload-{:?}", std::thread::current().id()).replace(['(', ')'], ""), &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        f
    }

    fn spawned(f: &Fixture) -> usize {
        f.app.world().resource::<TsxTree>().last.spawned
    }

    #[test]
    fn an_unchanged_override_is_not_written_again() {
        let mut f = arena();
        let b = f.by_name("EnemyB");
        f.app.world_mut().get_mut::<Transform>(b).unwrap().translation.x = 7.0;
        save(&mut f, "levels/arena.bsn", &ARENA.replace("Health { max: 250.0 }", "Health { max: 300.0 }"));
        assert_eq!(f.translation(b).x, 7.0, "a system's move survives");
        assert_eq!(f.hp(b).1, 300.0);
    }

    #[test]
    fn a_removed_override_takes_the_default_again() {
        let mut f = arena();
        let a = f.by_name("EnemyA");
        save(&mut f, "levels/arena.bsn", &ARENA.replace("Transform { translation: Vec3 { x: -3.0, z: -4.0 } }", "Transform { translation: Vec3 { x: -3.0 } }"));
        assert_eq!(f.translation(a), Vec3::new(-3.0, 0.0, 0.0));
        let node = f.app.world().get::<TsxNode>(a).unwrap();
        assert_eq!(node.components["Transform"].fields["translation"].provenance, Provenance::Override);
        save(&mut f, "levels/arena.bsn", &ARENA.replace("Transform { translation: Vec3 { x: -3.0, z: -4.0 } }\n", "Transform\n"));
        assert_eq!(f.translation(a), Vec3::ZERO);
    }

    #[test]
    fn reordering_respawns_nothing() {
        let mut f = arena();
        let (a, b) = (f.by_name("EnemyA"), f.by_name("EnemyB"));
        let a_text = "#EnemyA\nTransform { translation: Vec3 { x: -3.0, z: -4.0 } }\ngame::Health { hp: 100.0, max: 100.0 }\n--\n";
        let b_text = "#EnemyB\nTransform { translation: Vec3 { x: 3.0, z: -4.0 } }\nHealth { max: 250.0 }\n--\n";
        let swapped = ARENA.replace(a_text, "").replace(b_text, &format!("{b_text}{a_text}"));
        assert_ne!(swapped, ARENA);
        save(&mut f, "levels/arena.bsn", &swapped);
        assert_eq!((f.by_name("EnemyA"), f.by_name("EnemyB")), (a, b));
        assert_eq!(spawned(&f), 0);
    }

    #[test]
    fn a_despawned_entity_stays_dead() {
        let mut f = arena();
        let a = f.by_name("EnemyA");
        f.app.world_mut().despawn(a);
        save(&mut f, "levels/arena.bsn", &ARENA.replace("max: 250.0", "max: 260.0"));
        let world = f.app.world_mut();
        assert!(world.query::<&Name>().iter(world).all(|n| n.as_str() != "EnemyA"), "tombstoned (rule 7)");
    }

    #[test]
    fn an_included_file_reloads_every_includer_copy() {
        let mut f = arena();
        save(&mut f, "levels/room.bsn", &ROOM.replace("x: 10.0", "x: 12.0"));
        let world = f.app.world_mut();
        let scales: Vec<f32> = world.query::<(&Name, &Transform)>().iter(world).filter(|(n, _)| n.as_str() == "Floor").map(|(_, t)| t.scale.x).collect();
        assert_eq!(scales, [12.0, 12.0]);
    }

    #[test]
    fn a_bad_file_changes_nothing_and_names_its_place() {
        let mut f = arena();
        let b = f.by_name("EnemyB");
        save(&mut f, "levels/arena.bsn", &ARENA.replace("Health { max: 250.0 }", "Health { max: {250.0} }"));
        assert_eq!(f.hp(b).1, 250.0);
        let errors = f.errors();
        assert!(errors.iter().any(|e| e.starts_with("levels/arena.bsn:8:15 levels: expressions are not supported")), "{errors:?}");
        // Fixed, it applies. Late reports of the bad file come first, so `save` does not take
        // them for the fixed file's.
        for _ in 0..20 {
            f.app.update();
        }
        save(&mut f, "levels/arena.bsn", &ARENA.replace("max: 250.0", "max: 270.0"));
        assert_eq!(f.hp(b).1, 270.0);
    }

    #[test]
    fn a_generation_whose_types_break_a_level_fails() {
        let mut f = arena();
        f.fake.push(FakeProgram {
            manifest: Manifest { components: vec![], ..Default::default() },
            sections: vec![Section { name: "levels".into(), bytes: serde_json::to_vec(&json!({ "levels": ["levels/arena.bsn"] })).unwrap(), location: None }],
            ..Default::default()
        });
        let r = generation::apply(f.app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
        assert_eq!(r.status, ReloadStatus::Failed, "{r:?}");
        assert!(r.errors.iter().any(|e| e.message.contains("unknown component game::Health")), "{:?}", r.errors);
    }
}

mod viewport_routes {
    use super::*;
    use bevy_ts_levels::{WriteResults, route, write as write_field};

    #[test]
    fn a_level_entity_routes_bsn_and_a_drag_writes_one_lane() {
        let mut f = fixture("routes", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        let b = f.by_name("EnemyB");
        let r = route(f.app.world(), b, "Transform", "translation").unwrap();
        assert_eq!(r["kind"], "bsn");
        assert_eq!(r["namePath"], json!(["EnemyB"]));
        assert_eq!(r["target"], json!({ "file": "levels/arena.bsn", "namePath": ["EnemyB"] }));
        assert_eq!(r["policy"], "override");
        write_field(f.app.world_mut(), 42, b, "Transform", "translation", json!([5.0, 0.0, -4.0])).unwrap();
        let results = f.app.world_mut().resource_mut::<WriteResults>().drain();
        assert_eq!((results[0].id, results[0].status), (42, "written"), "{results:?}");
        let text = std::fs::read_to_string(f.dir.join("levels/arena.bsn")).unwrap();
        assert_eq!(text, ARENA.replace("x: 3.0, z: -4.0", "x: 5.0, z: -4.0"));
        assert_eq!(f.translation(b).x, 5.0);
        let floor = {
            let world = f.app.world_mut();
            world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == "Floor").map(|(e, _)| e).unwrap()
        };
        let r = route(f.app.world(), floor, "Transform", "scale").unwrap();
        assert_eq!(r["owners"].as_array().unwrap().len(), 2);
        let loose = f.app.world_mut().spawn(Transform::default()).id();
        assert!(route(f.app.world(), loose, "Transform", "translation").is_none());
    }
}

mod sidecar {
    use super::*;
    use bevy_ts_levels::{WriteResults, write_sidecar};

    fn scene_program() -> FakeProgram {
        let scene = json!({ "roots": [{ "name": "arena", "nodes": [{
            "key": "p", "type": "entity", "name": "player",
            "components": [{ "type": "Transform", "kind": "rust", "props": { "translation": [0, 0, -4] } }],
            "children": [],
        }] }] });
        FakeProgram {
            manifest: Manifest { components: vec![health()], ..Default::default() },
            sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(&scene).unwrap(), location: None }],
            ..Default::default()
        }
    }

    fn apply(f: &mut Fixture) {
        f.fake.push(scene_program());
        let r = generation::apply(f.app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        for _ in 0..50 {
            f.app.update();
        }
    }

    #[test]
    fn a_computed_scene_edit_persists_in_a_sidecar_level() {
        let mut f = fixture("sidecar", &[]);
        apply(&mut f);
        let p = f.by_name("player");
        write_sidecar(f.app.world_mut(), 9, p, "Transform", "translation", json!([2.0, 0.0, -4.0])).unwrap();
        let r = f.app.world_mut().resource_mut::<WriteResults>().drain().pop().unwrap();
        assert_eq!(r.status, "written", "{r:?}");
        let text = std::fs::read_to_string(f.dir.join("levels/arena.bsn")).unwrap();
        // The first write creates the file, which loads like any asset.
        for _ in 0..200 {
            f.app.update();
            if f.translation(p).x == 2.0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(text, "#p\nTransform { translation: Vec3 { x: 2.0, y: 0.0, z: -4.0 } }\n");
        assert_eq!(f.translation(p), Vec3::new(2.0, 0.0, -4.0), "{:?} {:?}", f.errors(), f.app.world().resource::<Levels>().roots.keys().collect::<Vec<_>>());
        assert_eq!(f.app.world().get::<TsxNode>(p).unwrap().components["Transform"].fields["translation"].provenance, Provenance::Override);
        assert_eq!(f.app.world().get::<Name>(p).unwrap().as_str(), "player", "the sidecar's names are keys only");
        // The next generation layers the sidecar again; a fresh app opens it from disk.
        apply(&mut f);
        assert_eq!(f.by_name("player"), p);
        assert_eq!(f.translation(p), Vec3::new(2.0, 0.0, -4.0), "{:?} {:?} {:?}", f.app.world().get::<TsxNode>(p).unwrap().components["Transform"], f.app.world().resource::<TsxTree>().overlays.keys().collect::<Vec<_>>(), f.app.world().resource::<ScriptLog>().since(0).map(|e| e.message.clone()).collect::<Vec<_>>());
        let mut g = fixture("sidecar-reopen", &[("levels/arena.bsn", &text)]);
        apply(&mut g);
        let p = g.by_name("player");
        assert_eq!(g.translation(p), Vec3::new(2.0, 0.0, -4.0));
    }
    #[test]
    fn an_add_to_a_scene_with_no_level_creates_its_sidecar() {
        use bevy_ts_levels::edit::{WriteRequest, write_level};
        use bevy_ts_levels::engine::content_hash;
        use bevy_ts_levels::writer::{Base, NewPatch, Op, WValue};
        let mut f = fixture("sidecar-add", &[]);
        apply(&mut f);
        let file = "levels/arena.bsn";
        let health = NewPatch { component: "game::Health".into(), fields: vec![("max".into(), WValue::Float { v: 5.0, f32: true })] };
        let ops = vec![Op::Add { parent: Vec::new(), name: Some("Light".into()), base: Base::None, patches: vec![health], asset: None }];
        let r = write_level(f.app.world_mut(), WriteRequest { id: 1, file: file.into(), base: content_hash(b""), ops });
        assert_eq!(r.status, "written", "{r:?}");
        assert_eq!(std::fs::read_to_string(f.dir.join(file)).unwrap(), "#Light\ngame::Health { max: 5.0 }\n");
        assert_eq!(f.app.world().resource::<Levels>().roots[file].overlay.as_deref(), Some("arena"));
        let light = r.created[0].1.expect("spawned in the scene");
        assert_eq!(f.app.world().get::<TsxNode>(light).unwrap().root, "arena");
        assert_eq!(f.hp(light).1, 5.0);
        // The scene's TSX entities stay the scene's.
        let text = std::fs::read_to_string(f.dir.join(file)).unwrap();
        for op in [Op::Delete { path: vec!["p".into()] }, Op::Rename { path: vec!["p".into()], to: "q".into() }] {
            let r = write_level(f.app.world_mut(), WriteRequest { id: 2, file: file.into(), base: content_hash(text.as_bytes()), ops: vec![op] });
            assert_eq!((r.status, r.reason.as_deref()), ("refused", Some("prefab-internal")), "{r:?}");
        }
        // What the sidecar added, it can delete.
        let r = write_level(f.app.world_mut(), WriteRequest { id: 3, file: file.into(), base: content_hash(text.as_bytes()), ops: r.inverse.clone() });
        assert_eq!(r.status, "written", "{r:?}");
        assert!(f.app.world().get_entity(light).is_err());
    }
}

/// `bevy-ts/level_open` and `level_close` (change level-open), through the engine calls they serve.
mod open {
    use super::*;
    use bevy_ts_levels::edit::{WriteRequest, write_level};
    use bevy_ts_levels::engine::{Opener, close_now, content_hash, open_now};
    use bevy_ts_levels::writer::{Op, WValue};

    fn arena(name: &str) -> Fixture {
        let mut f = fixture(&format!("open-{name}"), &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        // A generation with the game's components and no `app.level`.
        f.load(&[]);
        f
    }

    fn names(f: &mut Fixture) -> Vec<String> {
        let world = f.app.world_mut();
        let mut v: Vec<String> = world.query::<&Name>().iter(world).map(|n| n.to_string()).collect();
        v.sort();
        v
    }

    fn opener(f: &Fixture, path: &str) -> Opener {
        f.app.world().resource::<Levels>().roots[path].opener
    }

    /// `level+watch` messages since `seq`, as `event level`.
    fn events(f: &Fixture, seq: u64) -> Vec<String> {
        f.app.world().resource::<Levels>().events.iter().filter(|(s, _)| *s > seq).map(|(_, m)| format!("{} {}", m["event"].as_str().unwrap(), m["level"].as_str().unwrap())).collect()
    }

    #[test]
    fn opening_the_arena_reconciles_it_before_it_returns() {
        let mut f = arena("arena");
        // No frame runs between the call and the check: the open ran no system.
        assert_eq!(open_now(f.app.world_mut(), "levels/arena.bsn").unwrap(), "levels/arena.bsn");
        assert_eq!(opener(&f, "levels/arena.bsn"), Opener::Editor);
        let b = f.by_name("EnemyB");
        assert_eq!(f.app.world().get::<TsxNode>(b).unwrap().root, "level:levels/arena.bsn");
        assert_eq!(f.hp(b), (100.0, 250.0));
        assert_eq!(names(&mut f).iter().filter(|n| *n == "Floor").count(), 2);
        let levels = f.app.world().resource::<Levels>();
        let r = &levels.roots["levels/arena.bsn"];
        assert!(!r.dirty && r.error.is_none());
        assert!(r.files.contains("levels/room.bsn"));
        // The loads the asset server started bring nothing new.
        let x = f.translation(b);
        f.settle();
        assert_eq!(f.by_name("EnemyB"), b);
        assert_eq!(f.translation(b), x);
        assert!(f.errors().is_empty(), "{:?}", f.errors());
    }

    #[test]
    fn opening_an_open_level_changes_nothing() {
        let mut f = arena("twice");
        open_now(f.app.world_mut(), "levels/arena.bsn").unwrap();
        f.settle();
        let b = f.by_name("EnemyB");
        let seq = f.app.world().resource::<Levels>().seq;
        let written = f.app.world().resource::<TsxTree>().last.written;
        assert_eq!(open_now(f.app.world_mut(), "./levels/arena.bsn").unwrap(), "levels/arena.bsn");
        assert_eq!(f.by_name("EnemyB"), b);
        assert_eq!(f.app.world().resource::<TsxTree>().last.written, written);
        assert!(events(&f, seq).is_empty(), "{:?}", events(&f, seq));
        assert_eq!(f.app.world().resource::<Levels>().roots.len(), 1);
    }

    #[test]
    fn a_typo_opens_nothing() {
        let mut f = arena("typo");
        write(&f.dir, "levels/bad.bsn", &ARENA.replace("Health { max: 250.0 }", "Health { max: 250.0 }\nTransfrom { translation: Vec3 { x: 9.0 } }"));
        let e = open_now(f.app.world_mut(), "levels/bad.bsn").unwrap_err();
        assert_eq!(e.reason, "load");
        let at = e.diagnostics[0].location.as_ref().unwrap();
        assert_eq!((at.file.as_str(), at.line), ("levels/bad.bsn", 9), "{:?}", e.diagnostics);
        assert!(e.diagnostics[0].message.contains("Transfrom"), "{:?}", e.diagnostics);
        assert!(f.app.world().resource::<Levels>().roots.is_empty());
        assert!(!names(&mut f).contains(&"EnemyA".to_string()));
        // A file that does not parse says where.
        write(&f.dir, "levels/broken.bsn", "#A\nTransform {\n");
        let e = open_now(f.app.world_mut(), "levels/broken.bsn").unwrap_err();
        assert_eq!(e.reason, "load");
        assert_eq!(e.diagnostics[0].location.as_ref().unwrap().file, "levels/broken.bsn", "{:?}", e.diagnostics);
        // A missing inclusion says where it is included.
        write(&f.dir, "levels/holed.bsn", "#A\n:\"./gone.bsn\"\n");
        let e = open_now(f.app.world_mut(), "levels/holed.bsn").unwrap_err();
        assert_eq!(e.reason, "load");
        let at = e.diagnostics[0].location.as_ref().unwrap();
        assert_eq!((at.file.as_str(), at.line), ("levels/holed.bsn", 2), "{:?}", e.diagnostics);
        assert!(f.app.world().resource::<Levels>().roots.is_empty());
    }

    #[test]
    fn a_missing_or_outside_file_is_refused() {
        let mut f = arena("missing");
        assert_eq!(open_now(f.app.world_mut(), "levels/nope.bsn").unwrap_err().reason, "not-found");
        for bad in ["../outside.bsn", "levels/../../x.bsn", "/abs/arena.bsn", "levels/arena.txt"] {
            assert_eq!(open_now(f.app.world_mut(), bad).unwrap_err().reason, "invalid", "{bad}");
        }
        assert!(f.app.world().resource::<Levels>().roots.is_empty());
    }

    #[test]
    fn switching_levels() {
        let mut f = arena("switch");
        open_now(f.app.world_mut(), "levels/arena.bsn").unwrap();
        let b = f.by_name("EnemyB");
        let seq = f.app.world().resource::<Levels>().seq;
        open_now(f.app.world_mut(), "levels/room.bsn").unwrap();
        assert!(f.app.world().get_entity(b).is_ok(), "opening one level closes no other");
        assert!(close_now(f.app.world_mut(), "levels/arena.bsn"));
        assert!(f.app.world().get_entity(b).is_err());
        assert_eq!(names(&mut f), ["Floor", "Room"]);
        assert!(!close_now(f.app.world_mut(), "levels/arena.bsn"), "not open");
        // What a Hierarchy watching `level+watch` sees.
        assert_eq!(events(&f, seq), ["opened levels/room.bsn", "loaded levels/room.bsn", "closed levels/arena.bsn"]);
        let opened = f.app.world().resource::<Levels>().events.iter().find(|(s, _)| *s > seq).unwrap().1.clone();
        assert_eq!(opened["opener"], "editor");
    }

    #[test]
    fn a_level_the_game_declares_stays_closed_across_a_generation() {
        let mut f = fixture("open-declared", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        assert_eq!(opener(&f, "levels/arena.bsn"), Opener::Ts);
        let b = f.by_name("EnemyB");
        assert!(close_now(f.app.world_mut(), "levels/arena.bsn"));
        assert!(f.app.world().get_entity(b).is_err());
        f.load(&["levels/arena.bsn"]);
        f.settle();
        assert!(f.app.world().resource::<Levels>().roots.is_empty(), "a generation does not reopen it");
        open_now(f.app.world_mut(), "levels/arena.bsn").unwrap();
        assert_eq!(opener(&f, "levels/arena.bsn"), Opener::Ts);
        f.by_name("EnemyB");
        // Open again, it is the game's: a generation without it closes it.
        f.load(&[]);
        f.app.update();
        assert!(f.app.world().resource::<Levels>().roots.is_empty());
    }

    #[test]
    fn an_app_level_level_opened_by_the_editor_keeps_its_opener() {
        let mut f = fixture("open-keeps", &[("levels/arena.bsn", ARENA), ("levels/room.bsn", ROOM)]);
        f.load(&["levels/arena.bsn"]);
        f.settle();
        open_now(f.app.world_mut(), "levels/arena.bsn").unwrap();
        assert_eq!(opener(&f, "levels/arena.bsn"), Opener::Ts);
    }

    #[test]
    fn writing_an_opened_level() {
        let mut f = arena("write");
        open_now(f.app.world_mut(), "levels/room.bsn").unwrap();
        let floor = f.by_name("Floor");
        let op = Op::Set { path: vec!["Room".into(), "Floor".into()], component: "Transform".into(), field: vec!["translation".into(), "y".into()], value: WValue::Float { v: 2.0, f32: true }, containers: vec!["Vec3".into()] };
        let r = write_level(f.app.world_mut(), WriteRequest { id: 1, file: "levels/room.bsn".into(), base: content_hash(ROOM.as_bytes()), ops: vec![op] });
        assert_eq!(r.status, "written", "{r:?}");
        assert!(std::fs::read_to_string(f.dir.join("levels/room.bsn")).unwrap().contains("y: 2.0"));
        assert_eq!(f.by_name("Floor"), floor);
        assert_eq!(f.translation(floor).y, 2.0);
    }
}
