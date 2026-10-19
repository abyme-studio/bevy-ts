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
pub use bevy_ts_native_derive::NativeLayout;

use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
use bevy::prelude::*;
use bevy_ts::layout::{ComponentLayout, FieldType, NativeLayouts};

pub use bevy_ts::sets::BevyTsSlot;

pub mod prelude {
    pub use crate::{BevyTsNativeExt, BevyTsSlot, NativeLayout};
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

/// What a native crate or port registers for TS, on `App`, as Bevy's `register_type` is.
pub trait BevyTsNativeExt {
    /// Registers a [`NativeLayout`] component as a column for TS systems, with zeroed defaults.
    fn ts_layout<T: NativeLayout>(&mut self) -> &mut Self;

    /// Exposes `set` in `schedule` to TS as `name` (`"rapier:step"`), so TS systems can order
    /// before or after it. Names are `<crate>:<name>` and unique per engine.
    #[track_caller]
    fn ts_set(&mut self, name: &str, schedule: impl ScheduleLabel, set: impl SystemSet) -> &mut Self;
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
}
