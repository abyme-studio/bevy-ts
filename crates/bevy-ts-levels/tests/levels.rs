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
    assert_eq!(files, ["levels/arena.bsn", "levels/room.bsn", "levels/room.bsn"]);
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
    use bevy_ts_levels::writer::{Op, WValue};

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
}
