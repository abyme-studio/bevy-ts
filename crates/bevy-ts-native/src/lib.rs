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

use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
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
/// ```
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

/// A param `#[export]` made callable from TS.
///
/// A param that writes cannot be exported:
/// ```compile_fail
/// use bevy::ecs::system::SystemParam;
/// use bevy_ts_native::prelude::*;
/// #[derive(Resource)]
/// pub struct NavMesh(f32);
/// #[derive(SystemParam)]
/// pub struct Nav<'w> { mesh: ResMut<'w, NavMesh> }
/// #[export(ts = "Nav", module = "bevy-ts/game")]
/// impl Nav<'_> {
///     pub fn cost(&self) -> f32 { self.mesh.0 }
/// }
/// fn main() { App::new().ts_param::<Nav<'static>>(); }
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
/// ``` The param must be read-only: TS calls it in the
/// middle of a system run, with live column views.
pub trait ExportParam: ReadOnlyParam {
    fn spec() -> bevy_ts::host::ParamSpec;
}

/// A read-only `SystemParam`, the only kind TS may call.
#[diagnostic::on_unimplemented(
    message = "`{Self}` must be a read-only system param to be exported to TS",
    note = "exported params run in the middle of a TS system, which holds live column views; use Res, Query<&T> and read-only params, never ResMut or Query<&mut T>"
)]
pub trait ReadOnlyParam: bevy::ecs::system::ReadOnlySystemParam + 'static {}

impl<P: bevy::ecs::system::ReadOnlySystemParam + 'static> ReadOnlyParam for P {}

/// What a native crate or port registers for TS, on `App`, as Bevy's `register_type` is.
pub trait BevyTsNativeExt {
    /// Registers a [`NativeLayout`] component as a column for TS systems, with zeroed defaults.
    fn ts_layout<T: NativeLayout>(&mut self) -> &mut Self;

    /// Exposes `set` in `schedule` to TS as `name` (`"rapier:step"`), so TS systems can order
    /// before or after it. Names are `<crate>:<name>` and unique per engine.
    #[track_caller]
    fn ts_set(&mut self, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) -> &mut Self;

    /// Exports a param TS systems can take (`rapier: ReadRapierContext`) and call. Its methods
    /// come from `#[export]` on its `impl`.
    fn ts_param<P: ExportParam>(&mut self) -> &mut Self;
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

    #[track_caller]
    fn ts_set(&mut self, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) -> &mut Self {
        bevy_ts::sets::expose(self, name, schedule, set);
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
