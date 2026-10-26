//! Heap fields on TS components and resources (openspecs ts-component-heap-fields,
//! ts-heap-fields-2): every path that builds, moves, clones or drops a `string` or `T[]` field
//! frees each value once, counted by its shape's live values. On the fake backend: components are
//! declared by a generation, written as commands.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::commands::{self, CommandKinds, NativeRecipes, insert_owned};
use bevy_ts::components::TsComponents;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::heap::HeapShape;
use bevy_ts::layout::FieldType;
use bevy_ts::fake::FakeFn;
use bevy_ts::manifest::{ComponentDecl, ComponentRef, ComponentSource, FieldDecl, Lifecycle, Manifest, ParamDecl, ResourceDecl, ScheduleDecl, Stage, SystemDecl};
use bevy_ts::{places, plugin_api, reflect_codec, value};
use std::sync::{Arc, Mutex};
use serde_json::{Value, json};
use std::path::PathBuf;

fn field(name: &str, ty: FieldType, default: Option<Value>) -> FieldDecl {
    FieldDecl { name: name.into(), ty, default, hints: Default::default() }
}

fn list(of: FieldType) -> FieldType {
    FieldType::List { of: Box::new(of) }
}

/// `name { hp: f32, label: string = "crate", scores: f32[] = [1, 2], flags: bool[], names: string[] }`
/// plus `extra`.
fn tag(name: &str, extra: Vec<FieldDecl>) -> Manifest {
    let mut fields = vec![
        field("hp", FieldType::F32, None),
        field("label", FieldType::Str, Some(json!("crate"))),
        field("scores", list(FieldType::F32), Some(json!([1, 2]))),
        field("flags", list(FieldType::Bool), None),
        field("names", list(FieldType::Str), None),
    ];
    fields.extend(extra);
    Manifest { components: vec![ComponentDecl { name: name.into(), fields, ..Default::default() }], ..Default::default() }
}

struct Game {
    app: App,
    fake: FakeHandle,
}

impl Game {
    fn new(manifest: Manifest) -> Game {
        let fake = FakeHandle::default();
        let h = fake.clone();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
        let mut game = Game { app, fake };
        game.declare(manifest);
        game
    }

    fn declare(&mut self, manifest: Manifest) {
        self.fake.push(FakeProgram { manifest, ..Default::default() });
        let r = generation::apply(self.app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    }

    fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// The live shape of `name`.
    fn shape(&self, name: &str) -> &'static HeapShape {
        let c = self.app.world().resource::<TsComponents>().live(name).unwrap();
        HeapShape::of(name, &c.layout).unwrap()
    }

    fn spawn(&mut self, name: &str, v: Value) -> Entity {
        let world = self.app.world_mut();
        let (id, _) = plugin_api::component_by_name(world, name).unwrap();
        let bytes = plugin_api::encode_component(world, name, v.as_object().unwrap()).unwrap();
        let mut e = world.spawn_empty();
        // SAFETY: encoded with the component's own layout.
        unsafe { insert_owned(&mut e, id, bytes) };
        e.id()
    }

    fn read(&self, e: Entity, name: &str) -> Option<Value> {
        let world = self.app.world();
        let c = world.resource::<TsComponents>().get(name)?;
        let p = world.get_by_id(e, c.id)?;
        // SAFETY: the column holds a live value of `c.layout`.
        Some(unsafe { value::decode_component(&c.layout, std::slice::from_raw_parts(p.as_ptr(), c.layout.size)) })
    }

    /// Applies a command buffer as a run's commands are.
    fn commands(&mut self, bytes: &[u8]) -> Vec<String> {
        let world = self.app.world_mut();
        let sizes = world.resource::<TsComponents>().writable_sizes(world.resource::<bevy_ts::layout::NativeLayouts>());
        let ops = commands::decode(bytes, &sizes, &CommandKinds::default()).unwrap();
        commands::apply(world, ops, &CommandKinds::default(), &NativeRecipes::default())
    }
}

/// A spawn op of one component: JS's bytes (zeroes), then `heap` as a patch of field values.
fn spawn_op(e: Entity, id: u32, size: usize, heap: &[(u16, &str)]) -> Vec<u8> {
    let mut body = vec![0u8; size];
    if !heap.is_empty() {
        body.extend_from_slice(&(heap.len() as u16).to_le_bytes());
        for (i, s) in heap {
            body.extend_from_slice(&i.to_le_bytes());
            body.extend_from_slice(&(s.len() as u32).to_le_bytes());
            body.extend_from_slice(s.as_bytes());
        }
    }
    let mut out = vec![commands::OP_SPAWN];
    out.extend_from_slice(&e.to_bits().to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[test]
fn spawn_insert_despawn_and_world_drop_free_each_string_once() {
    let name = "it::heap_fields::Spawned";
    let mut g = Game::new(tag(name, vec![]));
    let shape = g.shape(name);
    let a = g.spawn(name, json!({ "label": "door", "hp": 2, "scores": [3.5], "flags": [true, false], "names": ["x", "y"] }));
    let b = g.spawn(name, json!({}));
    assert_eq!(g.read(a, name).unwrap(), json!({ "hp": 2.0, "label": "door", "scores": [3.5], "flags": [true, false], "names": ["x", "y"] }));
    assert_eq!(g.read(b, name).unwrap(), json!({ "hp": 0.0, "label": "crate", "scores": [1.0, 2.0], "flags": [], "names": [] }), "defaults");
    assert_eq!(shape.live(), 2);
    // Inserting over a value drops the old one.
    let world = g.world();
    let (id, _) = plugin_api::component_by_name(world, name).unwrap();
    let bytes = plugin_api::encode_component(world, name, json!({ "label": "gate" }).as_object().unwrap()).unwrap();
    // SAFETY: encoded with the component's own layout.
    unsafe { insert_owned(&mut world.entity_mut(a), id, bytes) };
    assert_eq!(g.read(a, name).unwrap()["label"], json!("gate"));
    assert_eq!(shape.live(), 2);
    g.world().despawn(b);
    assert_eq!(shape.live(), 1);
    drop(g);
    assert_eq!(shape.live(), 0, "the world's drop frees what its columns hold");
}

#[test]
fn a_spawn_command_builds_its_strings_in_rust() {
    let name = "it::heap_fields::Commanded";
    let mut g = Game::new(tag(name, vec![]));
    let shape = g.shape(name);
    let c = g.world().resource::<TsComponents>().live(name).unwrap().clone();
    let (a, b) = (g.world().entity_allocator().alloc(), g.world().entity_allocator().alloc());
    let label = c.layout.fields.iter().position(|f| f.name == "label").unwrap() as u16;
    let mut ops = spawn_op(a, c.id.index() as u32, c.layout.size, &[(label, "lamp")]);
    ops.extend(spawn_op(b, c.id.index() as u32, c.layout.size, &[]));
    assert!(g.commands(&ops).is_empty());
    assert_eq!(g.read(a, name).unwrap()["label"], json!("lamp"));
    assert_eq!(g.read(b, name).unwrap()["label"], json!("crate"), "a value the command leaves out takes its default");
    assert_eq!(shape.live(), 2);
    // A heap slot JS filled is never read: Rust writes every one.
    let world = g.world();
    let sizes = world.resource::<TsComponents>().writable_sizes(world.resource::<bevy_ts::layout::NativeLayouts>());
    let mut bad = spawn_op(world.entity_allocator().alloc(), c.id.index() as u32, c.layout.size, &[(label, "x")]);
    let at = bad.len() - 2;
    bad[at] = 0xff; // the string's length's high byte: now past the buffer
    assert!(commands::decode(&bad, &sizes, &CommandKinds::default()).is_err());
    assert_eq!(shape.live(), 2, "a bad command builds nothing that lives");
}

#[test]
fn clone_and_spawn_gives_the_clone_its_own_string() {
    let name = "it::heap_fields::Cloned";
    let mut g = Game::new(tag(name, vec![]));
    let shape = g.shape(name);
    let a = g.spawn(name, json!({ "label": "door", "names": ["n"] }));
    let clone = g.world().entity_allocator().alloc();
    let mut op = vec![commands::OP_CLONE_AND_SPAWN];
    op.extend_from_slice(&a.to_bits().to_le_bytes());
    op.extend_from_slice(&clone.to_bits().to_le_bytes());
    assert!(g.commands(&op).is_empty());
    assert_eq!(g.read(clone, name).unwrap()["label"], json!("door"));
    assert_eq!(shape.live(), 2);
    g.world().despawn(a);
    assert_eq!(g.read(clone, name).unwrap()["label"], json!("door"), "the clone's string outlives its source");
    assert_eq!(g.read(clone, name).unwrap()["names"], json!(["n"]), "and its list");
    g.world().despawn(clone);
    assert_eq!(shape.live(), 0);
}

#[test]
fn reload_moves_kept_strings_and_frees_reset_ones() {
    let name = "it::heap_fields::Reloaded";
    let mut g = Game::new(tag(name, vec![]));
    let first = g.shape(name);
    let a = g.spawn(name, json!({ "label": "door", "names": ["a", "b"] }));
    // Adding a field moves the string and the lists into the new value.
    g.declare(tag(name, vec![field("weight", FieldType::F32, Some(json!(1)))]));
    let second = g.shape(name);
    assert_eq!(g.read(a, name).unwrap(), json!({ "hp": 0.0, "label": "door", "scores": [1.0, 2.0], "flags": [], "names": ["a", "b"], "weight": 1.0 }));
    assert_eq!((first.live(), second.live()), (0, 1), "the old value is gone, the new one holds the string");
    // Retyping the string, or a list's items, resets it to its default and frees the old value.
    let retyped = Manifest {
        components: vec![ComponentDecl {
            name: name.into(),
            fields: vec![field("label", FieldType::F32, Some(json!(5))), field("names", list(FieldType::U8), Some(json!([7])))],
            ..Default::default()
        }],
        ..Default::default()
    };
    g.declare(retyped);
    assert_eq!(g.read(a, name).unwrap(), json!({ "label": 5.0, "names": [7] }));
    assert_eq!(second.live(), 0);
    let third = g.shape(name);
    g.world().despawn(a);
    assert_eq!(third.live(), 0);
}

#[test]
fn an_undeclared_component_still_frees_its_strings() {
    let name = "it::heap_fields::Undeclared";
    let mut g = Game::new(tag(name, vec![]));
    let shape = g.shape(name);
    let a = g.spawn(name, json!({ "label": "door" }));
    g.declare(Manifest::default());
    assert_eq!(g.read(a, name).unwrap()["label"], json!("door"), "its data stays on the entity");
    g.world().despawn(a);
    assert_eq!(shape.live(), 0, "the drop fn reads the value's header, not the registry");
}

#[test]
fn heap_fields_are_refused_on_messages() {
    let mut g = Game::new(Manifest::default());
    let bad = Manifest {
        messages: vec![bevy_ts::manifest::PayloadDecl { name: "it::heap_fields::Said".into(), fields: vec![field("text", FieldType::Str, None)], entity: false, propagate: false, added: true, location: None }],
        ..Default::default()
    };
    g.fake.push(FakeProgram { manifest: bad, ..Default::default() });
    let r = generation::apply(g.world(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
    assert_ne!(r.status, ReloadStatus::Applied);
    assert!(r.errors.iter().any(|e| e.message.contains("it::heap_fields::Said.text") && e.message.contains("allowed on components, resources and Local only")), "{:?}", r.errors);
}

/// `name { text: string = "x", n: f32 }`, inserted with `text: "hello"`.
fn title(name: &str) -> Manifest {
    let fields = vec![field("text", FieldType::Str, Some(json!("x"))), field("n", FieldType::F32, None)];
    let insert = Some(json!({ "text": "hello" }).as_object().unwrap().clone());
    Manifest { resources: vec![ResourceDecl { name: name.into(), fields, insert, location: None }], ..Default::default() }
}

#[test]
fn a_resource_string_is_freed_once_on_restart_and_world_drop() {
    let name = "it::heap_fields::Title";
    let mut g = Game::new(title(name));
    let shape = g.shape(name);
    let value = |g: &Game| {
        let id = g.app.world().resource::<TsComponents>().live(name).unwrap().id;
        let e = g.app.world().resource_entities().get(id).unwrap();
        g.read(e, name).unwrap()
    };
    assert_eq!(value(&g), json!({ "text": "hello", "n": 0.0 }));
    assert_eq!(shape.live(), 1);
    g.fake.push(FakeProgram { manifest: title(name), ..Default::default() });
    let r = generation::apply(g.world(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, true);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(value(&g), json!({ "text": "hello", "n": 0.0 }));
    assert_eq!(shape.live(), 1, "the restart freed the old value and inserted a new one");
    drop(g);
    assert_eq!(shape.live(), 0);
}

#[test]
fn a_lifecycle_value_is_a_copy_read_at_the_observer_place_and_freed_once() {
    let name = "it::heap_fields::Removed";
    let mut manifest = tag(name, vec![]);
    let decl = |n: &str, schedule, params| SystemDecl { name: n.into(), schedule, params, after: vec![], before: vec![], sets: vec![], condition: None, groups: vec![], location: None };
    let on = ParamDecl::On { event: None, lifecycle: Some(Lifecycle::Remove), component: Some(ComponentRef { name: name.into(), source: ComponentSource::Ts }) };
    manifest.systems = vec![decl("game.ts#gone", ScheduleDecl::Observer, vec![on]), decl("game.ts#late", ScheduleDecl::Stage { stage: Stage::Update }, vec![])];
    let saw = Arc::new(Mutex::new(Vec::<String>::new()));
    // Reads `label` at the observer place: during an observer's run, and from a system, where no
    // observer is running.
    let read = |saw: Arc<Mutex<Vec<String>>>| -> FakeFn {
        Box::new(move |call| {
            let t = call.resolution.holes["it::heap_fields::Removed"].iter().find(|h| h.name == "label").unwrap().target;
            let mut buf = vec![0u8; 64];
            let got = match call.host_call(reflect_codec::READ, &[places::OBSERVER, 0.0, t as f64], &mut [&mut buf]) {
                Ok(ret) => {
                    let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
                    assert_eq!(ret[0] as usize, 4 + n);
                    String::from_utf8(buf[4..4 + n].to_vec()).unwrap()
                }
                Err(e) => e.message,
            };
            saw.lock().unwrap().push(got);
            Ok(())
        })
    };
    let mut g = Game::new(Manifest::default());
    g.fake.push(FakeProgram { manifest, systems: vec![read(saw.clone()), read(saw.clone())], ..Default::default() });
    let r = generation::apply(g.world(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    generation::start_game(g.world());
    let shape = g.shape(name);
    let a = g.spawn(name, json!({ "label": "door" }));
    g.app.update();
    saw.lock().unwrap().clear();
    g.world().despawn(a);
    assert_eq!(shape.live(), 1, "the queued event holds a copy of the value the entity dropped");
    g.app.update();
    let saw = saw.lock().unwrap().clone();
    assert!(saw.contains(&"door".to_string()), "the observer reads the value as it was: {saw:?}");
    assert!(saw.iter().any(|m| m.contains("no observer is running")), "a system names the place that is not live: {saw:?}");
    assert_eq!(shape.live(), 0, "the drain dropped the copy once its observer ran");
}
