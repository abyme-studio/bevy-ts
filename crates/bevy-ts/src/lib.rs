//! bevy-ts: TypeScript components and systems for Bevy.
//!
//! Rust owns the one `World`. TS declares components (registered as real ECS components with
//! native layouts) and systems (run over zero-copy column views, one call into JS per run), and
//! queues structural changes into a binary command buffer. This crate is engine-free: a JS
//! engine plugs in through [`backend::ScriptBackend`] (`bevy-ts-v8` is the V8 one;
//! [`fake::FakeBackend`] runs tests without any engine).

pub mod backend;
pub mod bevy_system;
pub mod color;
pub mod commands;
pub mod components;
pub mod conditions;
pub mod engine;
pub mod fake;
pub mod generation;
pub mod host;
pub mod keys;
pub mod layout;
pub mod layout_reflect;
pub mod log;
pub mod manifest;
pub mod memory;
pub mod messages;
pub mod modules;
pub mod net;
pub mod observers;
pub mod plugin_api;
#[cfg(feature = "pbr")]
pub mod recipes;
#[cfg(feature = "pbr")]
pub mod reflect_value;
pub mod runtime;
pub mod schedules;
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
    pub use crate::schedules::{RunTsSystem, TsSchedule};
    pub use crate::{BevyTsPlugin, BevyTsSet, EngineMode, ScriptHidden};
}

/// Marks an entity TS never sees: every TS query skips it, TS lifecycle observers ignore it, and
/// `bevy-ts/entities` leaves it out. For editor-only entities (viewport's editor camera, gizmos).
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component, Default)]
pub struct ScriptHidden;

/// The image a windowless run (`--headless`) uses as its window: its cameras render there. A
/// camera rendering to it counts as on the window, so `<viewport camera>` gives such a camera an
/// image of its own, as it does a camera on a real window.
#[derive(Resource, Clone, Debug)]
pub struct WindowImage(pub Handle<Image>);

/// Whether the game runs (`Play`) or the engine is an editor over a still world (`Edit`, from
/// `bevy-ts dev --edit` or the viewport's edit mode). A shipped game is always `Play`. Native
/// systems that must not run in edit mode gate with `run_if(in_state(EngineMode::Play))`.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineMode {
    #[default]
    Play,
    Edit,
}

/// `First`, in edit mode: drops queued TS triggers and lifecycle events, so an edit fires no game
/// code, now or on play.
fn drop_edit_triggers(mut queue: ResMut<observers::ObserverQueue>) {
    queue.0.clear();
}

/// The system set each bevy-ts stage's TS systems run in, each as a Bevy system of its own, so
/// Rust plugins can order around them. A custom schedule's stage runs in `Schedule(path)`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BevyTsSet {
    First,
    PreUpdate,
    RunFixedMainLoop,
    Update,
    SpawnScene,
    PostUpdate,
    Last,
    FixedFirst,
    FixedPreUpdate,
    FixedUpdate,
    FixedPostUpdate,
    FixedLast,
    Schedule(&'static str),
}

/// Builds the backend on the main thread when the plugin is added.
pub type BackendFactory = Arc<dyn Fn() -> Box<dyn ScriptBackend> + Send + Sync>;

/// Adds the bridge: the runtime as non-send data, the four eager stages' sets, generation
/// application in `First`, and the native layout registry filled from reflection.
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

impl Plugin for BevyTsPlugin {
    fn build(&self, app: &mut App) {
        let backend = (self.backend)();
        let engine = backend.engine();
        let inspector = backend.inspector_address();
        app.insert_non_send(runtime::ScriptRuntime::new(backend, self.dev));
        app.insert_non_send(net::NetRegistry::new(Box::new(net::SocketTransport::default())));
        app.register_type::<commands::TsSpawned>().register_type::<ScriptHidden>();
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
            .init_resource::<conditions::ConditionMemory>()
            .init_resource::<schedules::MadeStages>()
            .init_resource::<schedules::DeferredEdits>()
            .init_resource::<schedules::PendingRuns>()
            .init_resource::<schedules::OrderState>()
            .init_resource::<messages::TsPayloads>()
            .init_resource::<observers::ObserverQueue>()
            .init_resource::<host::HostParams>()
            .init_resource::<net::NetPolicy>()
            .insert_resource(runtime::RuntimeInfo { dev: self.dev, engine, inspector, ..Default::default() });

        // Rust components' layouts come from reflection once every plugin has registered its
        // types: in `finish`, or when the first generation applies if the app never finishes.
        app.register_type::<Transform>().init_resource::<layout::NativeLayouts>();
        sets::expose_anchors(app);

        #[cfg(feature = "pbr")]
        recipes::register(app);

        app.add_systems(
            First,
            (
                runtime::advance_clock.before(bevy::time::TimeSystems),
                generation::apply_pending,
                net::net_deliver.after(bevy::time::TimeSystems).run_if(in_state(EngineMode::Play)),
                runtime::pump,
            )
                .chain(),
        );
        if !app.is_plugin_added::<bevy::state::app::StatesPlugin>() {
            app.add_plugins(bevy::state::app::StatesPlugin);
        }
        app.init_state::<EngineMode>();
        app.add_systems(OnEnter(EngineMode::Edit), net::close_for_edit);
        // The state changes after `First`: deliver on entering play too, so what waited (the closes
        // edit mode made, due timers) arrives in the first frame of play.
        app.add_systems(OnEnter(EngineMode::Play), (net::net_deliver, runtime::pump).chain());
        app.world_mut().resource_mut::<plugin_api::RestartHandlers>().0.push(Arc::new(net::close_all));
        app.add_systems(First, drop_edit_triggers.after(generation::apply_pending).run_if(in_state(EngineMode::Edit)));
        app.add_systems(First, messages::update_messages.after(generation::apply_pending));
        app.add_systems(bevy::state::state::StateTransition, systems::run_state_transitions);
        // A stage made while its schedule runs (`First`, by the generation applied in it) is added
        // when that schedule ends.
        app.add_systems(First, schedules::apply_deferred_edits.before(runtime::advance_clock));
        app.add_systems(Last, schedules::apply_deferred_edits);
        // `Main` holds the schedule orders while it runs: a generation's order edits wait for it.
        app.add_systems(Main, schedules::apply_pending_order.after(Main::run_main));
        // The eager stages' sets exist from startup; a generation adds its TS systems to them.
        app.init_resource::<bevy_system::LiveSystems>();
        for stage in Stage::ALL.into_iter().filter(|s| s.eager()) {
            schedules::configure_stage(&mut app.world_mut().resource_mut::<Schedules>(), stage);
        }
        // Errors TS systems left on the runtime are on the log when the frame ends.
        app.add_systems(Last, runtime::flush_log_system.after(BevyTsSet::Last));
    }

    fn finish(&self, app: &mut App) {
        if let Err(e) = app.world().resource::<sets::TsSets>().check() {
            panic!("{e}");
        }
        if let Some(Err(e)) = app.world().get_resource::<schedules::TsSchedules>().map(|s| s.check()) {
            panic!("{e}");
        }
        if let Some(Err(e)) = app.world().get_resource::<conditions::TsConditions>().map(|s| s.check()) {
            panic!("{e}");
        }
        layout_reflect::fill_once(app.world_mut());
    }
}
