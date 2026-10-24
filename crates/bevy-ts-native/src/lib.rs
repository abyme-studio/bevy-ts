//! The API a bevy-ts project's native crate is written against.
//!
//! A native crate is a normal Cargo library that depends on this crate at the exact version of
//! the `bevy-ts` npm package (`bevy-ts-native = "=x.y.z"`) and holds one Bevy plugin, named in
//! `bevy-ts.json`:
//!
//! ```jsonc
//! "native": { "crate": "native", "plugin": "native::GamePlugin" }
//! ```
//!
//! bevy-ts generates a project engine whose `main` runs the prebuilt engine's entry
//! (`bevy_ts_cli::run`) with that plugin added, and links the crate statically. Depend on `bevy`
//! by name at exactly the Bevy version this release was built with (`bevy = "=0.20.0"`): Bevy's
//! derives find it by that name, and bevy-ts checks `Cargo.lock` for the version.

pub use bevy;
pub use bevy_ts;
pub use bevy_ts::layout;
pub use bevy_ts_native_derive::{HostRecord, NativeLayout, export};

use bevy::ecs::event::GlobalTrigger;
use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
use bevy::reflect::GetTypeRegistration;
use bevy::prelude::*;
use bevy_ts::layout::{ComponentLayout, FieldType, NativeLayouts};

pub use bevy_ts::sets::BevyTsSlot;

pub mod prelude {
    pub use crate::{BevyTsNativeExt, BevyTsSlot, HostRecord, NativeLayout, export};
    pub use bevy::prelude::*;
    pub use bevy_ts::prelude::*;
}

/// A field type a native layout can hold: the bridge field types.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a bevy-ts field type",
    label = "not plain data TS can read in place",
    note = "native layout fields are f32, f64, i8..u32, bool, Entity, Vec2, Vec3, Vec4, Quat and arrays of numbers or vectors; keep heap data (String, Vec, Handle) in a component with Reflect"
)]
pub trait NativeField {
    fn field_type() -> FieldType;
}

macro_rules! fields {
    ($($t:ty => $f:expr),* $(,)?) => {
        $(impl NativeField for $t {
            fn field_type() -> FieldType {
                $f
            }
        })*
    };
}

fields! {
    f32 => FieldType::F32,
    f64 => FieldType::F64,
    i8 => FieldType::I8,
    u8 => FieldType::U8,
    i16 => FieldType::I16,
    u16 => FieldType::U16,
    i32 => FieldType::I32,
    u32 => FieldType::U32,
    bool => FieldType::Bool,
    Entity => FieldType::Entity,
    Vec2 => FieldType::Vec2,
    Vec3 => FieldType::Vec3,
    Vec4 => FieldType::Vec4,
    Quat => FieldType::Quat,
}

impl<T: NativeField, const N: usize> NativeField for [T; N] {
    fn field_type() -> FieldType {
        FieldType::Array { of: Box::new(T::field_type()), len: N as u32 }
    }
}

/// A component with a layout from `offset_of!`, for types without `Reflect`. Derive it:
///
/// ```no_run,standalone_crate
/// use bevy_ts_native::prelude::*;
///
/// #[derive(Component, NativeLayout)]
/// pub struct Fuel { pub level: f32, pub burning: bool }
/// ```
///
/// Heap data has no layout:
/// ```compile_fail
/// use bevy_ts_native::prelude::*;
/// #[derive(Component, NativeLayout)]
/// pub struct Named { pub name: String }
/// ```
/// ```compile_fail
/// use bevy_ts_native::prelude::*;
/// #[derive(Component, NativeLayout)]
/// pub struct Skin { pub mesh: Handle<Mesh> }
/// ```
/// Every field must be public:
/// ```compile_fail
/// use bevy_ts_native::prelude::*;
/// #[derive(Component, NativeLayout)]
/// pub struct Secret { hidden: f32 }
/// ```
pub trait NativeLayout: Component {
    fn native_layout() -> ComponentLayout;
}

/// A param `#[export]` made callable from TS. It may write (`ResMut`, `Query<&mut T>`) and
/// defer (`Commands`, `Gizmos`); see `bevy_ts::host` for when writes land.
///
/// A param that writes exports:
/// ```no_run,standalone_crate
/// use bevy::ecs::system::SystemParam;
/// use bevy_ts_native::prelude::*;
/// #[derive(Resource)]
/// pub struct NavMesh(Vec<bool>);
/// #[derive(SystemParam)]
/// pub struct NavEdit<'w> { mesh: ResMut<'w, NavMesh> }
/// #[export(ts = "NavEdit", module = "bevy-ts/game")]
/// impl NavEdit<'_> {
///     pub fn block(&mut self, cell: u32) { self.mesh.0[cell as usize] = true }
/// }
/// fn main() { App::new().insert_resource(NavMesh(vec![false; 8])).ts_param::<NavEdit<'static>>(); }
/// ```
/// Something that is not a `SystemParam`, such as `&mut World`, does not:
/// ```compile_fail
/// use bevy_ts_native::prelude::*;
/// pub struct Everything;
/// #[export(ts = "Everything", module = "bevy-ts/game")]
/// impl Everything {
///     pub fn count(&self) -> u32 { 0 }
/// }
/// fn main() { App::new().ts_param::<&'static mut World>(); }
/// ```
/// A method argument that cannot cross fails naming its type:
/// ```compile_fail
/// use bevy::ecs::system::SystemParam;
/// use bevy_ts_native::prelude::*;
/// #[derive(SystemParam)]
/// pub struct Names<'w> { time: Res<'w, Time> }
/// #[export(ts = "Names", module = "bevy-ts/game")]
/// impl Names<'_> {
///     pub fn find(&self, name: String) -> bool { name.is_empty() }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` must be a system param to be exported to TS",
    note = "export a SystemParam (Res, ResMut, Query, Commands, a #[derive(SystemParam)] struct); &mut World and exclusive params cannot be called during a TS run"
)]
pub trait ExportParam: bevy::ecs::system::SystemParam + 'static {
    fn spec() -> bevy_ts::host::ParamSpec;
}

/// What a native crate or port registers for TS, on `App`, as Bevy's `register_type` is.
pub trait BevyTsNativeExt {
    /// Registers a [`NativeLayout`] component as a column for TS systems, with zeroed defaults.
    fn ts_layout<T: NativeLayout>(&mut self) -> &mut Self;

    /// Registers a [`NativeLayout`] resource for TS systems' `Res<T>`/`ResMut<T>`, with zeroed
    /// defaults: the resource sibling of [`Self::ts_layout`]. Reflected resources need nothing.
    fn ts_resource_layout<T: NativeLayout + Resource>(&mut self) -> &mut Self;

    /// Exposes `set` in `schedule` to TS as `name` (`"rapier:step"`), so TS systems can order
    /// before or after it. Names are `<crate>:<name>` and unique per engine.
    #[track_caller]
    fn ts_set(&mut self, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) -> &mut Self;

    /// Exposes a schedule label to TS as `name` (`"net:tick"`): TS adds systems to it with
    /// `addSystems(schedule(name), ...)` and runs it with `commands.runSchedule`. Names are
    /// `<crate>:<name>` and unique per engine.
    #[track_caller]
    fn ts_schedule(&mut self, name: &str, label: impl ScheduleLabel) -> &mut Self;

    /// Exposes a run condition to TS as `name` (`"time:on-timer"`): `factory` builds the
    /// condition from the JSON arguments of `nativeCondition(name, args)`, once per use per
    /// generation; an error fails the load. Names are `<crate>:<name>` and unique per engine.
    #[track_caller]
    fn ts_condition(
        &mut self,
        name: &str,
        factory: impl Fn(&bevy_ts::conditions::ConditionArgs) -> Result<bevy::ecs::schedule::BoxedCondition, String> + Send + Sync + 'static,
    ) -> &mut Self;

    /// Exports a param TS systems can take (`rapier: ReadRapierContext`) and call. Its methods
    /// come from `#[export]` on its `impl`.
    fn ts_param<P: ExportParam>(&mut self) -> &mut Self;

    /// Ships a built-in TS module (`bevy-ts/rapier`): `js` is served to TS under the specifier,
    /// `dts` declares it in `native.d.ts`. Classes it defines with `__bevyts.data` are declared
    /// by every generation.
    fn ts_module(&mut self, specifier: &str, js: &str, dts: &str) -> &mut Self;

    /// Lets TS read (`MessageReader<M>`) and write (`MessageWriter<M>`) the Rust message `M`, by
    /// its reflect type path, payloads as JSON. Adds the message if it is not yet. The TS side
    /// lands with the rest of openspec rust-events; until then this only registers the type data.
    fn ts_message<M>(&mut self) -> &mut Self
    where
        M: Message + FromReflect + TypePath + GetTypeRegistration;

    /// Lets TS observe (`On<E>`) and trigger the Rust global event `E`. The TS side lands with the
    /// rest of openspec rust-events.
    fn ts_event<E>(&mut self) -> &mut Self
    where
        E: for<'a> Event<Trigger<'a> = GlobalTrigger> + Reflect + FromReflect + TypePath + GetTypeRegistration;

    /// Lets TS observe (`On<E>`, `entity(e).observe`) and trigger the Rust entity event `E`,
    /// bubbling as Bevy does. The TS side lands with the rest of openspec rust-events.
    fn ts_entity_event<E>(&mut self) -> &mut Self
    where
        E: EntityEvent + Reflect + FromReflect + TypePath + GetTypeRegistration,
        for<'a> E::Trigger<'a>: bevy_ts::rust_events::OriginalTarget + Default;
}

impl BevyTsNativeExt for App {
    fn ts_layout<T: NativeLayout>(&mut self) -> &mut Self {
        let layout = T::native_layout();
        let world = self.world_mut();
        let mut native = world.remove_resource::<NativeLayouts>().unwrap_or_default();
        native.register::<T>(world, layout, None);
        world.insert_resource(native);
        self
    }

    fn ts_resource_layout<T: NativeLayout + Resource>(&mut self) -> &mut Self {
        let layout = T::native_layout();
        let world = self.world_mut();
        let mut native = world.remove_resource::<NativeLayouts>().unwrap_or_default();
        native.register_resource::<T>(world, layout, None);
        world.insert_resource(native);
        self
    }

    #[track_caller]
    fn ts_set(&mut self, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) -> &mut Self {
        bevy_ts::sets::expose(self, name, schedule, set);
        self
    }

    #[track_caller]
    fn ts_schedule(&mut self, name: &str, label: impl ScheduleLabel) -> &mut Self {
        bevy_ts::schedules::expose(self, name, label);
        self
    }

    #[track_caller]
    fn ts_condition(
        &mut self,
        name: &str,
        factory: impl Fn(&bevy_ts::conditions::ConditionArgs) -> Result<bevy::ecs::schedule::BoxedCondition, String> + Send + Sync + 'static,
    ) -> &mut Self {
        bevy_ts::conditions::expose(self, name, factory);
        self
    }

    fn ts_module(&mut self, specifier: &str, js: &str, dts: &str) -> &mut Self {
        if let Err(e) = bevy_ts::modules::register(specifier, js, dts) {
            panic!("{e}");
        }
        self
    }

    fn ts_message<M>(&mut self) -> &mut Self
    where
        M: Message + FromReflect + TypePath + GetTypeRegistration,
    {
        bevy_ts::rust_events::register_message::<M>(self);
        self
    }

    fn ts_event<E>(&mut self) -> &mut Self
    where
        E: for<'a> Event<Trigger<'a> = GlobalTrigger> + Reflect + FromReflect + TypePath + GetTypeRegistration,
    {
        bevy_ts::rust_events::register_event::<E>(self);
        self
    }

    fn ts_entity_event<E>(&mut self) -> &mut Self
    where
        E: EntityEvent + Reflect + FromReflect + TypePath + GetTypeRegistration,
        for<'a> E::Trigger<'a>: bevy_ts::rust_events::OriginalTarget + Default,
    {
        bevy_ts::rust_events::register_entity_event::<E>(self);
        self
    }

    fn ts_param<P: ExportParam>(&mut self) -> &mut Self {
        let world = self.world_mut();
        let mut params = world.remove_resource::<bevy_ts::host::HostParams>().unwrap_or_default();
        let r = params.register(world, P::spec());
        world.insert_resource(params);
        if let Err(e) = r {
            panic!("{e}");
        }
        self
    }
}
