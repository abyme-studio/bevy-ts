//! Task 3.1 spike: can a BSN `Scene` be built from runtime data in Bevy 0.20, including TS
//! components that exist only as a `ComponentId` and a byte layout? And what does it cost
//! against direct insertion? Run with `--nocapture` for the timings.

use bevy::ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;
use bevy::ptr::OwningPtr;
use bevy::scene::{ResolveContext, ResolveSceneError, ResolvedScene, Scene, SceneEffect, ScenePatch};
use bevy::ecs::bundle::BundleWriter;
use std::alloc::Layout;
use std::ptr::NonNull;
use std::time::Instant;

/// One component as runtime data: an id, its layout and its bytes.
#[derive(Clone)]
struct Raw {
    id: ComponentId,
    layout: Layout,
    bytes: Vec<u8>,
}

impl SceneEffect for Raw {
    fn apply(self, _context: &mut TemplateContext, bundle_writer: &mut BundleWriter) {
        let mut bytes = self.bytes;
        // SAFETY: `bytes` holds a value of the component `id` was registered with (plain data,
        // `layout.size()` bytes); the writer copies it before `bytes` drops.
        unsafe {
            let ptr = OwningPtr::new(NonNull::new_unchecked(bytes.as_mut_ptr()));
            bundle_writer.push_component_by_id(self.id, ptr, self.layout);
        }
    }
}

// `Raw` is `Clone`, so it is its own `Template` (bevy_ecs's blanket impl).

/// An entity description built at runtime.
#[derive(Clone)]
struct RuntimeScene {
    components: Vec<Raw>,
    transform: Option<Transform>,
    children: Vec<RuntimeScene>,
}

impl Scene for RuntimeScene {
    #[expect(clippy::only_used_in_recursion, reason = "BSN threads the context to children")]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> Result<(), ResolveSceneError> {
        for c in self.components {
            scene.push_template(c);
        }
        if let Some(t) = self.transform {
            scene.push_template(t);
        }
        if !self.children.is_empty() {
            let mut resolved = Vec::new();
            for child in self.children {
                let mut r = ResolvedScene::default();
                child.resolve(context, &mut r)?;
                resolved.push(r);
            }
            scene.get_or_insert_related_resolved_scenes::<ChildOf>().scenes.extend(resolved);
        }
        Ok(())
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::scene::ScenePlugin));
    app
}

fn health(world: &mut World) -> ComponentId {
    let layout = Layout::from_size_align(8, 4).unwrap();
    // SAFETY: plain bytes, no drop.
    let d = unsafe {
        ComponentDescriptor::new_with_layout("game::Health", StorageType::Table, layout, None, true, true, ComponentCloneBehavior::Default, None)
    };
    world.register_component_with_descriptor(d)
}

fn enemy(health: ComponentId, hp: f32) -> RuntimeScene {
    let bytes = [hp.to_le_bytes(), hp.to_le_bytes()].concat();
    RuntimeScene {
        components: vec![Raw { id: health, layout: Layout::from_size_align(8, 4).unwrap(), bytes }],
        transform: Some(Transform::from_xyz(1.0, 0.0, 0.0)),
        children: vec![RuntimeScene { components: vec![], transform: Some(Transform::from_xyz(0.0, 0.6, 0.5)), children: vec![] }],
    }
}

#[test]
fn a_runtime_scene_with_a_dynamic_component_spawns_through_bsn() {
    let mut app = app();
    let world = app.world_mut();
    let id = health(world);
    let e = world.spawn_scene(enemy(id, 250.0)).expect("spawns").id();
    let ptr = world.get_by_id(e, id).expect("dynamic component inserted");
    // SAFETY: 8 bytes of plain data.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 8) };
    assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 250.0);
    assert_eq!(world.get::<Transform>(e).unwrap().translation.x, 1.0);
    let children = world.get::<Children>(e).expect("child spawned");
    assert_eq!(children.len(), 1);
}

#[test]
fn a_resolved_patch_spawns_many_times_and_timings() {
    let mut app = app();
    let world = app.world_mut();
    let id = health(world);
    let n = 1000;

    // BSN, resolved once (the static-subtree cache), spawned n times.
    let started = Instant::now();
    let mut patch = ScenePatch::load(world.resource::<AssetServer>(), enemy(id, 100.0));
    patch.resolve(world.resource::<AssetServer>(), world.resource::<Assets<ScenePatch>>()).unwrap();
    for _ in 0..n {
        patch.spawn(world).unwrap();
    }
    let cached = started.elapsed();

    // BSN, resolved per spawn.
    let started = Instant::now();
    for _ in 0..n {
        world.spawn_scene(enemy(id, 100.0)).unwrap();
    }
    let uncached = started.elapsed();

    // Direct insertion: spawn, insert by id, insert Transform, spawn the child.
    let started = Instant::now();
    for _ in 0..n {
        let s = enemy(id, 100.0);
        let mut e = world.spawn(s.transform.unwrap());
        let c = &s.components[0];
        let mut bytes = c.bytes.clone();
        // SAFETY: as above.
        unsafe { e.insert_by_id(c.id, OwningPtr::new(NonNull::new_unchecked(bytes.as_mut_ptr()))) };
        let parent = e.id();
        world.spawn((s.children[0].transform.unwrap(), ChildOf(parent)));
    }
    let direct = started.elapsed();
    println!("1000 enemies (2 entities each): bsn cached {cached:?}, bsn per spawn {uncached:?}, direct {direct:?}");
    assert_eq!(world.query::<&ChildOf>().iter(world).count(), 3 * n);
}
