//! bevy-ts: TypeScript components and systems for Bevy.
//!
//! Rust owns the one `World`. TS declares components (registered as real ECS components with
//! native layouts) and systems (run over zero-copy column views, one call into JS per run), and
//! queues structural changes into a binary command buffer. This crate is engine-free: a JS
//! engine plugs in through [`backend::ScriptBackend`] (`bevy-ts-v8` is the V8 one;
//! [`fake::FakeBackend`] runs tests without any engine).

pub mod backend;
pub mod commands;
pub mod components;
pub mod engine;
pub mod fake;
pub mod generation;
pub mod host;
pub mod layout;
pub mod layout_reflect;
pub mod log;
pub mod manifest;
pub mod memory;
pub mod messages;
pub mod observers;
pub mod plugin_api;
#[cfg(feature = "pbr")]
pub mod recipes;
pub mod runtime;
pub mod sets;
pub mod systems;
pub mod value;

use crate::backend::ScriptBackend;
use crate::manifest::Stage;
use bevy::prelude::*;
use std::sync::Arc;

pub mod prelude {
    pub use crate::backend::{ModuleSet, ScriptBackend, ScriptError};
    pub use crate::manifest::Stage;
    pub use crate::plugin_api::BevyTsAppExt;
    pub use crate::{BevyTsPlugin, BevyTsSet};
}

/// The system set each bevy-ts stage runs in, so Rust plugins can order around it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BevyTsSet {
    PreUpdate,
    Update,
    PostUpdate,
    FixedUpdate,
}

/// Builds the backend on the main thread when the plugin is added.
pub type BackendFactory = Arc<dyn Fn() -> Box<dyn ScriptBackend> + Send + Sync>;

/// Adds the bridge: the runtime as non-send data, the four stages, generation application in
/// `First`, and the native layout registry filled from reflection.
pub struct BevyTsPlugin {
    backend: BackendFactory,
    /// Dev mode: reload, the read-column check, inspector.
    pub dev: bool,
}

impl BevyTsPlugin {
    pub fn new(backend: impl Fn() -> Box<dyn ScriptBackend> + Send + Sync + 'static) -> Self {
        BevyTsPlugin { backend: Arc::new(backend), dev: false }
    }

    pub fn dev(mut self, dev: bool) -> Self {
        self.dev = dev;
        self
    }
}

fn stage_pre_update(world: &mut World) {
    systems::run_stage(world, Stage::PreUpdate);
}
fn stage_update(world: &mut World) {
    systems::run_stage(world, Stage::Update);
}
fn stage_post_update(world: &mut World) {
    systems::run_stage(world, Stage::PostUpdate);
}
fn stage_fixed_update(world: &mut World) {
    systems::run_stage(world, Stage::FixedUpdate);
}

impl Plugin for BevyTsPlugin {
    fn build(&self, app: &mut App) {
        let backend = (self.backend)();
        let engine = backend.engine();
        let inspector = backend.inspector_address();
        app.insert_non_send(runtime::ScriptRuntime::new(backend, self.dev));
        app.register_type::<commands::TsSpawned>();
        app.init_resource::<components::TsComponents>()
            .init_resource::<systems::TsSystems>()
            .init_resource::<commands::CommandKinds>()
            .init_resource::<commands::NativeRecipes>()
            .init_resource::<plugin_api::SectionHandlers>()
            .init_resource::<plugin_api::RestartHandlers>()
            .init_resource::<log::ScriptLog>()
            .init_resource::<generation::ReloadLog>()
            .init_resource::<generation::Generations>()
            .init_resource::<runtime::ScriptClock>()
            .init_resource::<runtime::ScriptFrame>()
            .init_resource::<memory::SystemMemory>()
            .init_resource::<messages::TsPayloads>()
            .init_resource::<observers::ObserverQueue>()
            .init_resource::<host::HostParams>()
            .insert_resource(runtime::RuntimeInfo { dev: self.dev, engine, inspector, ..Default::default() });

        // Rust components' layouts come from reflection once every plugin has registered its
        // types: in `finish`, or when the first generation applies if the app never finishes.
        app.register_type::<Transform>().init_resource::<layout::NativeLayouts>();
        sets::expose_anchors(app);

        #[cfg(feature = "pbr")]
        recipes::register(app);

        app.add_systems(
            First,
            (runtime::advance_clock.before(bevy::time::TimeSystems), generation::apply_pending, runtime::pump).chain(),
        );
        if !app.is_plugin_added::<bevy::state::app::StatesPlugin>() {
            app.add_plugins(bevy::state::app::StatesPlugin);
        }
        app.add_systems(First, messages::update_messages.after(generation::apply_pending));
        app.add_systems(bevy::state::state::StateTransition, systems::run_state_transitions);
        app.configure_sets(PostUpdate, BevyTsSet::PostUpdate.before(bevy::transform::TransformSystems::Propagate));
        app.add_systems(PreUpdate, stage_pre_update.in_set(BevyTsSet::PreUpdate))
            .add_systems(Update, stage_update.in_set(BevyTsSet::Update))
            .add_systems(PostUpdate, stage_post_update.in_set(BevyTsSet::PostUpdate))
            .add_systems(FixedUpdate, stage_fixed_update.in_set(BevyTsSet::FixedUpdate));
    }

    fn finish(&self, app: &mut App) {
        if let Err(e) = app.world().resource::<sets::TsSets>().check() {
            panic!("{e}");
        }
        layout_reflect::fill_once(app.world_mut());
    }
}
