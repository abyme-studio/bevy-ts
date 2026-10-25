//! Type data Bevy 0.20 leaves out of types bevy-ts exposes, registered once through Bevy's
//! registry so TSX, levels, stock BRP and scenes all see it with no special case.
//!
//! Each entry names the upstream file it covers. When Bevy reflects one itself, [`register`]
//! reports it as redundant and the hole check's test fails, so the entry is deleted.

use std::any::TypeId;
use std::ops::{Range, RangeInclusive};

use bevy::prelude::*;
use bevy::reflect::{CreateTypeData, GetTypeRegistration, TypeData, TypeRegistry};

/// The entries [`register`] found Bevy already reflects: each should be deleted.
#[derive(Resource, Default, Debug)]
pub struct RedundantTypeData(pub Vec<&'static str>);

/// Adds every entry to `app`'s registry and records the redundant ones in [`RedundantTypeData`].
/// Runs in `BevyTsPlugin::build`; it registers each type itself, so plugin order does not matter.
pub fn register(app: &mut App) {
    let registry = app.world().resource::<AppTypeRegistry>().clone();
    let mut r = registry.write();
    let mut redundant = Vec::new();
    defaults(&mut r, &mut redundant);
    ranges(&mut r, &mut redundant);
    drop(r);
    app.insert_resource(RedundantTypeData(redundant));
}

/// `ReflectDefault` for components that implement `Default` but do not reflect it.
#[cfg_attr(not(feature = "pbr"), allow(unused_variables))]
fn defaults(r: &mut TypeRegistry, redundant: &mut Vec<&'static str>) {
    #[cfg(feature = "pbr")]
    {
        // bevy_camera 0.20 src/visibility/range.rs:80
        with_default::<bevy::camera::visibility::VisibilityRange>(r, redundant);
        // bevy_camera 0.20 src/camera.rs:893 (Default at :1039)
        with_default::<bevy::camera::RenderTarget>(r, redundant);
        // bevy_text 0.20 src/text.rs:1349 (Default at :1366)
        with_default::<bevy::text::LineHeight>(r, redundant);
    }
    #[cfg(feature = "engine_type_data")]
    {
        use bevy::ui_widgets::*;
        // bevy_gltf 0.20 src/assets.rs:296, 321, 346
        with_default::<bevy::gltf::GltfSceneName>(r, redundant);
        with_default::<bevy::gltf::GltfMeshName>(r, redundant);
        with_default::<bevy::gltf::GltfMaterialName>(r, redundant);
        // bevy_ui_render 0.20 src/debug_overlay.rs:41 (Default at :70)
        with_default::<bevy::ui_render::UiDebugOptions>(r, redundant);
        // bevy_ui_widgets 0.20 src/button.rs:31, 37
        with_default::<Button>(r, redundant);
        with_default::<ActivateOnPress>(r, redundant);
        // src/checkbox.rs:44
        with_default::<Checkbox>(r, redundant);
        // src/dialog.rs:36, 58
        with_default::<Dialog>(r, redundant);
        with_default::<DialogDragHandle>(r, redundant);
        // src/list.rs:49, 60
        with_default::<ListItem>(r, redundant);
        with_default::<ActiveDescendant>(r, redundant);
        // src/menu.rs:128, 138, 427
        with_default::<MenuPopup>(r, redundant);
        with_default::<MenuItem>(r, redundant);
        with_default::<MenuButton>(r, redundant);
        // src/modal.rs:32, 37
        with_default::<ModalDialog>(r, redundant);
        with_default::<ModalDialogBarrier>(r, redundant);
        // src/popover.rs:86
        with_default::<popover::Popover>(r, redundant);
        // src/radio.rs:48, 66
        with_default::<RadioGroup>(r, redundant);
        with_default::<RadioButton>(r, redundant);
        // src/scrollarea.rs:23
        with_default::<ScrollArea>(r, redundant);
        // src/scrollbar.rs:104
        with_default::<ScrollbarThumb>(r, redundant);
        // src/slider.rs:115, 125, 132, 139, 226
        with_default::<Slider>(r, redundant);
        with_default::<SliderThumb>(r, redundant);
        with_default::<SliderValue>(r, redundant);
        with_default::<SliderRange>(r, redundant);
        with_default::<SliderStep>(r, redundant);
        // src/text_input.rs:73, 685
        with_default::<TextInput>(r, redundant);
        with_default::<SelectAllOnFocus>(r, redundant);
    }
}

/// `ReflectSerialize` and `ReflectDeserialize` for `Range<N>` and `RangeInclusive<N>` of every
/// numeric `N`: bevy_reflect 0.20 src/impls/core/ops.rs:3 reflects them opaque with `Clone` only.
/// serde's form is `{ start, end }`, the JSON shape `reflect_value` reads and writes.
fn ranges(r: &mut TypeRegistry, redundant: &mut Vec<&'static str>) {
    macro_rules! each {
        ($($n:ty),*) => {$(
            with_serde::<Range<$n>>(r, redundant);
            with_serde::<RangeInclusive<$n>>(r, redundant);
        )*};
    }
    each!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
}

#[cfg_attr(not(feature = "pbr"), allow(dead_code))]
fn with_default<T: Reflect + TypePath + GetTypeRegistration + Default>(r: &mut TypeRegistry, redundant: &mut Vec<&'static str>) {
    r.register::<T>();
    add::<T, ReflectDefault>(r, redundant);
}

fn with_serde<T>(r: &mut TypeRegistry, redundant: &mut Vec<&'static str>)
where
    T: bevy::reflect::FromReflect + TypePath + GetTypeRegistration + serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    r.register::<T>();
    add::<T, bevy::reflect::ReflectSerialize>(r, redundant);
    add::<T, bevy::reflect::ReflectDeserialize>(r, redundant);
}

/// Inserts `D` for the registered `T` when it lacks it; notes `T` when Bevy already has it.
fn add<T: TypePath + 'static, D: TypeData + CreateTypeData<T>>(r: &mut TypeRegistry, redundant: &mut Vec<&'static str>) {
    let reg = r.get_mut(TypeId::of::<T>()).expect("registered first");
    match reg.data::<D>() {
        Some(_) => redundant.push(T::type_path()),
        None => reg.insert(<D as CreateTypeData<T>>::create_type_data(())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry is still missing upstream: when Bevy reflects one itself, this fails and the
    /// entry is deleted. `tests/it/reflect_coverage.rs` checks the same on the prebuilt engine.
    #[test]
    fn every_entry_is_missing_upstream() {
        let mut app = App::new();
        register(&mut app);
        assert_eq!(app.world().resource::<RedundantTypeData>().0, Vec::<&str>::new());
    }

    #[test]
    fn visibility_range_and_ranges_get_their_data() {
        let mut app = App::new();
        register(&mut app);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        #[cfg(feature = "pbr")]
        assert!(registry.get_type_data::<ReflectDefault>(TypeId::of::<bevy::camera::visibility::VisibilityRange>()).is_some());
        assert!(registry.get_type_data::<bevy::reflect::ReflectDeserialize>(TypeId::of::<Range<f32>>()).is_some());
        assert!(registry.get_type_data::<bevy::reflect::ReflectSerialize>(TypeId::of::<RangeInclusive<usize>>()).is_some());
    }
}
