//! A test feature crate that uses every extension point of the plugin API against the fake
//! backend: no V8 in this build.

use bevy::prelude::*;
use bevy_ts::backend::{ModuleSet, ScriptError, SourceLocation};
use bevy_ts::commands::TsSpawned;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, Generations, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{ComponentDecl, EntryDecl, FieldDecl, Manifest, Section};
use bevy_ts::plugin_api::{self, BevyTsAppExt};
use bevy_ts::BevyTsPlugin;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
    (app, handle)
}

fn set() -> ModuleSet {
    ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }
}

fn apply(app: &mut App) -> generation::ReloadReport {
    generation::apply(app.world_mut(), set(), false)
}

fn at(line: u32, column: u32) -> Option<SourceLocation> {
    Some(SourceLocation { file: "game.tsx".into(), line, column })
}

fn health() -> ComponentDecl {
    ComponentDecl {
        name: "game::Health".into(),
        fields: vec![
            FieldDecl { name: "hp".into(), ty: FieldType::F32, default: None, hints: Default::default() },
            FieldDecl { name: "max".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
        ],
        location: None,
    }
}

/// What the feature crate saw, in order.
#[derive(Resource, Default, Clone)]
struct Seen(Arc<Mutex<Vec<String>>>);

impl Seen {
    fn push(&self, s: impl Into<String>) {
        self.0.lock().unwrap().push(s.into());
    }
    fn list(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// The feature crate: a section, a command kind and a restart hook.
fn feature(app: &mut App) -> Seen {
    let seen = Seen::default();
    let s = seen.clone();
    app.add_ts_section("test/scene", move |world, input| {
        let text = String::from_utf8_lossy(input.bytes).to_string();
        if text == "reject" {
            return Err(vec![ScriptError::at("scene rejected", at(12, 5))]);
        }
        // Components of the same generation are already resolvable.
        let found = input.components.lookup("game::Health").map(|(_, l)| l.size);
        s.push(format!("section {text} health={found:?} gen={}", input.generation));
        let _ = world;
        Ok(())
    });
    let s = seen.clone();
    app.add_ts_command_kind("test/name", move |world, bytes| {
        let e = Entity::from_bits(u64::from_le_bytes(bytes.try_into().map_err(|_| "8 bytes")?));
        let exists = world.get_entity(e).is_ok();
        s.push(format!("name op: target exists={exists}"));
        world.get_entity_mut(e).map_err(|e| e.to_string())?.insert(Name::new("named"));
        Ok(())
    });
    let s = seen.clone();
    app.on_ts_restart(move |world| {
        let spawned = world.query_filtered::<Entity, With<TsSpawned>>().iter(world).count();
        let live = world.resource::<Generations>().live;
        s.push(format!("restart: ts-spawned={spawned} live={live}"));
    });
    app.insert_resource(seen.clone());
    seen
}

#[test]
fn a_rejected_section_fails_the_generation_with_its_location() {
    let (mut app, fake) = app();
    let seen = feature(&mut app);
    let ok = FakeProgram {
        sections: vec![Section { name: "test/scene".into(), bytes: b"first".to_vec(), location: None }],
        ..Default::default()
    };
    fake.push(ok);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    fake.push(FakeProgram {
        sections: vec![Section { name: "test/scene".into(), bytes: b"reject".to_vec(), location: None }],
        ..Default::default()
    });
    let r = apply(&mut app);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert_eq!(r.errors[0].location, at(12, 5));
    assert_eq!(r.errors[0].to_string(), "game.tsx:12:5: scene rejected");
    assert_eq!(app.world().resource::<Generations>().live, 1);
    assert_eq!(seen.list(), vec!["section first health=None gen=1"]);
}

#[test]
fn sections_apply_in_the_same_step_as_components() {
    let (mut app, fake) = app();
    let seen = feature(&mut app);
    fake.push(FakeProgram {
        manifest: Manifest { components: vec![health()], ..Default::default() },
        sections: vec![Section { name: "test/scene".into(), bytes: b"s".to_vec(), location: None }],
        ..Default::default()
    });
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    assert_eq!(seen.list(), vec!["section s health=Some(8) gen=1"]);
}

#[test]
fn an_unregistered_section_fails_the_load() {
    let (mut app, fake) = app();
    fake.push(FakeProgram {
        sections: vec![Section { name: "nobody/owns".into(), bytes: vec![], location: at(3, 1) }],
        ..Default::default()
    });
    let r = apply(&mut app);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("nobody/owns"));
}

fn with_entry(name: &str, f: bevy_ts::fake::FakeFn) -> FakeProgram {
    let mut p = FakeProgram {
        manifest: Manifest { entries: vec![EntryDecl { name: name.into(), location: None }], ..Default::default() },
        ..Default::default()
    };
    p.entries.insert(name.into(), f);
    p
}

#[test]
fn an_entry_costs_no_call_until_the_plugin_calls_it() {
    let (mut app, fake) = app();
    fake.push(with_entry(
        "test/render",
        Box::new(|call| {
            call.output = [b"echo:".as_slice(), call.payload].concat();
            Ok(())
        }),
    ));
    apply(&mut app);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(fake.entry_calls("test/render"), 0);
    let out = plugin_api::call_entry(app.world_mut(), "test/render", b"hi").unwrap();
    assert_eq!(out, b"echo:hi");
    assert_eq!(fake.entry_calls("test/render"), 1);
}

#[test]
fn a_throwing_entry_discards_its_commands() {
    let (mut app, fake) = app();
    fake.push(with_entry(
        "test/boom",
        Box::new(|call| {
            call.commands.spawn(&[])?;
            Err(ScriptError::at("boom", at(4, 9)))
        }),
    ));
    apply(&mut app);
    let before = app.world().entities().count_spawned();
    let e = plugin_api::call_entry(app.world_mut(), "test/boom", b"").unwrap_err();
    assert_eq!(e.location, at(4, 9));
    assert_eq!(app.world().entities().count_spawned(), before);
}

#[test]
fn plugin_ops_apply_in_order_with_built_in_ones() {
    let (mut app, fake) = app();
    let seen = feature(&mut app);
    let victim = app.world_mut().spawn_empty().id();
    let opcode = app.world().resource::<bevy_ts::commands::CommandKinds>().opcode("test/name").unwrap();
    fake.push(with_entry(
        "test/ops",
        Box::new(move |call| {
            let e = call.commands.spawn(&[])?;
            call.commands.encoder.plugin(opcode, &e.to_bits().to_le_bytes());
            call.commands.encoder.despawn(victim);
            call.output = e.to_bits().to_le_bytes().to_vec();
            Ok(())
        }),
    ));
    apply(&mut app);
    let out = plugin_api::call_entry(app.world_mut(), "test/ops", b"").unwrap();
    let spawned = Entity::from_bits(u64::from_le_bytes(out.try_into().unwrap()));
    assert_eq!(seen.list(), vec!["name op: target exists=true"]);
    assert_eq!(app.world().get::<Name>(spawned).unwrap().as_str(), "named");
    assert!(app.world().get_entity(victim).is_err());
}

#[test]
fn an_unregistered_opcode_fails_the_runs_commands() {
    let (mut app, fake) = app();
    fake.push(with_entry(
        "test/bad",
        Box::new(|call| {
            call.commands.spawn(&[])?;
            call.commands.encoder.plugin(4242, b"");
            Ok(())
        }),
    ));
    apply(&mut app);
    let before = app.world().entities().count_spawned();
    plugin_api::call_entry(app.world_mut(), "test/bad", b"").unwrap();
    assert_eq!(app.world().entities().count_spawned(), before);
    let log: Vec<_> = app.world().resource::<bevy_ts::log::ScriptLog>().since(0).map(|e| e.message.clone()).collect();
    assert!(log.iter().any(|m| m.contains("opcode 4242")), "{log:?}");
}

#[test]
fn components_by_name_and_encoding() {
    let (mut app, fake) = app();
    fake.push(FakeProgram { manifest: Manifest { components: vec![health()], ..Default::default() }, ..Default::default() });
    apply(&mut app);
    let (id, layout) = plugin_api::component_by_name(app.world(), "game::Health").unwrap();
    assert_eq!(layout.size, 8);
    let bytes = plugin_api::encode_component(app.world(), "game::Health", json!({"hp": 50}).as_object().unwrap()).unwrap();
    assert_eq!(bytes, [50f32.to_le_bytes(), 100f32.to_le_bytes()].concat());
    let e = plugin_api::encode_component(app.world(), "game::Health", json!({"hp": "x"}).as_object().unwrap()).unwrap_err();
    assert_eq!(e.path, ".hp");
    let _ = id;
}

#[test]
fn reflected_text_is_read_only_when_its_tick_moved() {
    let (mut app, _fake) = app();
    app.register_type::<Text>();
    let e = app.world_mut().spawn(Text::new("hello")).id();
    let id = app.world_mut().register_component::<Text>();
    let calls_before = _fake.calls();
    // The plugin's per-frame check: ticks always, reflection only when they moved.
    let mut last = None;
    let mut reads = Vec::new();
    for frame in 0..4 {
        if frame == 2 {
            app.world_mut().get_mut::<Text>(e).unwrap().0 = "world".into();
        }
        let ticks = plugin_api::change_ticks(app.world(), e, id).unwrap();
        if last != Some(ticks.changed) {
            last = Some(ticks.changed);
            reads.push(plugin_api::read_reflected(app.world(), e, "bevy_ui::widget::text::Text").unwrap());
        }
        app.update();
    }
    assert_eq!(reads, vec![json!("hello"), json!("world")]);
    assert_eq!(_fake.calls(), calls_before, "no call into JS");
    plugin_api::write_reflected(app.world_mut(), e, "bevy_ui::widget::text::Text", &json!("again")).unwrap();
    assert_eq!(app.world().get::<Text>(e).unwrap().0, "again");
}

#[test]
fn restart_notifies_between_despawn_and_first_apply() {
    let (mut app, fake) = app();
    let seen = feature(&mut app);
    fake.push(with_entry(
        "test/spawn",
        Box::new(|call| {
            call.commands.spawn(&[])?;
            Ok(())
        }),
    ));
    apply(&mut app);
    plugin_api::call_entry(app.world_mut(), "test/spawn", b"").unwrap();
    assert_eq!(app.world_mut().query::<&TsSpawned>().iter(app.world()).count(), 1);
    fake.push(FakeProgram::default());
    let r = generation::apply(app.world_mut(), set(), true);
    assert_eq!(r.status, ReloadStatus::Applied);
    assert_eq!(seen.list(), vec!["restart: ts-spawned=0 live=1"]);
    assert_eq!(fake.restarts(), 1);
    assert_eq!(app.world().resource::<Generations>().live, 2);
}
